//! [`Run<R, V>`]: a logical type for run-end-encoded (run-length) columns.
//!
//! Run-end encoding stores a *run* of consecutive equal values once, together
//! with the logical index at which the run ends — a big space win for columns
//! with long stretches of repeated values. Stored as an
//! [`arrow::array::RunArray`] ([`DataType::RunEndEncoded`]).
//!
//! Like [`Dictionary`](crate::Dictionary), `Run<R, V>` is logically *a column of
//! `V`*: the encoding is a storage detail, and the element values are those of
//! `V`, looked up through the run ends. `R` is the run-end index type
//! (`i16`, `i32`, or `i64`) — a space/size trade-off, never user-visible.

use std::marker::PhantomData;

use arrow::array::{Array, ArrayRef, RunArray};
use arrow::datatypes::{DataType, Field};

use crate::datatype::{ColumnError, LogicalType, RefType, downcast_array};

/// Marker for an arrow run-end-encoded column, e.g. `Run<i32, Utf8>`.
///
/// Think of `Run<R, V>` as *a column of `V`, run-length-compressed*: the element
/// values are those of `V`, looked up through the run ends.
///
/// # Nullability
/// A run array has no row-validity buffer of its own; nulls live in its *values*.
/// So nullable rows are `Run<R, Option<V>>` (a null run value is a null row) —
/// `Option<Run<R, V>>` is not the way to express it.
///
/// ```
/// use quiver::{Column, Run, Utf8};
///
/// // Consecutive duplicates collapse into runs (building can fail on overflow):
/// let column = Column::<Run<i32, Utf8>>::try_from_values(["a", "a", "a", "b"]).unwrap();
/// assert_eq!(column.value(0), "a");
/// assert_eq!(column.to_vec(), ["a", "a", "a", "b"]);
/// ```
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct Run<R, V> {
    _marker: PhantomData<fn() -> (R, V)>,
}

/// A logical type usable as a [`Run`] end-index: `i16`, `i32`, or `i64`.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be used as a run-end index type",
    label = "run-end indices must be one of `i16`, `i32`, `i64`"
)]
pub trait RunEndType: crate::ConcreteType {
    /// The corresponding arrow run-end type, e.g. `Int32Type`.
    type ArrowRunType: arrow::datatypes::RunEndIndexType;
}

macro_rules! impl_run_end_type {
    ($rust:ty, $arrow:ty) => {
        impl RunEndType for $rust {
            type ArrowRunType = $arrow;
        }
    };
}

impl_run_end_type!(i16, arrow::datatypes::Int16Type);
impl_run_end_type!(i32, arrow::datatypes::Int32Type);
impl_run_end_type!(i64, arrow::datatypes::Int64Type);

/// The validated representation of a `Run` column:
/// the run array plus its downcast values.
pub struct TypedRun<R: RunEndType, V: LogicalType> {
    run: RunArray<R::ArrowRunType>,
    values: V::Typed,
}

impl<R: RunEndType, V: LogicalType> Clone for TypedRun<R, V> {
    fn clone(&self) -> Self {
        Self {
            run: self.run.clone(),
            values: self.values.clone(),
        }
    }
}

impl<R: RunEndType + 'static, V: LogicalType + 'static> LogicalType for Run<R, V> {
    type Typed = TypedRun<R, V>;
    type Value<'a> = V::Value<'a>;
    type Owned = V::Owned;

    fn matches(actual: &DataType) -> bool {
        match actual {
            DataType::RunEndEncoded(run_ends, values) => {
                R::matches(run_ends.data_type()) && V::matches(values.data_type())
            }
            _ => false,
        }
    }

    fn supported_datatypes() -> Vec<DataType> {
        V::supported_datatypes()
            .into_iter()
            .map(|value| {
                DataType::RunEndEncoded(
                    std::sync::Arc::new(Field::new("run_ends", R::datatype(), false)),
                    std::sync::Arc::new(Field::new("values", value, V::NULLABLE)),
                )
            })
            .collect()
    }

    fn downcast(array: &dyn Array) -> Result<Self::Typed, ColumnError> {
        let run = downcast_array::<RunArray<R::ArrowRunType>>(array)?;
        if !V::NULLABLE {
            // `logical_nulls` expands the runs to logical positions and counts
            // only the *reachable* nulls (respecting any slice window), so this
            // is the logical null count, like for lists and dictionaries.
            let null_count = run.logical_nulls().map_or(0, |nulls| nulls.null_count());
            if 0 < null_count {
                return Err(ColumnError::UnexpectedNulls { null_count });
            }
        }
        let values = V::downcast(&**run.values())?;
        Ok(TypedRun { run, values })
    }

    fn is_null(typed: &Self::Typed, index: usize) -> bool {
        let physical = typed.run.get_physical_index(index);
        V::is_null(&typed.values, physical)
    }

    fn value(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
        let physical = typed.run.get_physical_index(index);
        V::value(&typed.values, physical)
    }

    fn to_owned_value(value: Self::Value<'_>) -> Self::Owned {
        V::to_owned_value(value)
    }
}

impl<R: RunEndType + 'static, V: crate::ConcreteType + 'static> crate::ConcreteType for Run<R, V> {
    fn datatype() -> DataType {
        DataType::RunEndEncoded(
            std::sync::Arc::new(Field::new("run_ends", R::datatype(), false)),
            std::sync::Arc::new(Field::new("values", V::datatype(), V::NULLABLE)),
        )
    }

    fn build(values: impl Iterator<Item = Option<Self::Owned>>) -> Result<ArrayRef, ColumnError> {
        let plain = V::build(values)?;
        // This can fail on run-end overflow: more logical rows than `R` can index
        // (e.g. more than 32767 for `i16`). Hence `Run` is NOT `InfallibleBuild`.
        arrow::compute::cast(&plain, &Self::datatype()).map_err(ColumnError::Build)
    }
}

/// References are looked up through the run ends, like [`LogicalType::value`].
impl<R: RunEndType + 'static, V: RefType + 'static> RefType for Run<R, V> {
    type Ref = V::Ref;

    fn value_ref(typed: &Self::Typed, index: usize) -> &Self::Ref {
        let physical = typed.run.get_physical_index(index);
        V::value_ref(&typed.values, physical)
    }
}

/// `vec.try_into()` support for run-end-encoded columns,
/// whose building is fallible (run-end overflow) — see
/// [`crate::Column::try_from_values`].
impl<R, V, T> TryFrom<Vec<T>> for crate::Column<Run<R, V>>
where
    R: RunEndType + 'static,
    V: crate::ConcreteType + 'static,
    T: Into<V::Owned>,
{
    type Error = ColumnError;

    fn try_from(values: Vec<T>) -> Result<Self, Self::Error> {
        Self::try_from_values(values)
    }
}
