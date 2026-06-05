//! [`Column<L>`]: a strongly-typed, validated, zero-copy view of one record batch column.
//!
//! The wrapper validates the arrow array **once, eagerly** at construction
//! (exact datatype, including the inner types of nested arrays, plus nulls at
//! every non-`Option` nesting level). After that, element access is infallible,
//! fully typed, and zero-copy.

use arrow::array::{Array as _, ArrayRef};
use arrow::datatypes::DataType;

use crate::datatype::InfallibleBuild;
use crate::{ColumnError, Datatype};

/// A strongly-typed, validated, zero-copy view of one record batch column.
///
/// The logical type `L` describes the exact datatype and nullability,
/// e.g. `Column<List<String>>` or `Column<Option<i64>>`.
pub struct Column<L: Datatype> {
    /// The original arrow array (kept for cheap conversion back to arrow).
    array: ArrayRef,

    /// The fully-downcast representation.
    typed: L::Typed,

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
        let expected = L::datatype();
        let actual = array.data_type();
        if !crate::datatype::datatypes_compatible(actual, &expected) {
            return Err(ColumnError::WrongDatatype {
                expected,
                actual: actual.clone(),
            });
        }

        if !L::NULLABLE && 0 < array.null_count() {
            return Err(ColumnError::UnexpectedNulls {
                null_count: array.null_count(),
            });
        }

        let typed = L::downcast(&*array)?;
        Ok(Self {
            array,
            typed,
            metadata: std::collections::BTreeMap::new(),
        })
    }

    /// Per-column metadata, stored on the arrow [`arrow::datatypes::Field`]
    /// when converting to/from a record batch.
    pub fn metadata(&self) -> &std::collections::BTreeMap<String, String> {
        &self.metadata
    }

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

    /// The exact arrow datatype of this column.
    pub fn datatype() -> DataType {
        L::datatype()
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

    /// The value at `index`.
    ///
    /// Panics if out of bounds.
    pub fn value(&self, index: usize) -> L::Value<'_> {
        assert!(index < self.len(), "Index {index} out of bounds");
        L::value(&self.typed, index)
    }

    pub fn iter(&self) -> ColumnIter<'_, L> {
        ColumnIter {
            column: self,
            index: 0,
        }
    }

    /// Iterates over owned values, e.g. `String` instead of `&str`.
    pub fn iter_owned(&self) -> impl Iterator<Item = L::Owned> + '_ {
        self.iter().map(L::to_owned_value)
    }

    /// Copies the values into a `Vec` of owned values,
    /// e.g. `Vec<String>` for a `Column<String>`.
    pub fn to_vec(&self) -> Vec<L::Owned> {
        self.iter_owned().collect()
    }

    /// A zero-copy slice of the rows `offset..offset + length`.
    ///
    /// Panics if the range is out of bounds (like arrow's `slice`).
    /// The metadata is preserved.
    #[must_use]
    pub fn slice(&self, offset: usize, length: usize) -> Self {
        Self::try_new(self.array.slice(offset, length))
            .expect("Cannot fail: slicing preserves datatype and validity")
            .with_metadata(self.metadata.clone())
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
impl<L: Datatype> Default for Column<L> {
    fn default() -> Self {
        let array = arrow::array::new_empty_array(&L::datatype());
        Self::try_new(array).expect("An empty array of the right datatype is always valid")
    }
}

/// Compares the data (like arrow array equality) and the metadata.
impl<L: Datatype> PartialEq for Column<L> {
    fn eq(&self, other: &Self) -> bool {
        self.metadata == other.metadata && self.array.as_ref() == other.array.as_ref()
    }
}

impl<L: Datatype> Clone for Column<L> {
    fn clone(&self) -> Self {
        Self {
            array: ArrayRef::clone(&self.array),
            typed: self.typed.clone(),
            metadata: self.metadata.clone(),
        }
    }
}

impl<L: Datatype> std::fmt::Debug for Column<L> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Column")
            .field("array", &self.array)
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
