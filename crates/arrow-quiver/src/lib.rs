#![doc = include_str!("../../../README.md")]
// NOTE: the code blocks in the README double as doctests for this crate.

pub use arrow;

mod column;
mod error;

pub use self::column::{
    Column, ColumnError, ColumnIter, Datatype, Duration, List, ListValue, Microsecond, Millisecond,
    Nanosecond, NoTimezone, Second, TimeUnitSpec, Timestamp, TimezoneSpec, TypedList, Utc,
};
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
