//! [`Binary`], [`LargeBinary`], and [`BinaryView`]: logical types for columns of
//! byte strings.
//!
//! Each element is a variable-length sequence of bytes (like a `Vec<u8>`),
//! stored as an [`arrow::array::BinaryArray`]
//! ([`DataType::Binary`]),
//! [`arrow::array::LargeBinaryArray`] for 64-bit offsets (when a single column
//! may hold more than 2 `GiB` of data in total), or
//! [`arrow::array::BinaryViewArray`] for the newer "view" encoding
//! ([`DataType::BinaryView`]), optimized for comparisons and out-of-order writes.
//! Reading is zero-copy: the element values are `&[u8]`.
//!
//! [`AnyBinary`] accepts *any* binary encoding — these three plus
//! [`FixedSizeBinary`](crate::FixedSizeBinary) — all read as `&[u8]`, for when
//! the encoding is decided at runtime.

use arrow::array::{Array, ArrayRef};
use arrow::datatypes::DataType;

use crate::datatype::{ColumnError, InfallibleBuild, LogicalType, RefType, downcast_array};

/// Marker for an arrow `Binary` column: variable-length byte strings.
///
/// The element values are `&[u8]`; the owned values are `Vec<u8>`.
/// (A plain Rust type like `Vec<u8>` would be ambiguous here:
/// arrow distinguishes `Binary` from `List(UInt8)`.)
///
/// ```
/// use quiver::{Binary, Column};
///
/// let column = Column::<Binary>::from_values([b"abc".to_vec(), vec![0, 1]]);
/// assert_eq!(column.value(0), b"abc"); // borrowed `&[u8]`, zero-copy
/// ```
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct Binary;

/// Marker for an arrow `LargeBinary` column: like [`Binary`], with 64-bit offsets.
///
/// ```
/// use quiver::{Column, LargeBinary};
///
/// let column = Column::<LargeBinary>::from_values([b"abc".to_vec()]);
/// assert_eq!(column.value(0), b"abc");
/// ```
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct LargeBinary;

/// Marker for an arrow `BinaryView` column: like [`Binary`], in the newer "view"
/// encoding ([`arrow::array::BinaryViewArray`]), optimized for comparisons
/// and out-of-order writes.
///
/// ```
/// use quiver::{BinaryView, Column};
///
/// let column = Column::<BinaryView>::from_values([b"abc".to_vec()]);
/// assert_eq!(column.value(0), b"abc");
/// ```
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct BinaryView;

macro_rules! impl_binary_datatype {
    ($marker:ty, $array:ty, $datatype:expr) => {
        impl LogicalType for $marker {
            type Typed = $array;
            type Value<'a> = &'a [u8];
            type Owned = Vec<u8>;

            fn downcast(array: &dyn Array) -> Result<Self::Typed, ColumnError> {
                downcast_array::<$array>(array, || format!("{:?}", $datatype))
            }

            fn is_null(typed: &Self::Typed, index: usize) -> bool {
                typed.is_null(index)
            }

            fn value(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
                typed.value(index)
            }

            fn to_owned_value(value: Self::Value<'_>) -> Self::Owned {
                value.to_vec()
            }
        }

        impl crate::ConcreteType for $marker {
            fn datatype() -> DataType {
                $datatype
            }

            fn build(
                values: impl Iterator<Item = Option<Self::Owned>>,
            ) -> Result<ArrayRef, ColumnError> {
                Ok(std::sync::Arc::new(<$array>::from_iter(values)))
            }
        }

        impl InfallibleBuild for $marker {}

        impl RefType for $marker {
            type Ref = [u8];

            fn value_ref(typed: &Self::Typed, index: usize) -> &[u8] {
                typed.value(index)
            }
        }
    };
}

impl_binary_datatype!(Binary, arrow::array::BinaryArray, DataType::Binary);
impl_binary_datatype!(
    LargeBinary,
    arrow::array::LargeBinaryArray,
    DataType::LargeBinary
);
impl_binary_datatype!(
    BinaryView,
    arrow::array::BinaryViewArray,
    DataType::BinaryView
);

/// Marker for a binary column in *any* of arrow's byte-string encodings.
///
/// Accepts [`Binary`], [`LargeBinary`], [`BinaryView`], or
/// [`FixedSizeBinary`](crate::FixedSizeBinary) (of any size). They all read as
/// `&[u8]` — a `FixedSizeBinary<N>`'s fixed-width elements are seen here as
/// plain `&[u8]` slices (length `N`), not `&[u8; N]`.
///
/// Like [`AnyList`](crate::AnyList), this is a quiver-only logical type with no
/// single arrow datatype: `Column<AnyBinary>` accepts whichever encoding it is
/// handed and reads them all uniformly. It is *parse-only* — it implements
/// [`LogicalType`] (so `try_from`/reading work) but not
/// [`ConcreteType`](crate::ConcreteType), so there is no `from_values`/`Default`/
/// schema; to build, pick a concrete encoding such as `Column<Binary>`.
///
/// ```
/// use quiver::{AnyBinary, Column};
/// use quiver::arrow::array::{ArrayRef, LargeBinaryArray};
/// # use std::sync::Arc;
///
/// // `array` may be a Binary / LargeBinary / BinaryView:
/// let array: ArrayRef = Arc::new(LargeBinaryArray::from_iter_values([b"abc"]));
/// let column = Column::<AnyBinary>::try_from(array).unwrap();
/// assert_eq!(column.value(0), b"abc");
/// ```
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct AnyBinary;

/// The validated representation of an [`AnyBinary`] column: one of the
/// per-encoding binary arrays.
#[derive(Clone)]
pub enum AnyTypedBinary {
    Binary(arrow::array::BinaryArray),
    LargeBinary(arrow::array::LargeBinaryArray),
    BinaryView(arrow::array::BinaryViewArray),
    FixedSizeBinary(arrow::array::FixedSizeBinaryArray),
}

impl LogicalType for AnyBinary {
    type Typed = AnyTypedBinary;
    type Value<'a> = &'a [u8];
    type Owned = Vec<u8>;

    fn downcast(array: &dyn Array) -> Result<Self::Typed, ColumnError> {
        match array.data_type() {
            DataType::Binary => Ok(AnyTypedBinary::Binary(downcast_array(array, || {
                "Binary".to_owned()
            })?)),
            DataType::LargeBinary => {
                Ok(AnyTypedBinary::LargeBinary(downcast_array(array, || {
                    "LargeBinary".to_owned()
                })?))
            }
            DataType::BinaryView => Ok(AnyTypedBinary::BinaryView(downcast_array(array, || {
                "BinaryView".to_owned()
            })?)),
            DataType::FixedSizeBinary(_) => Ok(AnyTypedBinary::FixedSizeBinary(downcast_array(
                array,
                || "FixedSizeBinary".to_owned(),
            )?)),
            actual => Err(ColumnError::WrongDatatype {
                expected: "a binary array (Binary/LargeBinary/BinaryView/FixedSizeBinary)"
                    .to_owned(),
                actual: actual.clone(),
            }),
        }
    }

    fn is_null(typed: &Self::Typed, index: usize) -> bool {
        match typed {
            AnyTypedBinary::Binary(array) => array.is_null(index),
            AnyTypedBinary::LargeBinary(array) => array.is_null(index),
            AnyTypedBinary::BinaryView(array) => array.is_null(index),
            AnyTypedBinary::FixedSizeBinary(array) => array.is_null(index),
        }
    }

    fn value(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
        match typed {
            AnyTypedBinary::Binary(array) => array.value(index),
            AnyTypedBinary::LargeBinary(array) => array.value(index),
            AnyTypedBinary::BinaryView(array) => array.value(index),
            AnyTypedBinary::FixedSizeBinary(array) => array.value(index),
        }
    }

    fn to_owned_value(value: Self::Value<'_>) -> Self::Owned {
        value.to_vec()
    }
}

impl RefType for AnyBinary {
    type Ref = [u8];

    fn value_ref(typed: &Self::Typed, index: usize) -> &[u8] {
        Self::value(typed, index)
    }
}
