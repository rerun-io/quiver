//! [`TypedArray<L>`]: the data half of a [`Column`](crate::Column) —
//! the arrow array plus its downcast view, without the metadata.

use arrow::array::{Array as _, ArrayRef};
use arrow::datatypes::DataType;

use crate::datatype::{InfallibleBuild, PrimitiveType, RefType};
use crate::{ColumnError, LogicalType};

/// A strongly-typed, validated, zero-copy view of one arrow array:
/// a [`Column`](crate::Column) minus the per-column metadata.
///
/// Validates the array **once, eagerly** at construction
/// (exact datatype, including the inner types of nested arrays, plus nulls at
/// every non-`Option` nesting level). After that, element access is infallible,
/// fully typed, and zero-copy.
///
/// # Relationship to the other main types
/// [`Column<L>`](crate::Column) is this type plus the field metadata of a
/// record batch column, and forwards every value method here. Prefer a
/// `TypedArray` for an array that isn't a record batch column, and so has no
/// metadata to carry; call
/// [`Column::as_typed_array`](crate::Column::as_typed_array) or
/// [`Column::into_typed_array`](crate::Column::into_typed_array) to get at the
/// data half of a column, and `Column::from` to go back.
///
/// [`ColumnDesc::typed_array`](crate::ColumnDesc::typed_array) validates a
/// loose arrow array against a named column's `L`, without you naming that `L`.
///
/// ```
/// # use quiver::{TypedArray, Utf8};
/// let names = TypedArray::<Utf8>::from_values(["Alice", "Bob"]);
/// assert_eq!(names.value(1), "Bob");
/// ```
pub struct TypedArray<L: LogicalType> {
    /// The original arrow array (kept for cheap conversion back to arrow).
    array: ArrayRef,

    /// The fully-downcast representation.
    typed: L::Typed,
}

impl<L: LogicalType> TypedArray<L> {
    /// May the values of this array be null?
    pub const NULLABLE: bool = L::NULLABLE;

    /// Validates the array against the logical type `L` (datatype and nullability,
    /// recursively), then downcasts it (zero-copy).
    ///
    /// # Errors
    /// Errors on datatype mismatch, or on nulls at any non-`Option` nesting level.
    pub fn try_new(array: ArrayRef) -> Result<Self, ColumnError> {
        // Validate (and downcast) the datatype first — `downcast` rejects a wrong
        // datatype, including parameters the concrete arrow array type doesn't
        // encode (a fixed size, a timestamp timezone), and checks all child-level
        // nulls. Doing it before the top-level null check means a datatype
        // mismatch is reported as `WrongDatatype`, not masked by `UnexpectedNulls`.
        let typed = L::downcast(&*array)?;

        if !L::NULLABLE && 0 < array.null_count() {
            return Err(ColumnError::UnexpectedNulls {
                null_count: array.null_count(),
            });
        }

        Ok(Self { array, typed })
    }

    #[must_use]
    #[inline]
    pub fn len(&self) -> usize {
        self.array.len()
    }

    #[must_use]
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.array.is_empty()
    }

    /// The value at `index`, or `None` if out of bounds.
    ///
    /// See [`TypedArray::value`] for the returned view;
    /// [`TypedArray::get_owned`] returns the owned value instead.
    #[must_use]
    #[inline]
    pub fn get(&self, index: usize) -> Option<L::Value<'_>> {
        // SAFETY: bounds checked here, once.
        (index < self.len()).then(|| unsafe { self.value_unchecked(index) })
    }

    /// The owned value at `index`, or `None` if out of bounds —
    /// e.g. `String` (or your newtype) where [`TypedArray::get`] returns `&str`.
    #[must_use]
    pub fn get_owned(&self, index: usize) -> Option<L::Owned> {
        self.get(index).map(L::to_owned_value)
    }

    /// The value at `index`.
    ///
    /// Works for every logical type, returning the zero-copy view
    /// ([`LogicalType::Value`]): `&str`, `i64`, `Option<…>`, an iterator for
    /// `List<…>`, etc.
    /// For the owned value, see [`TypedArray::value_owned`].
    ///
    /// # Panics
    /// If `index` is out of bounds.
    #[must_use]
    #[inline]
    pub fn value(&self, index: usize) -> L::Value<'_> {
        assert!(index < self.len(), "Index {index} out of bounds");
        // SAFETY: bounds checked just above.
        unsafe { self.value_unchecked(index) }
    }

    /// The owned value at `index` — e.g. `String` (or your newtype)
    /// where [`TypedArray::value`] returns `&str`.
    ///
    /// May allocate (e.g. string arrays); for bulk access,
    /// prefer [`TypedArray::iter_owned`] or [`TypedArray::to_vec`].
    ///
    /// # Panics
    /// If `index` is out of bounds.
    #[must_use]
    pub fn value_owned(&self, index: usize) -> L::Owned {
        L::to_owned_value(self.value(index))
    }

    /// The value at `index`, without bounds checking.
    ///
    /// # Safety
    /// `index < self.len()`. That is the only quiver-level precondition; the
    /// read itself relies on arrow's buffer/offset invariants, which the array
    /// upholds by construction (quiver validated its datatype and nullability,
    /// not those internal invariants).
    #[inline]
    pub unsafe fn value_unchecked(&self, index: usize) -> L::Value<'_> {
        // SAFETY: forwarded from the caller's contract.
        unsafe { L::value_unchecked(&self.typed, index) }
    }

    /// Iterates over the zero-copy views ([`LogicalType::Value`]):
    /// `&str`, `i64`, etc — like [`TypedArray::value`], element by element.
    ///
    /// For owned values, see [`TypedArray::iter_owned`].
    #[must_use]
    pub fn iter(&self) -> TypedArrayIter<'_, L> {
        TypedArrayIter {
            array: self,
            index: 0,
            end: self.len(),
        }
    }

    /// Iterates over owned values — e.g. `String` (or your newtype)
    /// where [`TypedArray::iter`] yields `&str`.
    ///
    /// May allocate per element (e.g. string arrays).
    pub fn iter_owned(&self) -> impl Iterator<Item = L::Owned> + '_ {
        self.iter().map(L::to_owned_value)
    }

    /// Consumes the array, iterating over owned values — e.g. `String`
    /// (or your newtype) for a `TypedArray<Utf8>`.
    ///
    /// May allocate per element (e.g. string arrays); for borrowed views,
    /// iterate `&array` (or call [`TypedArray::iter`]) instead.
    #[must_use]
    pub fn into_iter_owned(self) -> TypedArrayIntoIter<L> {
        let end = self.len();
        TypedArrayIntoIter {
            array: self,
            index: 0,
            end,
        }
    }

    /// Copies the values into a `Vec` of owned values,
    /// e.g. `Vec<String>` for a `TypedArray<Utf8>`.
    #[must_use]
    pub fn to_vec(&self) -> Vec<L::Owned> {
        self.iter_owned().collect()
    }

    /// A zero-copy slice of the rows `offset..offset + length`.
    ///
    /// # Panics
    /// If the range is out of bounds (like arrow's `slice`).
    #[must_use]
    pub fn slice(&self, offset: usize, length: usize) -> Self {
        Self::try_new(self.array.slice(offset, length))
            .expect("Cannot fail: slicing preserves datatype and validity")
    }

    /// The underlying arrow array.
    #[must_use]
    pub fn as_arrow(&self) -> &ArrayRef {
        &self.array
    }

    /// Extract the underlying arrow array.
    #[must_use]
    pub fn into_arrow(self) -> ArrayRef {
        self.array
    }

    /// The same array, read as nullable; backs [`Column::optional`](crate::Column::optional).
    ///
    /// Free: nullability lives in the validity bitmap, and
    /// [`LogicalType::Optional`] is bound to the same `Typed`, so there is
    /// nothing to re-validate or re-downcast.
    pub(crate) fn into_optional(self) -> TypedArray<L::Optional> {
        let Self { array, typed } = self;
        TypedArray { array, typed }
    }

    /// The same array, read as non-nullable; backs
    /// [`Column::try_required`](crate::Column::try_required).
    ///
    /// Only the top-level validity needs checking: the child levels were
    /// validated when this array was built, and dropping the `Option` at this
    /// level does not touch them.
    pub(crate) fn try_into_required(self) -> Result<TypedArray<L::Required>, ColumnError> {
        let null_count = self.array.null_count();
        if 0 < null_count {
            return Err(ColumnError::UnexpectedNulls { null_count });
        }

        let Self { array, typed } = self;
        Ok(TypedArray { array, typed })
    }
}

/// Construction and schema, for logical types with a single concrete arrow
/// datatype. (Multi-encoding types like [`AnyList`](crate::AnyList) are
/// parse-only: build a concrete encoding instead.)
impl<L: crate::ConcreteType> TypedArray<L> {
    /// Builds an array from owned values; the fallible form of
    /// [`TypedArray::from_values`], needed only for fallible encodings
    /// (dictionary key overflow).
    ///
    /// # Errors
    /// Errors if the encoding fails, e.g. too many distinct values
    /// for the dictionary key type.
    pub fn try_from_values(
        values: impl IntoIterator<Item = impl Into<L::Owned>>,
    ) -> Result<Self, ColumnError> {
        let array = L::build(values.into_iter().map(|value| Some(value.into())))?;
        Self::try_new(array)
    }

    /// The exact arrow datatype of this array.
    #[must_use]
    pub fn datatype() -> DataType {
        L::datatype()
    }
}

impl<L: InfallibleBuild> TypedArray<L> {
    /// Builds an array from owned values,
    /// e.g. `TypedArray::<Utf8>::from_values(["a", "b"])`.
    ///
    /// Infallible — for the one fallible encoding (dictionaries),
    /// see [`TypedArray::try_from_values`].
    ///
    /// # Panics
    /// Never: the logical type is [`InfallibleBuild`].
    pub fn from_values(values: impl IntoIterator<Item = impl Into<L::Owned>>) -> Self {
        Self::try_from_values(values).expect("Cannot fail: the logical type is InfallibleBuild")
    }
}

impl<L: crate::ConcreteType> TypedArray<Option<L>> {
    /// An array of `len` nulls.
    ///
    /// The `TypedArray` counterpart of [`Column::new_null`](crate::Column::new_null).
    ///
    /// ```
    /// # use quiver::{TypedArray, Utf8};
    /// let array = TypedArray::<Option<Utf8>>::new_null(3);
    /// assert_eq!(array.to_vec(), [None, None, None]);
    /// ```
    ///
    /// # Panics
    /// Panics for run-end encoding, which has no validity of its own — see
    /// [`Column::new_null`](crate::Column::new_null).
    #[must_use]
    pub fn new_null(len: usize) -> Self {
        let array = arrow::array::new_null_array(&L::datatype(), len);
        Self::try_new(array).expect(
            "An all-null array of the right datatype is valid, \
             except for run-end encoding: use `Run<K, Option<V>>`",
        )
    }

    /// Builds a nullable array from optional values; the fallible form of
    /// [`TypedArray::from_nullable_values`].
    ///
    /// # Errors
    /// Errors if the encoding fails, e.g. too many distinct values
    /// for the dictionary key type.
    pub fn try_from_nullable_values(
        values: impl IntoIterator<Item = Option<impl Into<L::Owned>>>,
    ) -> Result<Self, ColumnError> {
        Self::try_from_values(values.into_iter().map(|value| value.map(Into::into)))
    }
}

impl<L: InfallibleBuild> TypedArray<Option<L>> {
    /// Builds a nullable array from optional values.
    ///
    /// Unlike [`TypedArray::from_values`], the values inside the `Option`s may
    /// still need converting, e.g. `Option<&str>` for a `TypedArray<Option<Utf8>>`:
    ///
    /// ```
    /// # use quiver::{TypedArray, Utf8};
    /// let array = TypedArray::<Option<Utf8>>::from_nullable_values([Some("a"), None]);
    /// ```
    pub fn from_nullable_values(
        values: impl IntoIterator<Item = Option<impl Into<L::Owned>>>,
    ) -> Self {
        Self::from_values(values.into_iter().map(|value| value.map(Into::into)))
    }
}

impl<L: RefType> TypedArray<L> {
    /// Like [`TypedArray::value`], but borrows from the array.
    ///
    /// # Panics
    /// If `index` is out of bounds.
    #[inline]
    pub fn value_ref(&self, index: usize) -> &L::Ref {
        assert!(index < self.len(), "Index {index} out of bounds");
        L::value_ref(&self.typed, index)
    }
}

/// `array[index]`: like [`TypedArray::value`], but borrows from the array —
/// `&array[i]` is `&str` for a `TypedArray<Utf8>`, `&i64` for a `TypedArray<i64>`.
///
/// Available for arrays whose values can be borrowed from the array:
/// strings, binaries, and primitives — but not `bool` (bit-packed),
/// nullable (`Option<…>`), or nested (`List<…>`) arrays,
/// whose values are built on the fly.
///
/// Panics if out of bounds (like [`TypedArray::value`]).
impl<L: RefType> std::ops::Index<usize> for TypedArray<L> {
    type Output = L::Ref;

    fn index(&self, index: usize) -> &Self::Output {
        self.value_ref(index)
    }
}

impl<L: PrimitiveType> TypedArray<L> {
    /// The values as a contiguous zero-copy slice,
    /// e.g. `&[f32]` for a `TypedArray<f32>`,
    /// or `&[[u8; 16]]` for a `TypedArray<FixedSizeBinary<16>>`.
    ///
    /// Only available for primitive and fixed-size binary non-nullable arrays
    /// (`bool` is excluded: arrow bit-packs it).
    #[must_use]
    #[inline]
    pub fn as_slice(&self) -> &[L::Native] {
        L::values(&self.typed)
    }
}

impl<L: InfallibleBuild, T: Into<L::Owned>> From<Vec<T>> for TypedArray<L> {
    fn from(values: Vec<T>) -> Self {
        Self::from_values(values)
    }
}

impl<L: InfallibleBuild, T: Into<L::Owned>> FromIterator<T> for TypedArray<L> {
    fn from_iter<I: IntoIterator<Item = T>>(values: I) -> Self {
        Self::from_values(values)
    }
}

/// An empty array. Only for logical types with a single concrete datatype.
impl<L: crate::ConcreteType> Default for TypedArray<L> {
    fn default() -> Self {
        let array = arrow::array::new_empty_array(&L::datatype());
        Self::try_new(array).expect("An empty array of the right datatype is always valid")
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

impl<L: LogicalType> TryFrom<ArrayRef> for TypedArray<L> {
    type Error = ColumnError;

    fn try_from(array: ArrayRef) -> Result<Self, Self::Error> {
        Self::try_new(array)
    }
}

/// Iterator over the values of a [`TypedArray`] (or a [`Column`](crate::Column)).
///
/// The length is fixed and was validated at construction, so each step
/// reads with [`value_unchecked`](LogicalType::value_unchecked) — no
/// per-element bounds check — and the combinators are overridden to skip the
/// default `next`-based `Option` plumbing.
pub struct TypedArrayIter<'a, L: LogicalType> {
    array: &'a TypedArray<L>,
    index: usize,
    end: usize,
}

impl<'a, L: LogicalType + 'a> Iterator for TypedArrayIter<'a, L> {
    type Item = L::Value<'a>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.end {
            // SAFETY: index < end <= array length.
            let value = unsafe { self.array.value_unchecked(self.index) };
            self.index += 1;
            Some(value)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.end - self.index;
        (remaining, Some(remaining))
    }

    fn count(self) -> usize {
        self.end - self.index
    }

    fn last(self) -> Option<Self::Item> {
        // SAFETY: when non-empty, `end - 1` is in `index..end`.
        (self.index < self.end).then(|| unsafe { self.array.value_unchecked(self.end - 1) })
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        match self.index.checked_add(n) {
            Some(target) if target < self.end => {
                self.index = target + 1;
                // SAFETY: target < end <= array length.
                Some(unsafe { self.array.value_unchecked(target) })
            }
            _ => {
                self.index = self.end;
                None
            }
        }
    }

    fn fold<B, F>(self, init: B, mut f: F) -> B
    where
        F: FnMut(B, Self::Item) -> B,
    {
        let Self { array, index, end } = self;
        let mut acc = init;
        for i in index..end {
            // SAFETY: i < end <= array length.
            acc = f(acc, unsafe { array.value_unchecked(i) });
        }
        acc
    }
}

impl<'a, L: LogicalType + 'a> DoubleEndedIterator for TypedArrayIter<'a, L> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.index < self.end {
            self.end -= 1;
            // SAFETY: the new `end` is in `index..old end`, hence in bounds.
            Some(unsafe { self.array.value_unchecked(self.end) })
        } else {
            None
        }
    }
}

impl<'a, L: LogicalType + 'a> ExactSizeIterator for TypedArrayIter<'a, L> {}

impl<'a, L: LogicalType + 'a> std::iter::FusedIterator for TypedArrayIter<'a, L> {}

impl<'a, L: LogicalType + 'a> IntoIterator for &'a TypedArray<L> {
    type Item = L::Value<'a>;
    type IntoIter = TypedArrayIter<'a, L>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// By-value iterator over the owned values of a [`TypedArray`],
/// created by [`TypedArray::into_iter_owned`].
///
/// [`TypedArray`] deliberately does **not** implement [`IntoIterator`] by value:
/// `for x in array` would have to allocate (owned values), so that path is
/// explicit via [`into_iter_owned`](TypedArray::into_iter_owned). Iterate
/// `&array` for the zero-copy borrowed views.
pub struct TypedArrayIntoIter<L: LogicalType> {
    array: TypedArray<L>,
    index: usize,
    end: usize,
}

impl<L: LogicalType> Iterator for TypedArrayIntoIter<L> {
    type Item = L::Owned;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.end {
            // SAFETY: index < end <= array length.
            let value = unsafe { self.array.value_unchecked(self.index) };
            let value = L::to_owned_value(value);
            self.index += 1;
            Some(value)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.end - self.index;
        (remaining, Some(remaining))
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        match self.index.checked_add(n) {
            Some(target) if target < self.end => {
                self.index = target + 1;
                // SAFETY: target < end <= array length.
                Some(L::to_owned_value(unsafe {
                    self.array.value_unchecked(target)
                }))
            }
            _ => {
                self.index = self.end;
                None
            }
        }
    }
}

impl<L: LogicalType> DoubleEndedIterator for TypedArrayIntoIter<L> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.index < self.end {
            self.end -= 1;
            // SAFETY: the new `end` is in `index..old end`, hence in bounds.
            Some(L::to_owned_value(unsafe {
                self.array.value_unchecked(self.end)
            }))
        } else {
            None
        }
    }
}

impl<L: LogicalType> ExactSizeIterator for TypedArrayIntoIter<L> {}

impl<L: LogicalType> std::iter::FusedIterator for TypedArrayIntoIter<L> {}
