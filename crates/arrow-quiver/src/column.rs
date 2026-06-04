//! Strongly-typed column wrappers: [`Column<L>`] parameterized by a logical type `L`.
//!
//! The wrapper validates the arrow array **once, eagerly** at construction
//! (exact datatype, including the inner types of nested arrays, plus nulls at
//! every non-`Option` nesting level). After that, element access is infallible,
//! fully typed, and zero-copy.

use std::marker::PhantomData;

use arrow::array::{Array, ArrayRef};
use arrow::datatypes::ArrowNativeType as _;
use arrow::datatypes::DataType;

use crate::ErrorKind;

/// A logical column type, e.g. `String`, `Option<i64>`, or `List<String>`.
///
/// `Option<L>` means the values at this nesting level may be null.
pub trait Datatype {
    /// May the values at this level be null? (`true` only for `Option<…>`)
    const NULLABLE: bool = false;

    /// The fully-downcast, validated representation of one column of this datatype.
    /// Cheap to clone (arrow arrays are reference-counted).
    type Typed: Clone;

    /// Zero-copy element view: `&'a str` for `String`, `i64` for `i64`,
    /// an iterator for `List<T>`, `Option<…>` for `Option<T>`.
    type Value<'a>
    where
        Self: 'a;

    /// The owned value of one element, used by the convenience constructors:
    /// `String` for `String`, `Option<i64>` for `Option<i64>`, `Vec<…>` for `List<…>`, etc.
    type Owned;

    /// The exact arrow datatype, built recursively
    /// (including the nullability of inner fields).
    fn datatype() -> DataType;

    /// Recursively downcasts the array, checking the nulls of all *children*.
    ///
    /// Nulls at the level of `array` itself are the responsibility of the caller
    /// (the parent datatype, or [`Column::try_new`] at the top level),
    /// because only the caller knows if this level is wrapped in an `Option`.
    ///
    /// # Errors
    /// Errors on unexpected nulls in children.
    /// The datatype is assumed to have already been checked (see [`Column::try_new`]),
    /// making the downcasts themselves infallible.
    fn downcast(array: &dyn Array) -> Result<Self::Typed, ColumnError>;

    /// Is the value at `index` null?
    fn is_null(typed: &Self::Typed, index: usize) -> bool;

    /// The value at `index`.
    ///
    /// Contract: `index` is in bounds, and the value is non-null unless `Self` is an `Option`.
    fn value(typed: &Self::Typed, index: usize) -> Self::Value<'_>;

    /// Builds an arrow array of this datatype from owned values.
    ///
    /// `None` items only ever occur at `Option<…>` levels.
    fn build(values: impl Iterator<Item = Option<Self::Owned>>) -> ArrayRef;
}

/// What can go wrong when constructing a [`Column`].
///
/// Does not know which column it concerns — see [`ColumnError::for_column`].
#[derive(Debug, thiserror::Error)]
pub enum ColumnError {
    #[error("Expected datatype {expected:?}, found {actual:?}")]
    WrongDatatype {
        expected: DataType,
        actual: DataType,
    },

    #[error(
        "Found {null_count} null(s) at a non-nullable level. Use `Option<…>` in the logical type to allow nulls"
    )]
    UnexpectedNulls { null_count: usize },
}

impl ColumnError {
    /// Attach the column name, producing an [`ErrorKind`].
    pub fn for_column(self, column: String) -> ErrorKind {
        match self {
            Self::WrongDatatype { expected, actual } => ErrorKind::WrongDatatype {
                column,
                expected,
                actual,
            },
            Self::UnexpectedNulls { null_count } => {
                ErrorKind::UnexpectedNulls { column, null_count }
            }
        }
    }
}

/// Lets `?` convert column errors in functions returning arrow results.
///
/// The error is preserved (including its source chain),
/// wrapped as an [`arrow::error::ArrowError::ExternalError`].
impl From<ColumnError> for arrow::error::ArrowError {
    fn from(err: ColumnError) -> Self {
        Self::ExternalError(Box::new(err))
    }
}

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
        if actual != &expected {
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

    /// Builds a column from owned values,
    /// e.g. `Column::<String>::from_values(["a", "b"])`.
    pub fn from_values(values: impl IntoIterator<Item = impl Into<L::Owned>>) -> Self {
        let array = L::build(values.into_iter().map(|value| Some(value.into())));
        Self::try_new(array).expect("The built array always matches the datatype")
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

    /// The underlying arrow array.
    pub fn as_arrow(&self) -> &ArrayRef {
        &self.array
    }

    /// Extract the underlying arrow array.
    pub fn into_arrow(self) -> ArrayRef {
        self.array
    }
}

impl<L: Datatype, T: Into<L::Owned>> From<Vec<T>> for Column<L> {
    fn from(values: Vec<T>) -> Self {
        Self::from_values(values)
    }
}

impl<L: Datatype, T: Into<L::Owned>> FromIterator<T> for Column<L> {
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

// ----------------------------------------------------------------------------
// Logical types

/// `Option<L>`: the values at this level may be null.
impl<L: Datatype> Datatype for Option<L> {
    const NULLABLE: bool = true;

    type Typed = L::Typed;
    type Value<'a>
        = Option<L::Value<'a>>
    where
        Self: 'a;
    type Owned = Option<L::Owned>;

    fn datatype() -> DataType {
        L::datatype()
    }

    fn downcast(array: &dyn Array) -> Result<Self::Typed, ColumnError> {
        L::downcast(array)
    }

    fn is_null(typed: &Self::Typed, index: usize) -> bool {
        L::is_null(typed, index)
    }

    fn value(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
        if L::is_null(typed, index) {
            None
        } else {
            Some(L::value(typed, index))
        }
    }

    fn build(values: impl Iterator<Item = Option<Self::Owned>>) -> ArrayRef {
        L::build(values.map(Option::flatten))
    }
}

/// Marker for an arrow `List` column with items of logical type `L`.
///
/// Item nullability: `List<Option<L>>`.
/// This type is never instantiated — it only appears as a type parameter.
pub struct List<L> {
    _marker: PhantomData<fn() -> L>,
}

/// The validated representation of a `List` column: the list array plus its downcast values.
pub struct TypedList<L: Datatype> {
    list: arrow::array::ListArray,
    values: L::Typed,
}

impl<L: Datatype> Clone for TypedList<L> {
    fn clone(&self) -> Self {
        Self {
            list: self.list.clone(),
            values: self.values.clone(),
        }
    }
}

impl<L: Datatype> Datatype for List<L> {
    type Typed = TypedList<L>;
    type Value<'a>
        = ListValue<'a, L>
    where
        Self: 'a;
    type Owned = Vec<L::Owned>;

    fn datatype() -> DataType {
        DataType::List(std::sync::Arc::new(arrow::datatypes::Field::new(
            "item",
            L::datatype(),
            L::NULLABLE,
        )))
    }

    fn downcast(array: &dyn Array) -> Result<Self::Typed, ColumnError> {
        let list = downcast_array::<arrow::array::ListArray>(array)?;
        let values = list.values();
        if !L::NULLABLE && 0 < values.null_count() {
            return Err(ColumnError::UnexpectedNulls {
                null_count: values.null_count(),
            });
        }
        let values = L::downcast(&**values)?;
        Ok(TypedList { list, values })
    }

    fn is_null(typed: &Self::Typed, index: usize) -> bool {
        typed.list.is_null(index)
    }

    fn value(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
        let offsets = typed.list.value_offsets();
        ListValue {
            values: &typed.values,
            index: offsets[index].as_usize(),
            end: offsets[index + 1].as_usize(),
        }
    }

    fn build(values: impl Iterator<Item = Option<Self::Owned>>) -> ArrayRef {
        let mut lengths = Vec::new();
        let mut validity = Vec::new();
        let mut flattened = Vec::new();
        for list in values {
            if let Some(items) = list {
                lengths.push(items.len());
                validity.push(true);
                flattened.extend(items);
            } else {
                lengths.push(0);
                validity.push(false);
            }
        }

        let field = std::sync::Arc::new(arrow::datatypes::Field::new(
            "item",
            L::datatype(),
            L::NULLABLE,
        ));
        let offsets = arrow::buffer::OffsetBuffer::from_lengths(lengths);
        let values_array = L::build(flattened.into_iter().map(Some));
        let nulls = validity
            .contains(&false)
            .then(|| arrow::buffer::NullBuffer::from(validity));

        std::sync::Arc::new(arrow::array::ListArray::new(
            field,
            offsets,
            values_array,
            nulls,
        ))
    }
}

/// One list element of a `Column<List<L>>`: an iterator over the typed items.
pub struct ListValue<'a, L: Datatype> {
    values: &'a L::Typed,
    index: usize,
    end: usize,
}

impl<'a, L: Datatype + 'a> Iterator for ListValue<'a, L> {
    type Item = L::Value<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.end {
            let value = L::value(self.values, self.index);
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
}

impl<'a, L: Datatype + 'a> ExactSizeIterator for ListValue<'a, L> {}

/// Downcasts and clones (cheaply) a typed arrow array.
///
/// The datatype has already been validated, so a failure here is a bug —
/// but we return an error instead of panicking, to be safe.
fn downcast_array<A: Array + Clone + 'static>(array: &dyn Array) -> Result<A, ColumnError> {
    array
        .as_any()
        .downcast_ref::<A>()
        .cloned()
        .ok_or_else(|| ColumnError::WrongDatatype {
            expected: DataType::Null, // unreachable; see docstring
            actual: array.data_type().clone(),
        })
}

macro_rules! impl_flat_datatype {
    ($rust:ty, $array:ty, $value:ty, $datatype:expr) => {
        impl Datatype for $rust {
            type Typed = $array;
            type Value<'a> = $value;
            type Owned = $rust;

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

            fn build(values: impl Iterator<Item = Option<Self::Owned>>) -> ArrayRef {
                std::sync::Arc::new(<$array>::from_iter(values))
            }
        }
    };
}

impl_flat_datatype!(bool, arrow::array::BooleanArray, bool, DataType::Boolean);
impl_flat_datatype!(i8, arrow::array::Int8Array, i8, DataType::Int8);
impl_flat_datatype!(i16, arrow::array::Int16Array, i16, DataType::Int16);
impl_flat_datatype!(i32, arrow::array::Int32Array, i32, DataType::Int32);
impl_flat_datatype!(i64, arrow::array::Int64Array, i64, DataType::Int64);
impl_flat_datatype!(u8, arrow::array::UInt8Array, u8, DataType::UInt8);
impl_flat_datatype!(u16, arrow::array::UInt16Array, u16, DataType::UInt16);
impl_flat_datatype!(u32, arrow::array::UInt32Array, u32, DataType::UInt32);
impl_flat_datatype!(u64, arrow::array::UInt64Array, u64, DataType::UInt64);
impl_flat_datatype!(f32, arrow::array::Float32Array, f32, DataType::Float32);
impl_flat_datatype!(f64, arrow::array::Float64Array, f64, DataType::Float64);
impl_flat_datatype!(String, arrow::array::StringArray, &'a str, DataType::Utf8);

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

    fn build(values: impl Iterator<Item = Option<Self::Owned>>) -> ArrayRef {
        const {
            assert!(N <= i32::MAX as usize, "FixedSizeBinary size too large");
        }
        #[expect(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let array =
            arrow::array::FixedSizeBinaryArray::try_from_sparse_iter_with_size(values, N as i32)
                .expect("All values have the same size");
        std::sync::Arc::new(array)
    }
}

// ----------------------------------------------------------------------------
// Timestamps

/// Marker for an arrow `Timestamp` column, e.g. `Timestamp<Nanosecond, Utc>`.
///
/// The values are raw `i64` ticks in the given [`TimeUnitSpec`],
/// counted from the unix epoch.
///
/// The timezone defaults to [`NoTimezone`]. Note that timezones are matched
/// *exactly*: a column declared `Timestamp<Nanosecond, Utc>` ("UTC") will not
/// accept an array with the timezone "+00:00".
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct Timestamp<U, Z = NoTimezone> {
    _marker: PhantomData<fn() -> (U, Z)>,
}

/// A [`Timestamp`]/[`Duration`] time unit:
/// [`Second`], [`Millisecond`], [`Microsecond`], or [`Nanosecond`].
pub trait TimeUnitSpec {
    /// The corresponding arrow timestamp type, e.g. `TimestampNanosecondType`.
    type TimestampType: arrow::datatypes::ArrowTimestampType;

    /// The corresponding arrow duration type, e.g. `DurationNanosecondType`.
    type DurationType: arrow::datatypes::ArrowPrimitiveType<Native = i64>;
}

pub struct Second;
pub struct Millisecond;
pub struct Microsecond;
pub struct Nanosecond;

impl TimeUnitSpec for Second {
    type TimestampType = arrow::datatypes::TimestampSecondType;
    type DurationType = arrow::datatypes::DurationSecondType;
}
impl TimeUnitSpec for Millisecond {
    type TimestampType = arrow::datatypes::TimestampMillisecondType;
    type DurationType = arrow::datatypes::DurationMillisecondType;
}
impl TimeUnitSpec for Microsecond {
    type TimestampType = arrow::datatypes::TimestampMicrosecondType;
    type DurationType = arrow::datatypes::DurationMicrosecondType;
}
impl TimeUnitSpec for Nanosecond {
    type TimestampType = arrow::datatypes::TimestampNanosecondType;
    type DurationType = arrow::datatypes::DurationNanosecondType;
}

/// The timezone of a [`Timestamp`]: [`NoTimezone`], [`Utc`], or your own marker type.
pub trait TimezoneSpec {
    /// E.g. `Some("UTC")`, `Some("+02:00")`, or `None` for timezone-naive timestamps.
    fn timezone() -> Option<std::sync::Arc<str>>;
}

/// Timezone-naive timestamps.
pub struct NoTimezone;

impl TimezoneSpec for NoTimezone {
    fn timezone() -> Option<std::sync::Arc<str>> {
        None
    }
}

/// The "UTC" timezone.
pub struct Utc;

impl TimezoneSpec for Utc {
    fn timezone() -> Option<std::sync::Arc<str>> {
        Some("UTC".into())
    }
}

impl<U: TimeUnitSpec + 'static, Z: TimezoneSpec + 'static> Datatype for Timestamp<U, Z> {
    type Typed = arrow::array::PrimitiveArray<U::TimestampType>;
    type Value<'a>
        = i64
    where
        Self: 'a;
    type Owned = i64;

    fn datatype() -> DataType {
        DataType::Timestamp(
            <U::TimestampType as arrow::datatypes::ArrowTimestampType>::UNIT,
            Z::timezone(),
        )
    }

    fn downcast(array: &dyn Array) -> Result<Self::Typed, ColumnError> {
        downcast_array::<Self::Typed>(array)
    }

    fn is_null(typed: &Self::Typed, index: usize) -> bool {
        typed.is_null(index)
    }

    fn value(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
        typed.value(index)
    }

    fn build(values: impl Iterator<Item = Option<Self::Owned>>) -> ArrayRef {
        let array: arrow::array::PrimitiveArray<U::TimestampType> = values.collect();
        std::sync::Arc::new(array.with_timezone_opt(Z::timezone()))
    }
}

/// Marker for an arrow `Duration` column, e.g. `Duration<Nanosecond>`.
///
/// The values are raw `i64` ticks in the given [`TimeUnitSpec`].
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct Duration<U> {
    _marker: PhantomData<fn() -> U>,
}

impl<U: TimeUnitSpec + 'static> Datatype for Duration<U> {
    type Typed = arrow::array::PrimitiveArray<U::DurationType>;
    type Value<'a>
        = i64
    where
        Self: 'a;
    type Owned = i64;

    fn datatype() -> DataType {
        DataType::Duration(<U::TimestampType as arrow::datatypes::ArrowTimestampType>::UNIT)
    }

    fn downcast(array: &dyn Array) -> Result<Self::Typed, ColumnError> {
        downcast_array::<Self::Typed>(array)
    }

    fn is_null(typed: &Self::Typed, index: usize) -> bool {
        typed.is_null(index)
    }

    fn value(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
        typed.value(index)
    }

    fn build(values: impl Iterator<Item = Option<Self::Owned>>) -> ArrayRef {
        let array: arrow::array::PrimitiveArray<U::DurationType> = values.collect();
        std::sync::Arc::new(array)
    }
}
