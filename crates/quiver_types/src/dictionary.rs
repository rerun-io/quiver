//! [`Dictionary`]: a logical type for dictionary-encoded (interned) columns.
//!
//! Dictionary encoding stores each *distinct* value once, in a value table,
//! and represents the column as integer keys pointing into that table —
//! a big space win for columns with many repeated values (e.g. enums, tags).
//! Stored as an [`arrow::array::DictionaryArray`]
//! ([`DataType::Dictionary`]).

use std::marker::PhantomData;

use arrow::array::{Array, ArrayRef};
use arrow::datatypes::ArrowNativeType as _;
use arrow::datatypes::DataType;

use crate::datatype::{ColumnError, LogicalType, RefType, downcast_array};

/// Marker for an arrow `Dictionary` column, e.g. `Dictionary<i32, Utf8>`.
///
/// Think of `Dictionary<K, V>` as *a column of `V`, dictionary-compressed*:
/// the encoding is a storage detail, and the element values are those of `V`
/// (e.g. `&str`), looked up through the dictionary keys.
/// `K` is the integer key type (`i8`–`i64`, `u8`–`u64`) — a space/size trade-off,
/// never user-visible.
///
/// # Nullability
/// Since `Dictionary<K, V>` is logically a column of `V`, *row* nullability works
/// like for any other column: `Option<Dictionary<K, V>>` means the rows may be null
/// (arrow encodes this in the validity bitmap of the keys).
/// `Dictionary<Option<K>, V>` would be meaningless: the keys are storage indices,
/// not values anyone reads — `Option<…>` always marks nullability of *readable* values.
///
/// Additionally, arrow allows null entries in the dictionary's *value table* itself,
/// so a valid key can point at a null. That (rare) case is `Dictionary<K, Option<V>>`.
/// `Column<Dictionary<K, V>>` (no `Option` anywhere) guarantees the absence of both.
///
/// ```
/// use quiver::{Column, Dictionary, Utf8};
///
/// // Building dictionary-encodes the values (can fail on key overflow):
/// let column = Column::<Dictionary<i32, Utf8>>::try_from_values(["a", "b", "a"]).unwrap();
/// assert_eq!(column.value(2), "a"); // transparent: reads like a plain column
/// assert_eq!(column.to_vec(), ["a", "b", "a"]);
/// ```
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct Dictionary<K, V> {
    _marker: PhantomData<fn() -> (K, V)>,
}

/// A logical type usable as a [`Dictionary`] key: `i8`–`i64`, `u8`–`u64`.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be used as a dictionary key type",
    label = "dictionary keys must be one of `i8`–`i64`, `u8`–`u64`",
    note = "for nullable dictionary rows, use `Option<Dictionary<K, V>>` instead of `Dictionary<Option<K>, V>` — the keys are storage indices, not readable values"
)]
pub trait DictionaryKey: crate::ConcreteType {
    /// The corresponding arrow key type, e.g. `Int32Type`.
    type ArrowKeyType: arrow::datatypes::ArrowDictionaryKeyType;
}

macro_rules! impl_dictionary_key {
    ($rust:ty, $arrow:ty) => {
        impl DictionaryKey for $rust {
            type ArrowKeyType = $arrow;
        }
    };
}

impl_dictionary_key!(i8, arrow::datatypes::Int8Type);
impl_dictionary_key!(i16, arrow::datatypes::Int16Type);
impl_dictionary_key!(i32, arrow::datatypes::Int32Type);
impl_dictionary_key!(i64, arrow::datatypes::Int64Type);
impl_dictionary_key!(u8, arrow::datatypes::UInt8Type);
impl_dictionary_key!(u16, arrow::datatypes::UInt16Type);
impl_dictionary_key!(u32, arrow::datatypes::UInt32Type);
impl_dictionary_key!(u64, arrow::datatypes::UInt64Type);

/// The validated representation of a `Dictionary` column:
/// the dictionary array plus its downcast values.
pub struct TypedDictionary<K: DictionaryKey, V: LogicalType> {
    dictionary: arrow::array::DictionaryArray<K::ArrowKeyType>,
    values: V::Typed,
}

impl<K: DictionaryKey, V: LogicalType> Clone for TypedDictionary<K, V> {
    fn clone(&self) -> Self {
        Self {
            dictionary: self.dictionary.clone(),
            values: self.values.clone(),
        }
    }
}

impl<K: DictionaryKey + 'static, V: LogicalType + 'static> LogicalType for Dictionary<K, V> {
    type Typed = TypedDictionary<K, V>;
    type Value<'a> = V::Value<'a>;
    type Owned = V::Owned;

    fn matches(actual: &DataType) -> bool {
        match actual {
            DataType::Dictionary(key, value) => K::matches(key) && V::matches(value),
            _ => false,
        }
    }

    fn expected_datatype() -> String {
        format!(
            "Dictionary({}, {})",
            K::expected_datatype(),
            V::expected_datatype()
        )
    }

    fn downcast(array: &dyn Array) -> Result<Self::Typed, ColumnError> {
        let dictionary = downcast_array::<arrow::array::DictionaryArray<K::ArrowKeyType>>(array)?;
        if !V::NULLABLE && 0 < dictionary.values().null_count() {
            // Only count *logical* nulls: null entries in the value table that
            // some key actually references. Unreferenced null entries are fine.
            //
            // `logical_nulls` combines null keys and referenced null entries;
            // subtracting the null keys leaves the referenced null entries.
            let logical = dictionary
                .logical_nulls()
                .map_or(0, |nulls| nulls.null_count());
            let null_keys = dictionary.keys().null_count();
            let referenced_null_entries = logical.saturating_sub(null_keys);
            if 0 < referenced_null_entries {
                return Err(ColumnError::UnexpectedNulls {
                    null_count: referenced_null_entries,
                });
            }
        }
        let values = V::downcast(&**dictionary.values())?;
        Ok(TypedDictionary { dictionary, values })
    }

    fn is_null(typed: &Self::Typed, index: usize) -> bool {
        typed.dictionary.is_null(index)
    }

    fn value(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
        let key = typed.dictionary.keys().value(index).as_usize();
        V::value(&typed.values, key)
    }

    fn to_owned_value(value: Self::Value<'_>) -> Self::Owned {
        V::to_owned_value(value)
    }
}

impl<K: DictionaryKey + 'static, V: crate::ConcreteType + 'static> crate::ConcreteType
    for Dictionary<K, V>
{
    fn datatype() -> DataType {
        DataType::Dictionary(Box::new(K::datatype()), Box::new(V::datatype()))
    }

    fn build(values: impl Iterator<Item = Option<Self::Owned>>) -> Result<ArrayRef, ColumnError> {
        let plain = V::build(values)?;
        // This can fail on key overflow: too many distinct values for `K`
        // (e.g. more than 127 for `i8`). Hence `Dictionary` is NOT `InfallibleBuild`.
        arrow::compute::cast(&plain, &Self::datatype()).map_err(ColumnError::Build)
    }
}

/// References are looked up through the dictionary keys, like [`LogicalType::value`].
impl<K: DictionaryKey + 'static, V: RefType + 'static> RefType for Dictionary<K, V> {
    type Ref = V::Ref;

    fn value_ref(typed: &Self::Typed, index: usize) -> &Self::Ref {
        let key = typed.dictionary.keys().value(index).as_usize();
        V::value_ref(&typed.values, key)
    }
}

/// `vec.try_into()` support for dictionary columns,
/// whose building is fallible (key overflow) — see [`crate::Column::try_from_values`].
impl<K, V, T> TryFrom<Vec<T>> for crate::Column<Dictionary<K, V>>
where
    K: DictionaryKey + 'static,
    V: crate::ConcreteType + 'static,
    T: Into<V::Owned>,
{
    type Error = ColumnError;

    fn try_from(values: Vec<T>) -> Result<Self, Self::Error> {
        Self::try_from_values(values)
    }
}
