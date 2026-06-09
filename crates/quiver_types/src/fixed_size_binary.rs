//! [`FixedSizeBinary<N>`]: a logical type for columns of fixed-size byte arrays.
//!
//! A `Column<FixedSizeBinary<16>>` is a column where every element is exactly
//! 16 bytes (e.g. UUIDs or hashes), stored as an
//! [`arrow::array::FixedSizeBinaryArray`] ([`DataType::FixedSizeBinary`]).
//! The size is part of the type, checked at the parse boundary;
//! the element values are `&[u8; N]` and the owned values are `[u8; N]`.

use arrow::array::{Array, ArrayRef};
use arrow::datatypes::DataType;

use crate::datatype::{
    ColumnError, InfallibleBuild, LogicalType, PrimitiveType, RefType, downcast_array,
};

/// Marker for an arrow `FixedSizeBinary(N)` column, e.g. `FixedSizeBinary<16>`
/// for UUIDs.
///
/// The element values are `&[u8; N]`; the owned values are `[u8; N]`.
///
/// ```
/// use quiver::{Column, FixedSizeBinary};
///
/// let column = Column::<FixedSizeBinary<4>>::from_values([[1, 2, 3, 4], [5, 6, 7, 8]]);
/// assert_eq!(column.value(0), &[1, 2, 3, 4]);
/// assert_eq!(column.as_slice(), &[[1, 2, 3, 4], [5, 6, 7, 8]]); // bulk, zero-copy
/// ```
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct FixedSizeBinary<const N: usize>;

impl<const N: usize> LogicalType for FixedSizeBinary<N> {
    type Typed = arrow::array::FixedSizeBinaryArray;
    type Value<'a> = &'a [u8; N];
    type Owned = [u8; N];

    fn matches(actual: &DataType) -> bool {
        matches!(actual, DataType::FixedSizeBinary(n) if usize::try_from(*n) == Ok(N))
    }

    fn supported_datatypes() -> Vec<DataType> {
        vec![<Self as crate::ConcreteType>::datatype()]
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

    fn to_owned_value(value: Self::Value<'_>) -> Self::Owned {
        *value
    }
}

impl<const N: usize> crate::ConcreteType for FixedSizeBinary<N> {
    fn datatype() -> DataType {
        const {
            assert!(N <= i32::MAX as usize, "FixedSizeBinary size too large");
        }
        #[expect(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        DataType::FixedSizeBinary(N as i32)
    }

    fn build(values: impl Iterator<Item = Option<Self::Owned>>) -> Result<ArrayRef, ColumnError> {
        const {
            assert!(N <= i32::MAX as usize, "FixedSizeBinary size too large");
        }
        #[expect(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let array =
            arrow::array::FixedSizeBinaryArray::try_from_sparse_iter_with_size(values, N as i32)
                .map_err(ColumnError::Build)?; // Cannot happen: the values all have the same size
        Ok(std::sync::Arc::new(array))
    }
}

impl<const N: usize> InfallibleBuild for FixedSizeBinary<N> {}

/// Enables the bulk zero-copy [`Column::as_slice`](crate::Column::as_slice):
/// `&[[u8; N]]` for a `Column<FixedSizeBinary<N>>`.
impl<const N: usize> PrimitiveType for FixedSizeBinary<N> {
    type Native = [u8; N];

    fn values(typed: &Self::Typed) -> &[Self::Native] {
        const {
            assert!(
                0 < N,
                "as_slice() is not available for FixedSizeBinary<0> columns"
            );
        }
        // The buffer of a `FixedSizeBinaryArray` is normalized on construction
        // and slicing: `value_data()` is exactly the `len * N` bytes of the
        // logical window, with no leading offset.
        let (chunks, remainder) = typed.value_data().as_chunks::<N>();
        debug_assert!(remainder.is_empty(), "Guaranteed by the validated datatype");
        chunks
    }
}

impl<const N: usize> RefType for FixedSizeBinary<N> {
    type Ref = [u8; N];

    fn value_ref(typed: &Self::Typed, index: usize) -> &[u8; N] {
        Self::value(typed, index)
    }
}
