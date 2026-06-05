//! [`Column<L>`]: a strongly-typed, validated, zero-copy view of one record batch column.
//!
//! The wrapper validates the arrow array **once, eagerly** at construction
//! (exact datatype, including the inner types of nested arrays, plus nulls at
//! every non-`Option` nesting level). After that, element access is infallible,
//! fully typed, and zero-copy.

use arrow::array::ArrayRef;
use arrow::datatypes::DataType;

use crate::datatype::{InfallibleBuild, PrimitiveDatatype, RefDatatype};
use crate::typed_array::TypedArray;
use crate::{ColumnError, Datatype};

/// A strongly-typed, validated, zero-copy view of one record batch column.
///
/// The logical type `L` describes the exact datatype and nullability,
/// e.g. `Column<List<String>>` or `Column<Option<i64>>`.
pub struct Column<L: Datatype> {
    /// The data: the arrow array plus its downcast view.
    array: TypedArray<L>,

    /// Per-column metadata, stored on the arrow [`arrow::datatypes::Field`]
    /// when converting to/from a record batch.
    metadata: std::collections::BTreeMap<String, String>,
}

impl<L: Datatype> Column<L> {
    /// May the values of this column be null?
    pub const NULLABLE: bool = L::NULLABLE;

    /// Validates the array against the logical type `L` (datatype and nullability,
    /// recursively), then downcasts it (zero-copy).
    ///
    /// # Errors
    /// Errors on datatype mismatch, or on nulls at any non-`Option` nesting level.
    pub fn try_new(array: ArrayRef) -> Result<Self, ColumnError> {
        Ok(Self {
            array: TypedArray::try_new(array)?,
            metadata: std::collections::BTreeMap::new(),
        })
    }

    /// Per-column metadata, stored on the arrow [`arrow::datatypes::Field`]
    /// when converting to/from a record batch.
    #[must_use]
    pub fn metadata(&self) -> &std::collections::BTreeMap<String, String> {
        &self.metadata
    }

    /// Mutable access to the per-column metadata; see [`Column::metadata`].
    pub fn metadata_mut(&mut self) -> &mut std::collections::BTreeMap<String, String> {
        &mut self.metadata
    }

    /// Replace the per-column metadata.
    #[must_use]
    pub fn with_metadata(mut self, metadata: std::collections::BTreeMap<String, String>) -> Self {
        self.metadata = metadata;
        self
    }

    /// Builds a column from owned values; the fallible form of
    /// [`Column::from_values`], needed only for fallible encodings
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

    /// The exact arrow datatype of this column, or `None` if the logical type
    /// has no static datatype (it contains a [`Dyn`](crate::Dyn) leaf).
    #[must_use]
    pub fn datatype() -> Option<DataType> {
        L::datatype()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.array.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.array.is_empty()
    }

    /// The value at `index`, or `None` if out of bounds.
    ///
    /// See [`Column::value`] for the returned view;
    /// [`Column::get_owned`] returns the owned value instead.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<L::Value<'_>> {
        self.array.get(index)
    }

    /// The owned value at `index`, or `None` if out of bounds —
    /// e.g. `String` (or your newtype) where [`Column::get`] returns `&str`.
    #[must_use]
    pub fn get_owned(&self, index: usize) -> Option<L::Owned> {
        self.get(index).map(L::to_owned_value)
    }

    /// The value at `index`.
    ///
    /// Works for every logical type, returning the zero-copy view
    /// ([`Datatype::Value`]): `&str`, `i64`, `Option<…>`, an iterator for
    /// `List<…>`, etc.
    /// Where a plain reference exists in the array — strings, binaries,
    /// primitives (but not `bool`, `Option<…>`, or `List<…>`) — `column[index]`
    /// works too, and is handy with generic code expecting `&T`.
    /// For the owned value (e.g. `String`, or your `newtype_datatype!` type),
    /// see [`Column::value_owned`].
    ///
    /// Panics if out of bounds.
    #[must_use]
    pub fn value(&self, index: usize) -> L::Value<'_> {
        self.array.value(index)
    }

    /// The owned value at `index` — e.g. `String` (or your newtype)
    /// where [`Column::value`] returns `&str`.
    ///
    /// May allocate (e.g. string columns); for bulk access,
    /// prefer [`Column::iter_owned`] or [`Column::to_vec`].
    ///
    /// Panics if out of bounds.
    #[must_use]
    pub fn value_owned(&self, index: usize) -> L::Owned {
        L::to_owned_value(self.value(index))
    }

    /// Iterates over the zero-copy views ([`Datatype::Value`]):
    /// `&str`, `i64`, etc — like [`Column::value`], element by element.
    ///
    /// For owned values, see [`Column::iter_owned`].
    #[must_use]
    pub fn iter(&self) -> ColumnIter<'_, L> {
        ColumnIter {
            column: self,
            index: 0,
        }
    }

    /// Iterates over owned values — e.g. `String` (or your newtype)
    /// where [`Column::iter`] yields `&str`.
    ///
    /// May allocate per element (e.g. string columns).
    pub fn iter_owned(&self) -> impl Iterator<Item = L::Owned> + '_ {
        self.iter().map(L::to_owned_value)
    }

    /// Copies the values into a `Vec` of owned values,
    /// e.g. `Vec<String>` for a `Column<String>`.
    #[must_use]
    pub fn to_vec(&self) -> Vec<L::Owned> {
        self.iter_owned().collect()
    }

    /// A zero-copy slice of the rows `offset..offset + length`.
    ///
    /// Panics if the range is out of bounds (like arrow's `slice`).
    /// The metadata is preserved.
    #[must_use]
    pub fn slice(&self, offset: usize, length: usize) -> Self {
        Self::try_new(self.array.as_arrow().slice(offset, length))
            .expect("Cannot fail: slicing preserves datatype and validity")
            .with_metadata(self.metadata.clone())
    }

    /// The underlying arrow array.
    #[must_use]
    pub fn as_arrow(&self) -> &ArrayRef {
        self.array.as_arrow()
    }

    /// Extract the underlying arrow array.
    #[must_use]
    pub fn into_arrow(self) -> ArrayRef {
        self.array.into_arrow()
    }
}

/// `column[index]`: like [`Column::value`], but borrows from the array —
/// `&column[i]` is `&str` for a `Column<String>`, `&i64` for a `Column<i64>`.
///
/// Available for columns whose values can be borrowed from the array:
/// strings, binaries, and primitives — but not `bool` (bit-packed),
/// nullable (`Option<…>`), or nested (`List<…>`) columns,
/// whose values are built on the fly.
///
/// Panics if out of bounds (like [`Column::value`]).
///
/// ```
/// # use quiver::Column;
/// let strings = Column::<String>::from_values(["a", "b"]);
/// assert_eq!(&strings[1], "b");
///
/// let numbers = Column::<i64>::from_values([1, 2, 3]);
/// assert_eq!(numbers[2], 3);
/// ```
impl<L: RefDatatype> std::ops::Index<usize> for Column<L> {
    type Output = L::Ref;

    fn index(&self, index: usize) -> &Self::Output {
        self.array.value_ref(index)
    }
}

impl<L: PrimitiveDatatype> Column<L> {
    /// The values as a contiguous zero-copy slice,
    /// e.g. `&[f32]` for a `Column<f32>`,
    /// or `&[[u8; 16]]` for a `Column<[u8; 16]>` (fixed-size binary).
    ///
    /// Only available for primitive and fixed-size binary non-nullable columns
    /// (`bool` is excluded: arrow bit-packs it).
    ///
    /// ```
    /// # use quiver::Column;
    /// let column = Column::<f32>::from_values([1.0, 2.0, 3.0]);
    /// assert_eq!(column.as_slice(), &[1.0, 2.0, 3.0]);
    ///
    /// let hashes = Column::<[u8; 4]>::from_values([[1, 2, 3, 4], [5, 6, 7, 8]]);
    /// assert_eq!(hashes.as_slice(), &[[1, 2, 3, 4], [5, 6, 7, 8]]);
    /// ```
    #[must_use]
    pub fn as_slice(&self) -> &[L::Native] {
        self.array.values()
    }
}

impl<L: Datatype> Column<L>
where
    L: InfallibleBuild,
{
    /// Builds a column from owned values,
    /// e.g. `Column::<String>::from_values(["a", "b"])`.
    ///
    /// Infallible — for the one fallible encoding (dictionaries),
    /// see [`Column::try_from_values`].
    pub fn from_values(values: impl IntoIterator<Item = impl Into<L::Owned>>) -> Self {
        Self::try_from_values(values).expect("Cannot fail: the logical type is InfallibleBuild")
    }
}

impl<L: Datatype> Column<Option<L>> {
    /// Builds a nullable column from optional values; the fallible form of
    /// [`Column::from_nullable_values`].
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

impl<L: InfallibleBuild> Column<Option<L>> {
    /// Builds a nullable column from optional values.
    ///
    /// Unlike [`Column::from_values`], the values inside the `Option`s may still
    /// need converting, e.g. `Option<&str>` for a `Column<Option<String>>`:
    ///
    /// ```
    /// # use quiver::Column;
    /// let column = Column::<Option<String>>::from_nullable_values([Some("a"), None]);
    /// ```
    pub fn from_nullable_values(
        values: impl IntoIterator<Item = Option<impl Into<L::Owned>>>,
    ) -> Self {
        Self::from_values(values.into_iter().map(|value| value.map(Into::into)))
    }
}

impl<L: InfallibleBuild, T: Into<L::Owned>> From<Vec<T>> for Column<L> {
    fn from(values: Vec<T>) -> Self {
        Self::from_values(values)
    }
}

impl<L: InfallibleBuild, T: Into<L::Owned>> FromIterator<T> for Column<L> {
    fn from_iter<I: IntoIterator<Item = T>>(values: I) -> Self {
        Self::from_values(values)
    }
}

/// An empty column.
///
/// Panics for logical types without a static datatype ([`Dyn`](crate::Dyn)-containing) —
/// there is no datatype to make the empty array with.
impl<L: Datatype> Default for Column<L> {
    fn default() -> Self {
        let datatype =
            L::datatype().expect("Cannot make an empty column without a static datatype");
        let array = arrow::array::new_empty_array(&datatype);
        Self::try_new(array).expect("An empty array of the right datatype is always valid")
    }
}

/// Compares the data (like arrow array equality) and the metadata.
impl<L: Datatype> PartialEq for Column<L> {
    fn eq(&self, other: &Self) -> bool {
        self.metadata == other.metadata && self.array == other.array
    }
}

impl<L: Datatype> Clone for Column<L> {
    fn clone(&self) -> Self {
        Self {
            array: self.array.clone(),
            metadata: self.metadata.clone(),
        }
    }
}

impl<L: Datatype> std::fmt::Debug for Column<L> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Column")
            .field("array", self.array.as_arrow())
            .field("metadata", &self.metadata)
            .finish_non_exhaustive()
    }
}

impl<L: Datatype> TryFrom<ArrayRef> for Column<L> {
    type Error = ColumnError;

    fn try_from(array: ArrayRef) -> Result<Self, Self::Error> {
        Self::try_new(array)
    }
}

/// Iterator over the values of a [`Column`].
pub struct ColumnIter<'a, L: Datatype> {
    column: &'a Column<L>,
    index: usize,
}

impl<'a, L: Datatype + 'a> Iterator for ColumnIter<'a, L> {
    type Item = L::Value<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let value = self.column.get(self.index)?;
        self.index += 1;
        Some(value)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.column.len() - self.index;
        (remaining, Some(remaining))
    }
}

impl<'a, L: Datatype + 'a> ExactSizeIterator for ColumnIter<'a, L> {}

impl<'a, L: Datatype + 'a> IntoIterator for &'a Column<L> {
    type Item = L::Value<'a>;
    type IntoIter = ColumnIter<'a, L>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Iterating a `Column` by value yields owned values, like a `Vec` —
/// e.g. `String` for a `Column<String>`.
impl<L: Datatype> IntoIterator for Column<L> {
    type Item = L::Owned;
    type IntoIter = ColumnIntoIter<L>;

    fn into_iter(self) -> Self::IntoIter {
        ColumnIntoIter {
            column: self,
            index: 0,
        }
    }
}

/// By-value iterator over the owned values of a [`Column`].
pub struct ColumnIntoIter<L: Datatype> {
    column: Column<L>,
    index: usize,
}

impl<L: Datatype> Iterator for ColumnIntoIter<L> {
    type Item = L::Owned;

    fn next(&mut self) -> Option<Self::Item> {
        let value = self.column.get(self.index)?;
        let value = L::to_owned_value(value);
        self.index += 1;
        Some(value)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.column.len() - self.index;
        (remaining, Some(remaining))
    }
}

impl<L: Datatype> ExactSizeIterator for ColumnIntoIter<L> {}
