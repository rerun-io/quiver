//! [`FixedSizeBinary<N>`]: a logical type for columns of fixed-size byte arrays.
//!
//! A `Column<FixedSizeBinary<16>>` is a column where every element is exactly
//! 16 bytes (e.g. UUIDs or hashes), stored as an
//! [`arrow::array::FixedSizeBinaryArray`] ([`DataType::FixedSizeBinary`]).
//! The size is part of the type, checked at the parse boundary;
//! the element values are `&[u8; N]` and the owned values are `[u8; N]`.

use arrow::array::{Array, ArrayRef};
use arrow::datatypes::DataType;

use crate::data_type::{
    ColumnError, InfallibleBuild, LogicalType, PrimitiveBuild, PrimitiveType, RefType,
    downcast_array,
};

/// Marker for an arrow `FixedSizeBinary(N)` column, e.g. `FixedSizeBinary<16>`
/// for UUIDs.
///
/// The element values are `&[u8; N]`; the owned values are `[u8; N]`.
///
/// ```
/// use quiver::{FixedSizeBinary, TypedArray};
///
/// let array = TypedArray::<FixedSizeBinary<4>>::from_values([[1, 2, 3, 4], [5, 6, 7, 8]]);
/// assert_eq!(array.value(0), &[1, 2, 3, 4]);
/// assert_eq!(array.as_slice(), &[[1, 2, 3, 4], [5, 6, 7, 8]]); // bulk, zero-copy
/// ```
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct FixedSizeBinary<const N: usize>;

impl<const N: usize> LogicalType for FixedSizeBinary<N> {
    type Typed = arrow::array::FixedSizeBinaryArray;
    type Value<'a> = &'a [u8; N];
    type Owned = [u8; N];
    type Optional = Option<Self>;
    type Required = Self;

    fn downcast(array: &dyn Array) -> Result<Self::Typed, ColumnError> {
        // The element width is not in `FixedSizeBinaryArray`'s Rust type, so
        // check it here (a `FixedSizeBinary(8)` would otherwise read as `<16>`
        // and panic in `value`).
        let expected = || format!("FixedSizeBinary({N})");
        if matches!(array.data_type(), DataType::FixedSizeBinary(n) if usize::try_from(*n) == Ok(N))
        {
            downcast_array::<arrow::array::FixedSizeBinaryArray>(array, expected)
        } else {
            Err(ColumnError::WrongDataType {
                expected: expected(),
                actual: array.data_type().clone(),
            })
        }
    }

    fn slice_typed(typed: &Self::Typed, offset: usize, length: usize) -> Option<Self::Typed> {
        // A leaf array slices itself; nothing to re-validate. Arrow normalizes
        // the value data on slice, so the element width still holds.
        Some(typed.slice(offset, length))
    }

    #[inline]
    fn is_null(typed: &Self::Typed, index: usize) -> bool {
        typed.is_null(index)
    }

    #[inline]
    unsafe fn is_null_unchecked(typed: &Self::Typed, index: usize) -> bool {
        // SAFETY: the caller guarantees `index` is in bounds.
        unsafe { crate::data_type::leaf_is_null_unchecked(typed, index) }
    }

    #[inline]
    fn value(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
        typed
            .value(index)
            .first_chunk::<N>()
            .expect("The length is guaranteed by the validated data type")
    }

    #[inline]
    unsafe fn value_unchecked(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
        // SAFETY: the caller guarantees `index` is in bounds.
        unsafe { typed.value_unchecked(index) }
            .first_chunk::<N>()
            .expect("The length is guaranteed by the validated data type")
    }

    fn to_owned_value(value: Self::Value<'_>) -> Self::Owned {
        *value
    }
}

impl<const N: usize> crate::ConcreteType for FixedSizeBinary<N> {
    fn data_type() -> DataType {
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
        debug_assert!(
            remainder.is_empty(),
            "Guaranteed by the validated data type"
        );
        chunks
    }
}

/// Enables the bulk [`TypedArray::from_slice`](crate::TypedArray::from_slice):
/// one `memcpy` of the `len * N` bytes, instead of a per-element builder.
impl<const N: usize> PrimitiveBuild for FixedSizeBinary<N> {
    fn array_from_slice(values: &[Self::Native]) -> ArrayRef {
        Self::from_byte_buffer(arrow::buffer::Buffer::from(values.as_flattened()))
    }

    fn array_from_vec(values: Vec<Self::Native>) -> ArrayRef {
        // `Vec<[u8; N]>` → `Vec<u8>` → `Buffer` keeps the same allocation.
        Self::from_byte_buffer(arrow::buffer::Buffer::from_vec(values.into_flattened()))
    }

    fn array_from_buffer(buffer: arrow::buffer::Buffer) -> Result<ArrayRef, ColumnError> {
        const {
            assert!(
                0 < N,
                "from_slice() / from_buffer() are not available for FixedSizeBinary<0> arrays: \
                 zero-width elements carry no length"
            );
            assert!(N <= i32::MAX as usize, "FixedSizeBinary size too large");
        }

        // `FixedSizeBinaryArray::try_new` would silently round a trailing
        // partial element down, so reject it here.
        if !buffer.len().is_multiple_of(N) {
            return Err(ColumnError::Build(
                arrow::error::ArrowError::InvalidArgumentError(format!(
                    "A buffer of {} bytes is not a whole number of {N}-byte elements",
                    buffer.len()
                )),
            ));
        }

        #[expect(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let array = arrow::array::FixedSizeBinaryArray::try_new(N as i32, buffer, None)
            .map_err(ColumnError::Build)?;
        Ok(std::sync::Arc::new(array))
    }
}

impl<const N: usize> FixedSizeBinary<N> {
    /// Wraps `len * N` bytes, which is every buffer the callers above can build.
    fn from_byte_buffer(buffer: arrow::buffer::Buffer) -> ArrayRef {
        Self::array_from_buffer(buffer)
            .expect("Cannot fail: N bytes per element, and bytes need no alignment")
    }
}

impl<const N: usize> RefType for FixedSizeBinary<N> {
    type Ref = [u8; N];

    fn value_ref(typed: &Self::Typed, index: usize) -> &[u8; N] {
        Self::value(typed, index)
    }
}
