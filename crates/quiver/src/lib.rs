#![cfg_attr(doc, doc = include_str!("../README.md"))]
// NOTE: the code blocks in the README double as doctests for this crate.

pub use arrow;
pub use half;

mod binary;
mod column;
mod column_desc;
mod datatype;
mod date;
mod dictionary;
mod duration;
mod error;
mod fixed_size_binary;
mod fixed_size_list;
mod list;
mod newtype;
mod option;
mod primitive;
mod string;
mod time;
mod timestamp;

pub use self::binary::{Binary, LargeBinary};
pub use self::column::{Column, ColumnIntoIter, ColumnIter};
pub use self::column_desc::{ColumnDesc, DynColumnDesc};
pub use self::datatype::{ColumnError, Datatype, InfallibleBuild};
pub use self::date::{Date32, Date64};
pub use self::dictionary::{Dictionary, DictionaryKey, TypedDictionary};
pub use self::duration::{
    Duration, DurationMicrosecond, DurationMillisecond, DurationNanosecond, DurationSecond,
};
pub use self::error::{Error, ErrorKind};
pub use self::fixed_size_list::{FixedSizeList, TypedFixedSizeList};
pub use self::list::{List, ListValue, TypedList};
pub use self::newtype::As;
pub use self::string::LargeUtf8;
pub use self::time::{Time32Millisecond, Time32Second, Time64Microsecond, Time64Nanosecond};
pub use self::timestamp::{
    Microsecond, Millisecond, Nanosecond, NoTimezone, Second, TimeUnitSpec, Timestamp,
    TimestampMicrosecond, TimestampMillisecond, TimestampNanosecond, TimestampSecond, TimezoneSpec,
    Utc,
};

#[cfg(feature = "derive")]
pub use quiver_derive::Quiver;

/// A single dynamically-typed column of a record batch:
/// the field description plus the actual data.
#[derive(Clone, Debug)]
pub struct DynColumn {
    pub field: arrow::datatypes::FieldRef,
    pub array: arrow::array::ArrayRef,
}
