//! [`Binary`] and [`LargeBinary`]: logical types for columns of byte strings.
//!
//! Each element is a variable-length sequence of bytes (like a `Vec<u8>`),
//! stored as an [`arrow::array::BinaryArray`]
//! ([`DataType::Binary`](arrow::datatypes::DataType::Binary)), or
//! [`arrow::array::LargeBinaryArray`] for 64-bit offsets (when a single column
//! may hold more than 2 `GiB` of data in total).
//! Reading is zero-copy: the element values are `&[u8]`.

use arrow::array::{Array, ArrayRef};
use arrow::datatypes::DataType;

use crate::datatype::{ColumnError, Datatype, InfallibleBuild, downcast_array};

/// Marker for an arrow `Binary` column: variable-length byte strings.
///
/// The element values are `&[u8]`; the owned values are `Vec<u8>`.
/// (A plain Rust type like `Vec<u8>` would be ambiguous here:
/// arrow distinguishes `Binary` from `List(UInt8)`.)
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct Binary;

/// Marker for an arrow `LargeBinary` column: like [`Binary`], with 64-bit offsets.
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct LargeBinary;

macro_rules! impl_binary_datatype {
    ($marker:ty, $array:ty, $datatype:expr) => {
        impl Datatype for $marker {
            type Typed = $array;
            type Value<'a> = &'a [u8];
            type Owned = Vec<u8>;

            fn datatype() -> DataType {
                $datatype
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

            fn build(
                values: impl Iterator<Item = Option<Self::Owned>>,
            ) -> Result<ArrayRef, ColumnError> {
                Ok(std::sync::Arc::new(<$array>::from_iter(values)))
            }

            fn to_owned_value(value: Self::Value<'_>) -> Self::Owned {
                value.to_vec()
            }
        }

        impl InfallibleBuild for $marker {}
    };
}

impl_binary_datatype!(Binary, arrow::array::BinaryArray, DataType::Binary);
impl_binary_datatype!(
    LargeBinary,
    arrow::array::LargeBinaryArray,
    DataType::LargeBinary
);
