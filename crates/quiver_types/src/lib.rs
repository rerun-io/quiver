//! Core types for the [`quiver`](https://docs.rs/quiver) crate.
//!
//! You should normally depend on `quiver` instead of this crate.
//! `quiver_types` exists so that the bulk of `quiver` compiles independently
//! of the (optional) `quiver_derive` proc-macro crate.

// The workspace warns on `unsafe_code`; this crate opts into it for one audited
// use: [`LogicalType::value_unchecked`] and [`LogicalType::is_null_unchecked`]
// skip arrow's per-element bounds check on the hot read path. Their only
// precondition is `index < length`, which the caller establishes once (the
// column length, or a list element's offset range) before iterating. The read
// then relies on arrow's own buffer/offset invariants — which a constructed
// arrow array upholds by safe-Rust construction; quiver does not re-validate
// them, it validates datatype and nullability. See `value_unchecked`.
#![expect(
    unsafe_code,
    reason = "value_unchecked / is_null_unchecked skip arrow's per-element bounds check; the index is bounds-checked once up front"
)]

pub use arrow;
pub use half;

mod any_list;
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
mod large_list;
mod list;
mod list_value;
mod list_view;
mod map;
mod newtype;
mod option;
mod primitive;
mod run;
mod string;
mod time;
mod timestamp;
mod typed_array;

pub use self::any_list::{AnyList, AnyTypedList};
pub use self::binary::{AnyBinary, AnyTypedBinary, Binary, BinaryView, LargeBinary};
pub use self::column::{Column, ColumnIntoIter, ColumnIter};
pub use self::column_desc::{ColumnDesc, DynColumnDesc};
pub use self::datatype::{
    ColumnError, ConcreteType, InfallibleBuild, LogicalType, PrimitiveType, RefType,
};
pub use self::date::{Date32, Date64};
pub use self::dictionary::{Dictionary, DictionaryKey, TypedDictionary};
pub use self::duration::{
    Duration, DurationMicrosecond, DurationMillisecond, DurationNanosecond, DurationSecond,
};
pub use self::error::{Error, ErrorKind};
pub use self::fixed_size_binary::FixedSizeBinary;
pub use self::fixed_size_list::{FixedSizeList, TypedFixedSizeList};
pub use self::large_list::{LargeList, TypedLargeList};
pub use self::list::{List, TypedList};
pub use self::list_value::ListValue;
pub use self::list_view::{LargeListView, ListView, TypedLargeListView, TypedListView};
pub use self::map::{Map, MapValue, TypedMap};
pub use self::newtype::As;
pub use self::run::{Run, RunEndType, TypedRun};
pub use self::string::{AnyTypedUtf8, AnyUtf8, LargeUtf8, Utf8, Utf8View};
pub use self::time::{Time32Millisecond, Time32Second, Time64Microsecond, Time64Nanosecond};
pub use self::timestamp::{
    Microsecond, Millisecond, Nanosecond, NoTimezone, Second, TimeUnitSpec, Timestamp,
    TimestampMicrosecond, TimestampMillisecond, TimestampNanosecond, TimestampSecond, TimezoneSpec,
    Utc,
};

/// A single dynamically-typed column of a record batch:
/// the field description plus the actual data.
#[derive(Clone, Debug)]
pub struct DynColumn {
    pub field: arrow::datatypes::FieldRef,
    pub array: arrow::array::ArrayRef,
}
