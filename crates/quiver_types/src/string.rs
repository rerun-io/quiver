//! [`Utf8`], [`LargeUtf8`], and [`Utf8View`]: logical types for columns of UTF-8 text.
//!
//! A `Column<Utf8>` is a column of strings, stored as an
//! [`arrow::array::StringArray`] ([`DataType::Utf8`]).
//! [`LargeUtf8`] is the same with 64-bit offsets
//! (for single columns holding more than 2 `GiB` of text in total),
//! stored as an [`arrow::array::LargeStringArray`].
//! [`Utf8View`] is the newer variable-length encoding
//! ([`arrow::array::StringViewArray`]), optimized for comparisons
//! and out-of-order writes.
//!
//! All three are markers: the owned values are `String`s, and reading is
//! zero-copy — the element values are `&str` borrows into the array.

use arrow::array::{Array, ArrayRef};
use arrow::datatypes::DataType;

use crate::datatype::{ColumnError, LogicalType, RefType, impl_marker_datatype};

/// UTF-8 text: an arrow [`DataType::Utf8`] column with `String` values.
///
/// ```
/// use quiver::{Column, Utf8};
///
/// let column = Column::<Utf8>::from_values(["alice", "bob"]);
/// assert_eq!(column.value(0), "alice"); // borrowed `&str`, zero-copy
/// ```
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct Utf8;

impl_marker_datatype!(
    Utf8,
    arrow::array::StringArray,
    &'a str,
    String,
    DataType::Utf8
);

/// Like [`Utf8`], with 64-bit offsets
/// (for single columns holding more than 2 `GiB` of text in total).
///
/// ```
/// use quiver::{Column, LargeUtf8};
///
/// let column = Column::<LargeUtf8>::from_values(["alice", "bob"]);
/// assert_eq!(column.value(1), "bob");
/// ```
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

/// Like [`Utf8`], in the newer "view" encoding
/// ([`arrow::array::StringViewArray`]), optimized for comparisons
/// and out-of-order writes.
///
/// ```
/// use quiver::{Column, Utf8View};
///
/// let column = Column::<Utf8View>::from_values(["alice", "bob"]);
/// assert_eq!(column.value(0), "alice");
/// ```
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct Utf8View;

impl_marker_datatype!(
    Utf8View,
    arrow::array::StringViewArray,
    &'a str,
    String,
    DataType::Utf8View
);

impl RefType for Utf8 {
    type Ref = str;

    fn value_ref(typed: &Self::Typed, index: usize) -> &str {
        typed.value(index)
    }
}

impl RefType for LargeUtf8 {
    type Ref = str;

    fn value_ref(typed: &Self::Typed, index: usize) -> &str {
        typed.value(index)
    }
}

impl RefType for Utf8View {
    type Ref = str;

    fn value_ref(typed: &Self::Typed, index: usize) -> &str {
        typed.value(index)
    }
}
