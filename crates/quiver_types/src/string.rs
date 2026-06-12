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
//!
//! [`AnyUtf8`] accepts *any* of those three encodings (they all read as `&str`),
//! for when the encoding is decided at runtime.

use arrow::array::{Array, ArrayRef};
use arrow::datatypes::DataType;

use crate::datatype::{ColumnError, LogicalType, RefType, downcast_array, impl_marker_datatype};

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

/// Marker for a UTF-8 column in *any* of arrow's string encodings.
///
/// Accepts [`Utf8`], [`LargeUtf8`], or [`Utf8View`] — they all read as `&str`.
///
/// Like [`AnyList`](crate::AnyList), this is a quiver-only logical type with no
/// single arrow datatype: `Column<AnyUtf8>` accepts whichever encoding it is
/// handed and reads them all uniformly. It is *parse-only* — it implements
/// [`LogicalType`] (so `try_from`/reading work) but not
/// [`ConcreteType`](crate::ConcreteType), so there is no `from_values`/`Default`/
/// schema; to build, pick a concrete encoding such as `Column<Utf8>`.
///
/// ```
/// use quiver::{AnyUtf8, Column};
/// use quiver::arrow::array::{ArrayRef, LargeStringArray};
/// # use std::sync::Arc;
///
/// // `array` may be a Utf8 / LargeUtf8 / Utf8View:
/// let array: ArrayRef = Arc::new(LargeStringArray::from(vec!["alice", "bob"]));
/// let column = Column::<AnyUtf8>::try_from(array).unwrap();
/// assert_eq!(column.value(0), "alice");
/// ```
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct AnyUtf8;

/// The validated representation of an [`AnyUtf8`] column: one of the
/// per-encoding string arrays.
#[derive(Clone)]
pub enum AnyTypedUtf8 {
    Utf8(arrow::array::StringArray),
    LargeUtf8(arrow::array::LargeStringArray),
    Utf8View(arrow::array::StringViewArray),
}

impl LogicalType for AnyUtf8 {
    type Typed = AnyTypedUtf8;
    type Value<'a> = &'a str;
    type Owned = String;

    fn downcast(array: &dyn Array) -> Result<Self::Typed, ColumnError> {
        match array.data_type() {
            DataType::Utf8 => Ok(AnyTypedUtf8::Utf8(downcast_array(array, || {
                "Utf8".to_owned()
            })?)),
            DataType::LargeUtf8 => Ok(AnyTypedUtf8::LargeUtf8(downcast_array(array, || {
                "LargeUtf8".to_owned()
            })?)),
            DataType::Utf8View => Ok(AnyTypedUtf8::Utf8View(downcast_array(array, || {
                "Utf8View".to_owned()
            })?)),
            actual => Err(ColumnError::WrongDatatype {
                expected: "a string array (Utf8/LargeUtf8/Utf8View)".to_owned(),
                actual: actual.clone(),
            }),
        }
    }

    #[inline]
    fn is_null(typed: &Self::Typed, index: usize) -> bool {
        match typed {
            AnyTypedUtf8::Utf8(array) => array.is_null(index),
            AnyTypedUtf8::LargeUtf8(array) => array.is_null(index),
            AnyTypedUtf8::Utf8View(array) => array.is_null(index),
        }
    }

    #[inline]
    unsafe fn is_null_unchecked(typed: &Self::Typed, index: usize) -> bool {
        // SAFETY: the caller guarantees `index` is in bounds for the held array.
        unsafe {
            match typed {
                AnyTypedUtf8::Utf8(array) => crate::datatype::leaf_is_null_unchecked(array, index),
                AnyTypedUtf8::LargeUtf8(array) => {
                    crate::datatype::leaf_is_null_unchecked(array, index)
                }
                AnyTypedUtf8::Utf8View(array) => {
                    crate::datatype::leaf_is_null_unchecked(array, index)
                }
            }
        }
    }

    #[inline]
    fn value(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
        match typed {
            AnyTypedUtf8::Utf8(array) => array.value(index),
            AnyTypedUtf8::LargeUtf8(array) => array.value(index),
            AnyTypedUtf8::Utf8View(array) => array.value(index),
        }
    }

    #[inline]
    unsafe fn value_unchecked(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
        // SAFETY: the caller guarantees `index` is in bounds for the held array.
        unsafe {
            match typed {
                AnyTypedUtf8::Utf8(array) => array.value_unchecked(index),
                AnyTypedUtf8::LargeUtf8(array) => array.value_unchecked(index),
                AnyTypedUtf8::Utf8View(array) => array.value_unchecked(index),
            }
        }
    }

    fn to_owned_value(value: Self::Value<'_>) -> Self::Owned {
        value.to_owned()
    }
}

impl RefType for AnyUtf8 {
    type Ref = str;

    #[inline]
    fn value_ref(typed: &Self::Typed, index: usize) -> &str {
        Self::value(typed, index)
    }
}
