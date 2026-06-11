//! [`Timestamp`]: a logical type for columns of points in time.
//!
//! Each element is an `i64` counting time units since the Unix epoch
//! (1970-01-01 00:00:00), e.g. `TimestampNanosecond<Utc>` for nanoseconds in UTC.
//! Stored as the [`arrow::array::TimestampNanosecondArray`] family
//! ([`DataType::Timestamp`]).
//!
//! The time unit and the (optional) timezone are part of the type,
//! via the [`TimeUnitSpec`] and [`TimezoneSpec`] marker types.

use std::marker::PhantomData;

use arrow::array::{Array, ArrayRef};
use arrow::datatypes::DataType;

use crate::datatype::{
    ColumnError, InfallibleBuild, LogicalType, PrimitiveType, RefType, downcast_array,
};

/// Marker for an arrow `Timestamp` column, e.g. `Timestamp<Nanosecond, Utc>`.
///
/// The values are raw `i64` ticks in the given [`TimeUnitSpec`],
/// counted from the unix epoch.
///
/// The timezone defaults to [`NoTimezone`]. Note that timezones are matched
/// *exactly*: a column declared `Timestamp<Nanosecond, Utc>` ("UTC") will not
/// accept an array with the timezone "+00:00".
///
/// ```
/// use quiver::{Column, Nanosecond, Timestamp, Utc};
///
/// let column = Column::<Timestamp<Nanosecond, Utc>>::from_values([1, 2, 3]);
/// assert_eq!(column.value(0), 1); // raw `i64` ticks since the epoch
/// ```
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct Timestamp<U, Z = NoTimezone> {
    _marker: PhantomData<fn() -> (U, Z)>,
}

/// A [`Timestamp`]/[`Duration`](crate::Duration) time unit:
/// [`Second`], [`Millisecond`], [`Microsecond`], or [`Nanosecond`].
pub trait TimeUnitSpec {
    /// The corresponding arrow timestamp type, e.g. `TimestampNanosecondType`.
    type TimestampType: arrow::datatypes::ArrowTimestampType;

    /// The corresponding arrow duration type, e.g. `DurationNanosecondType`.
    type DurationType: arrow::datatypes::ArrowPrimitiveType<Native = i64>;
}

pub struct Second;
pub struct Millisecond;
pub struct Microsecond;
pub struct Nanosecond;

impl TimeUnitSpec for Second {
    type TimestampType = arrow::datatypes::TimestampSecondType;
    type DurationType = arrow::datatypes::DurationSecondType;
}
impl TimeUnitSpec for Millisecond {
    type TimestampType = arrow::datatypes::TimestampMillisecondType;
    type DurationType = arrow::datatypes::DurationMillisecondType;
}
impl TimeUnitSpec for Microsecond {
    type TimestampType = arrow::datatypes::TimestampMicrosecondType;
    type DurationType = arrow::datatypes::DurationMicrosecondType;
}
impl TimeUnitSpec for Nanosecond {
    type TimestampType = arrow::datatypes::TimestampNanosecondType;
    type DurationType = arrow::datatypes::DurationNanosecondType;
}

/// The timezone of a [`Timestamp`]: [`NoTimezone`], [`Utc`], or your own marker type.
pub trait TimezoneSpec {
    /// E.g. `Some("UTC")`, `Some("+02:00")`, or `None` for timezone-naive timestamps.
    fn timezone() -> Option<std::sync::Arc<str>>;
}

/// Timezone-naive timestamps.
pub struct NoTimezone;

impl TimezoneSpec for NoTimezone {
    fn timezone() -> Option<std::sync::Arc<str>> {
        None
    }
}

/// The "UTC" timezone.
pub struct Utc;

impl TimezoneSpec for Utc {
    fn timezone() -> Option<std::sync::Arc<str>> {
        Some("UTC".into())
    }
}

impl<U: TimeUnitSpec + 'static, Z: TimezoneSpec + 'static> LogicalType for Timestamp<U, Z> {
    type Typed = arrow::array::PrimitiveArray<U::TimestampType>;
    type Value<'a>
        = i64
    where
        Self: 'a;
    type Owned = i64;

    fn downcast(array: &dyn Array) -> Result<Self::Typed, ColumnError> {
        // The timezone is not in the array's Rust type (only the unit is), so
        // check the full datatype here — timezones are matched exactly.
        let expected = || format!("{:?}", <Self as crate::ConcreteType>::datatype());
        if array.data_type() != &<Self as crate::ConcreteType>::datatype() {
            return Err(ColumnError::WrongDatatype {
                expected: expected(),
                actual: array.data_type().clone(),
            });
        }
        downcast_array::<Self::Typed>(array, expected)
    }

    fn is_null(typed: &Self::Typed, index: usize) -> bool {
        typed.is_null(index)
    }

    fn value(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
        typed.value(index)
    }

    unsafe fn value_unchecked(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
        // SAFETY: the caller guarantees `index` is in bounds.
        unsafe { typed.value_unchecked(index) }
    }

    fn to_owned_value(value: Self::Value<'_>) -> Self::Owned {
        value
    }
}

impl<U: TimeUnitSpec + 'static, Z: TimezoneSpec + 'static> crate::ConcreteType for Timestamp<U, Z> {
    fn datatype() -> DataType {
        DataType::Timestamp(
            <U::TimestampType as arrow::datatypes::ArrowTimestampType>::UNIT,
            Z::timezone(),
        )
    }

    fn build(values: impl Iterator<Item = Option<Self::Owned>>) -> Result<ArrayRef, ColumnError> {
        let array: arrow::array::PrimitiveArray<U::TimestampType> = values.collect();
        Ok(std::sync::Arc::new(array.with_timezone_opt(Z::timezone())))
    }
}

pub type TimestampSecond<Z = NoTimezone> = Timestamp<Second, Z>;
pub type TimestampMillisecond<Z = NoTimezone> = Timestamp<Millisecond, Z>;
pub type TimestampMicrosecond<Z = NoTimezone> = Timestamp<Microsecond, Z>;
pub type TimestampNanosecond<Z = NoTimezone> = Timestamp<Nanosecond, Z>;

impl<U: TimeUnitSpec + 'static, Z: TimezoneSpec + 'static> InfallibleBuild for Timestamp<U, Z> {}

impl<U: TimeUnitSpec + 'static, Z: TimezoneSpec + 'static> PrimitiveType for Timestamp<U, Z> {
    type Native = i64;

    fn values(typed: &Self::Typed) -> &[i64] {
        typed.values()
    }
}

impl<U: TimeUnitSpec + 'static, Z: TimezoneSpec + 'static> RefType for Timestamp<U, Z> {
    type Ref = i64;

    fn value_ref(typed: &Self::Typed, index: usize) -> &i64 {
        &typed.values()[index]
    }
}
