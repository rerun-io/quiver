//! [`Duration`]: a logical type for columns of elapsed time.
//!
//! Each element is an `i64` counting time units (e.g. milliseconds) — the
//! *difference* between two points in time, as opposed to a [`crate::Timestamp`].
//! Stored as the [`arrow::array::DurationMillisecondArray`] family
//! ([`DataType::Duration`]).

use arrow::array::{Array, ArrayRef};
use arrow::datatypes::DataType;

use crate::datatype::{
    ColumnError, Datatype, InfallibleBuild, PrimitiveDatatype, RefDatatype, downcast_array,
};
use crate::timestamp::{Microsecond, Millisecond, Nanosecond, Second, TimeUnitSpec};
use std::marker::PhantomData;

/// Marker for an arrow `Duration` column, e.g. `Duration<Nanosecond>`.
///
/// The values are raw `i64` ticks in the given [`TimeUnitSpec`].
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct Duration<U> {
    _marker: PhantomData<fn() -> U>,
}

impl<U: TimeUnitSpec + 'static> Datatype for Duration<U> {
    type Typed = arrow::array::PrimitiveArray<U::DurationType>;
    type Value<'a>
        = i64
    where
        Self: 'a;
    type Owned = i64;

    fn datatype() -> Option<DataType> {
        Some(DataType::Duration(
            <U::TimestampType as arrow::datatypes::ArrowTimestampType>::UNIT,
        ))
    }

    fn downcast(array: &dyn Array) -> Result<Self::Typed, ColumnError> {
        downcast_array::<Self::Typed>(array)
    }

    fn is_null(typed: &Self::Typed, index: usize) -> bool {
        typed.is_null(index)
    }

    fn value(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
        typed.value(index)
    }

    fn build(values: impl Iterator<Item = Option<Self::Owned>>) -> Result<ArrayRef, ColumnError> {
        let array: arrow::array::PrimitiveArray<U::DurationType> = values.collect();
        Ok(std::sync::Arc::new(array))
    }

    fn to_owned_value(value: Self::Value<'_>) -> Self::Owned {
        value
    }
}

pub type DurationSecond = Duration<Second>;
pub type DurationMillisecond = Duration<Millisecond>;
pub type DurationMicrosecond = Duration<Microsecond>;
pub type DurationNanosecond = Duration<Nanosecond>;

impl<U: TimeUnitSpec + 'static> InfallibleBuild for Duration<U> {}

impl<U: TimeUnitSpec + 'static> PrimitiveDatatype for Duration<U> {
    type Native = i64;

    fn values(typed: &Self::Typed) -> &[i64] {
        typed.values()
    }
}

impl<U: TimeUnitSpec + 'static> RefDatatype for Duration<U> {
    type Ref = i64;

    fn value_ref(typed: &Self::Typed, index: usize) -> &i64 {
        &typed.values()[index]
    }
}
