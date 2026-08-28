//! Core types for the [`quiver`](https://docs.rs/quiver) crate.
//!
//! You should normally depend on `quiver` instead of this crate.
//! `quiver_types` exists so that the bulk of `quiver` compiles independently
//! of the (optional) `quiver_derive` proc-macro crate.
//!
//! ## The main types
//!
//! * [`Column<L>`]: one column of a record batch — an arrow array validated
//!   against the logical type `L`, plus that column's field metadata.
//! * [`TypedArray<L>`]: the data half of a [`Column<L>`](Column) — the same
//!   validated array with the same value API, minus the metadata.
//!   A [`Column<L>`](Column) is a [`TypedArray<L>`](TypedArray) plus metadata;
//!   use a [`TypedArray<L>`](TypedArray) directly for arrays that aren't
//!   record batch columns.
//! * [`ColumnDesc<L>`](ColumnDesc): a named handle on one column,
//!   holding no data. It knows both the column name and `L`, so it can
//!   [`extract`](ColumnDesc::extract) a [`Column<L>`](Column) from a record
//!   batch, or validate a loose `ArrayRef` into a
//!   [`TypedArray<L>`](TypedArray) with
//!   [`typed_array`](ColumnDesc::typed_array) — without you naming either at
//!   the call site. `#[derive(Quiver)]` generates one per field as a `COLUMN_*`
//!   constant; [`ColumnDesc::new`] is `const`, so you can declare your own.
//!
//! Dynamically typed columns get the same pair without the `L`:
//! [`DynColumn`] and [`DynColumnDesc`].
//!
//! The logical types themselves (`L`) live in [`LogicalType`] and its
//! implementors: [`Utf8`], [`List`], [`Timestamp`], `Option<…>`, `i64`, ….

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
pub use bytemuck;
pub use half;

mod any_list;
mod binary;
mod column;
mod column_desc;
mod datatype;
mod date;
mod dictionary;
mod duration;
mod dyn_column;
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
pub use self::column::Column;
#[expect(deprecated, reason = "re-exporting the old names for one more release")]
pub use self::column::{ColumnIntoIter, ColumnIter};
pub use self::column_desc::{ColumnDesc, DynColumnDesc};
pub use self::datatype::{
    ColumnError, ConcreteType, InfallibleBuild, LogicalType, PrimitiveType, RefType,
};
pub use self::date::{Date32, Date64};
pub use self::dictionary::{Dictionary, DictionaryKey, TypedDictionary};
pub use self::duration::{
    Duration, DurationMicrosecond, DurationMillisecond, DurationNanosecond, DurationSecond,
};
pub use self::dyn_column::DynColumn;
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
pub use self::typed_array::{TypedArray, TypedArrayIntoIter, TypedArrayIter};
