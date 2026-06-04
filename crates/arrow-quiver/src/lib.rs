#![cfg_attr(doc, doc = include_str!("../../../README.md"))]
// NOTE: the code blocks in the README double as doctests for this crate.

pub use arrow;

mod column;
mod column_desc;
mod error;

pub use self::column::{
    Binary, Column, ColumnError, ColumnIntoIter, ColumnIter, Datatype, Duration,
    DurationMicrosecond, DurationMillisecond, DurationNanosecond, DurationSecond, LargeBinary,
    List, ListValue, Microsecond, Millisecond, Nanosecond, NoTimezone, Second, TimeUnitSpec,
    Timestamp, TimestampMicrosecond, TimestampMillisecond, TimestampNanosecond, TimestampSecond,
    TimezoneSpec, TypedList, Utc,
};
pub use self::column_desc::{ColumnDesc, DynColumnDesc};
pub use self::error::{Error, ErrorKind};

#[cfg(feature = "derive")]
pub use arrow_quiver_derive::Quiver;

/// A single dynamically-typed column of a record batch:
/// the field description plus the actual data.
#[derive(Clone, Debug)]
pub struct DynColumn {
    pub field: arrow::datatypes::FieldRef,
    pub array: arrow::array::ArrayRef,
}
