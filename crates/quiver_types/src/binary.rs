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

            fn matches(actual: &DataType) -> bool {
                crate::datatype::datatypes_compatible(actual, &$datatype)
            }

            fn supported_datatypes() -> Vec<DataType> {
                vec![$datatype]
            }

            fn downcast(array: &dyn Array) -> Result<Self::Typed, ColumnError> {
                downcast_array::<$array>(array)
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
