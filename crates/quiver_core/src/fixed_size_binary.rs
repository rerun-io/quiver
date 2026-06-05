//! `[u8; N]`: a logical type for columns of fixed-size byte arrays.
//!
//! A `Column<[u8; 16]>` is a column where every element is exactly 16 bytes
//! (e.g. UUIDs or hashes), stored as an [`arrow::array::FixedSizeBinaryArray`]
//! ([`DataType::FixedSizeBinary`]).
//! The size is part of the type, checked at the parse boundary;
//! the element values are `&[u8; N]`.

use arrow::array::{Array, ArrayRef};
use arrow::datatypes::DataType;

use crate::datatype::{ColumnError, Datatype, InfallibleBuild, RefDatatype, downcast_array};

/// `[u8; N]`: an arrow `FixedSizeBinary(N)` column, e.g. `[u8; 16]` for UUIDs.
impl<const N: usize> Datatype for [u8; N] {
    type Typed = arrow::array::FixedSizeBinaryArray;
    type Value<'a> = &'a [u8; N];
    type Owned = [u8; N];

    fn datatype() -> DataType {
        const {
            assert!(N <= i32::MAX as usize, "FixedSizeBinary size too large");
        }
        #[expect(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        DataType::FixedSizeBinary(N as i32)
    }

    fn downcast(array: &dyn Array) -> Result<Self::Typed, ColumnError> {
        downcast_array::<arrow::array::FixedSizeBinaryArray>(array)
    }

    fn is_null(typed: &Self::Typed, index: usize) -> bool {
        typed.is_null(index)
    }

    fn value(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
        typed
            .value(index)
            .first_chunk::<N>()
            .expect("The length is guaranteed by the validated datatype")
    }

    fn build(values: impl Iterator<Item = Option<Self::Owned>>) -> Result<ArrayRef, ColumnError> {
        const {
            assert!(N <= i32::MAX as usize, "FixedSizeBinary size too large");
        }
        #[expect(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let array =
            arrow::array::FixedSizeBinaryArray::try_from_sparse_iter_with_size(values, N as i32)
                .map_err(ColumnError::Build)?; // Cannot happen: `[u8; N]` values all have the same size
        Ok(std::sync::Arc::new(array))
    }

    fn to_owned_value(value: Self::Value<'_>) -> Self::Owned {
        *value
    }
}

impl<const N: usize> InfallibleBuild for [u8; N] {}

impl<const N: usize> RefDatatype for [u8; N] {
    type Ref = [u8; N];

    fn value_ref(typed: &Self::Typed, index: usize) -> &[u8; N] {
        Self::value(typed, index)
    }
}
