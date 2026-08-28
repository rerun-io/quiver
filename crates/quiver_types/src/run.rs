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

use crate::data_type::{ColumnError, LogicalType, RefType, downcast_array};

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
/// use quiver::{Run, TypedArray, Utf8};
///
/// // Consecutive duplicates collapse into runs (building can fail on overflow):
/// let array = TypedArray::<Run<i32, Utf8>>::try_from_values(["a", "a", "a", "b"]).unwrap();
/// assert_eq!(array.value(0), "a");
/// assert_eq!(array.to_vec(), ["a", "a", "a", "b"]);
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
    type Optional = Option<Self>;
    type Required = Self;

    fn downcast(array: &dyn Array) -> Result<Self::Typed, ColumnError> {
        // `downcast_array` checks the run-end index type (it's part of
        // `RunArray<R::ArrowRunType>`'s Rust type); the value type is validated
        // below by recursing into `V`.
        let run = downcast_array::<RunArray<R::ArrowRunType>>(array, || {
            format!("RunEndEncoded({:?}, …)", R::data_type())
        })?;
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

    fn slice_typed(typed: &Self::Typed, offset: usize, length: usize) -> Option<Self::Typed> {
        // Slicing moves the logical window; the run ends and values are shared
        // whole, and `get_physical_index` accounts for the offset — so the
        // values' view carries over.
        Some(TypedRun {
            run: typed.run.slice(offset, length),
            values: typed.values.clone(),
        })
    }

    #[inline]
    fn is_null(typed: &Self::Typed, index: usize) -> bool {
        let physical = typed.run.get_physical_index(index);
        V::is_null(&typed.values, physical)
    }

    #[inline]
    unsafe fn is_null_unchecked(typed: &Self::Typed, index: usize) -> bool {
        // `get_physical_index` maps an in-bounds logical index to an in-bounds
        // physical one.
        let physical = typed.run.get_physical_index(index);
        // SAFETY: `physical` is in bounds for the values when `index` is.
        unsafe { V::is_null_unchecked(&typed.values, physical) }
    }

    #[inline]
    fn value(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
        let physical = typed.run.get_physical_index(index);
        V::value(&typed.values, physical)
    }

    #[inline]
    unsafe fn value_unchecked(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
        // `get_physical_index` maps the logical index to a values index; for an
        // in-bounds logical index it returns an in-bounds physical one.
        let physical = typed.run.get_physical_index(index);
        // SAFETY: `physical` is in bounds for the values when `index` is.
        unsafe { V::value_unchecked(&typed.values, physical) }
    }

    fn to_owned_value(value: Self::Value<'_>) -> Self::Owned {
        V::to_owned_value(value)
    }
}

impl<R: RunEndType + 'static, V: crate::ConcreteType + 'static> crate::ConcreteType for Run<R, V> {
    fn data_type() -> DataType {
        DataType::RunEndEncoded(
            std::sync::Arc::new(Field::new("run_ends", R::data_type(), false)),
            std::sync::Arc::new(Field::new("values", V::data_type(), V::NULLABLE)),
        )
    }

    fn build(values: impl Iterator<Item = Option<Self::Owned>>) -> Result<ArrayRef, ColumnError> {
        let plain = V::build(values)?;
        // This can fail on run-end overflow: more logical rows than `R` can index
        // (e.g. more than 32767 for `i16`). Hence `Run` is NOT `InfallibleBuild`.
        arrow::compute::cast(&plain, &Self::data_type()).map_err(ColumnError::Build)
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

/// `vec.try_into()` support for run-end-encoded arrays,
/// whose building is fallible (run-end overflow) — see
/// [`crate::TypedArray::try_from_values`].
///
/// There is no `Column` counterpart: a column needs a name, which a `Vec` does
/// not carry. Use [`Column::try_from_values`](crate::Column::try_from_values).
impl<R, V, T> TryFrom<Vec<T>> for crate::TypedArray<Run<R, V>>
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
