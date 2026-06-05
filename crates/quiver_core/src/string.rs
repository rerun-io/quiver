//! `String` and [`LargeUtf8`]: logical types for columns of UTF-8 text.
//!
//! A `Column<String>` is a column of strings, stored as an
//! [`arrow::array::StringArray`] ([`DataType::Utf8`]).
//! [`LargeUtf8`] is the same with 64-bit offsets
//! (for single columns holding more than 2 `GiB` of text in total),
//! stored as an [`arrow::array::LargeStringArray`].
//! Reading is zero-copy: the element values are `&str` borrows into the array.

use arrow::array::{Array, ArrayRef};
use arrow::datatypes::DataType;

use crate::datatype::{
    ColumnError, Datatype, RefDatatype, downcast_array, impl_flat_datatype, impl_marker_datatype,
};

impl_flat_datatype!(String, arrow::array::StringArray, &'a str, DataType::Utf8);

/// Like `String`, with 64-bit offsets.
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct LargeUtf8;

impl_marker_datatype!(
    LargeUtf8,
    arrow::array::LargeStringArray,
    &'a str,
    String,
    DataType::LargeUtf8
);

impl RefDatatype for String {
    type Ref = str;

    fn value_ref(typed: &Self::Typed, index: usize) -> &str {
        typed.value(index)
    }
}

impl RefDatatype for LargeUtf8 {
    type Ref = str;

    fn value_ref(typed: &Self::Typed, index: usize) -> &str {
        typed.value(index)
    }
}
