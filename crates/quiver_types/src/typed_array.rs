//! [`TypedArray<L>`]: the data half of a [`Column`](crate::Column) —
//! the arrow array plus its downcast view, without the metadata.

use arrow::array::{Array as _, ArrayRef};

use crate::datatype::{PrimitiveType, RefType};
use crate::{ColumnError, LogicalType};

/// A strongly-typed, validated, zero-copy view of one arrow array:
/// a [`Column`](crate::Column) minus the per-column metadata.
///
/// Validates the array **once, eagerly** at construction
/// (exact datatype, including the inner types of nested arrays, plus nulls at
/// every non-`Option` nesting level). After that, element access is infallible,
/// fully typed, and zero-copy.
pub(crate) struct TypedArray<L: LogicalType> {
    /// The original arrow array (kept for cheap conversion back to arrow).
    array: ArrayRef,

    /// The fully-downcast representation.
    typed: L::Typed,
}

impl<L: LogicalType> TypedArray<L> {
    /// Validates the array against the logical type `L` (datatype and nullability,
    /// recursively), then downcasts it (zero-copy).
    ///
    /// # Errors
    /// Errors on datatype mismatch, or on nulls at any non-`Option` nesting level.
    pub fn try_new(array: ArrayRef) -> Result<Self, ColumnError> {
        // `matches` and `downcast` are complementary — both are needed:
        //
        // * `matches` checks the *arrow datatype*, including the parameters that
        //   are not encoded in the concrete array's Rust type and so are
        //   invisible to `downcast`: a `FixedSizeBinary`/`FixedSizeList` size, a
        //   timestamp's timezone, etc. (`FixedSizeBinaryArray` is one Rust type
        //   for every size.) Skipping it would let those mismatches through —
        //   into wrong reads or panics (e.g. `first_chunk::<N>` on a too-short
        //   element). It also produces the good "expected vs found" error.
        //
        // * `downcast` then assumes the datatype is already valid (see its
        //   contract): it just performs the zero-copy downcasts and validates
        //   *nulls* at every non-`Option` nesting level.
        let actual = array.data_type();
        if !L::matches(actual) {
            return Err(ColumnError::WrongDatatype {
                supported: L::supported_datatypes(),
                actual: actual.clone(),
            });
        }

        // Top-level nulls; child-level nulls are checked inside `downcast`.
        if !L::NULLABLE && 0 < array.null_count() {
            return Err(ColumnError::UnexpectedNulls {
                null_count: array.null_count(),
            });
        }

        let typed = L::downcast(&*array)?;
        Ok(Self { array, typed })
    }

    pub fn len(&self) -> usize {
        self.array.len()
    }

    pub fn is_empty(&self) -> bool {
        self.array.is_empty()
    }

    /// The value at `index`, or `None` if out of bounds.
    pub fn get(&self, index: usize) -> Option<L::Value<'_>> {
        (index < self.len()).then(|| L::value(&self.typed, index))
    }

    /// The value at `index`. Panics if out of bounds.
    pub fn value(&self, index: usize) -> L::Value<'_> {
        assert!(index < self.len(), "Index {index} out of bounds");
        L::value(&self.typed, index)
    }

    /// The underlying arrow array.
    pub fn as_arrow(&self) -> &ArrayRef {
        &self.array
    }

    /// Extract the underlying arrow array.
    pub fn into_arrow(self) -> ArrayRef {
        self.array
    }
}

impl<L: RefType> TypedArray<L> {
    /// Like [`TypedArray::value`], but borrows from the array.
    /// Panics if out of bounds.
    pub fn value_ref(&self, index: usize) -> &L::Ref {
        assert!(index < self.len(), "Index {index} out of bounds");
        L::value_ref(&self.typed, index)
    }
}

impl<L: PrimitiveType> TypedArray<L> {
    /// The values as a contiguous zero-copy slice.
    pub fn values(&self) -> &[L::Native] {
        L::values(&self.typed)
    }
}

/// Compares the data (like arrow array equality).
impl<L: LogicalType> PartialEq for TypedArray<L> {
    fn eq(&self, other: &Self) -> bool {
        self.array.as_ref() == other.array.as_ref()
    }
}

impl<L: LogicalType> Clone for TypedArray<L> {
    fn clone(&self) -> Self {
        Self {
            array: ArrayRef::clone(&self.array),
            typed: self.typed.clone(),
        }
    }
}

impl<L: LogicalType> std::fmt::Debug for TypedArray<L> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TypedArray")
            .field("array", &self.array)
            .finish_non_exhaustive()
    }
}
