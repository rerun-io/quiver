//! [`Duration`]: a logical type for columns of elapsed time.
//!
//! Each element is an `i64` counting time units (e.g. milliseconds) — the
//! *difference* between two points in time, as opposed to a [`crate::Timestamp`].
//! Stored as the [`arrow::array::DurationMillisecondArray`] family
//! ([`DataType::Duration`]).

use arrow::array::{Array, ArrayRef};
use arrow::datatypes::DataType;

use crate::datatype::{
    ColumnError, InfallibleBuild, LogicalType, PrimitiveType, RefType, downcast_array,
};
use crate::timestamp::{Microsecond, Millisecond, Nanosecond, Second, TimeUnitSpec};
use std::marker::PhantomData;

/// Marker for an arrow `Duration` column, e.g. `Duration<Nanosecond>`.
///
/// The values are raw `i64` ticks in the given [`TimeUnitSpec`].
///
/// ```
/// use quiver::{Column, Duration, Millisecond};
///
/// let column = Column::<Duration<Millisecond>>::from_values([10, 20]);
/// assert_eq!(column.value(1), 20); // elapsed milliseconds
/// ```
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct Duration<U> {
    _marker: PhantomData<fn() -> U>,
}

impl<U: TimeUnitSpec + 'static> LogicalType for Duration<U> {
    type Typed = arrow::array::PrimitiveArray<U::DurationType>;
    type Value<'a>
        = i64
    where
        Self: 'a;
    type Owned = i64;

    fn downcast(array: &dyn Array) -> Result<Self::Typed, ColumnError> {
        // The unit is part of `Self::Typed`'s Rust type, so `downcast_array`
        // already rejects the wrong unit.
        downcast_array::<Self::Typed>(array, || {
            format!("{:?}", <Self as crate::ConcreteType>::datatype())
        })
    }

    fn is_null(typed: &Self::Typed, index: usize) -> bool {
        typed.is_null(index)
    }

    fn value(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
        typed.value(index)
    }

    fn to_owned_value(value: Self::Value<'_>) -> Self::Owned {
        value
    }
}

impl<U: TimeUnitSpec + 'static> crate::ConcreteType for Duration<U> {
    fn datatype() -> DataType {
        DataType::Duration(<U::TimestampType as arrow::datatypes::ArrowTimestampType>::UNIT)
    }

    fn build(values: impl Iterator<Item = Option<Self::Owned>>) -> Result<ArrayRef, ColumnError> {
        let array: arrow::array::PrimitiveArray<U::DurationType> = values.collect();
        Ok(std::sync::Arc::new(array))
    }
}

pub type DurationSecond = Duration<Second>;
pub type DurationMillisecond = Duration<Millisecond>;
pub type DurationMicrosecond = Duration<Microsecond>;
pub type DurationNanosecond = Duration<Nanosecond>;

impl<U: TimeUnitSpec + 'static> InfallibleBuild for Duration<U> {}

impl<U: TimeUnitSpec + 'static> PrimitiveType for Duration<U> {
    type Native = i64;

    fn values(typed: &Self::Typed) -> &[i64] {
        typed.values()
    }
}

impl<U: TimeUnitSpec + 'static> RefType for Duration<U> {
    type Ref = i64;

    fn value_ref(typed: &Self::Typed, index: usize) -> &i64 {
        &typed.values()[index]
    }
}
