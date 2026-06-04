//! [`Dictionary`]: a logical type for dictionary-encoded (interned) columns.
//!
//! Dictionary encoding stores each *distinct* value once, in a value table,
//! and represents the column as integer keys pointing into that table —
//! a big space win for columns with many repeated values (e.g. enums, tags).
//! Stored as an [`arrow::array::DictionaryArray`]
//! ([`DataType::Dictionary`](arrow::datatypes::DataType::Dictionary)).

use std::marker::PhantomData;

use arrow::array::{Array, ArrayRef};
use arrow::datatypes::ArrowNativeType as _;
use arrow::datatypes::DataType;

use crate::datatype::{ColumnError, Datatype, downcast_array};

/// Marker for an arrow `Dictionary` column, e.g. `Dictionary<i32, String>`.
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
pub trait DictionaryKey: Datatype {
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
pub struct TypedDictionary<K: DictionaryKey, V: Datatype> {
    dictionary: arrow::array::DictionaryArray<K::ArrowKeyType>,
    values: V::Typed,
}

impl<K: DictionaryKey, V: Datatype> Clone for TypedDictionary<K, V> {
    fn clone(&self) -> Self {
        Self {
            dictionary: self.dictionary.clone(),
            values: self.values.clone(),
        }
    }
}

impl<K: DictionaryKey + 'static, V: Datatype + 'static> Datatype for Dictionary<K, V> {
    type Typed = TypedDictionary<K, V>;
    type Value<'a> = V::Value<'a>;
    type Owned = V::Owned;

    fn datatype() -> DataType {
        DataType::Dictionary(Box::new(K::datatype()), Box::new(V::datatype()))
    }

    fn downcast(array: &dyn Array) -> Result<Self::Typed, ColumnError> {
        let dictionary = downcast_array::<arrow::array::DictionaryArray<K::ArrowKeyType>>(array)?;
        let values = dictionary.values();
        if !V::NULLABLE && 0 < values.null_count() {
            return Err(ColumnError::UnexpectedNulls {
                null_count: values.null_count(),
            });
        }
        let values = V::downcast(&**values)?;
        Ok(TypedDictionary { dictionary, values })
    }

    fn is_null(typed: &Self::Typed, index: usize) -> bool {
        typed.dictionary.is_null(index)
    }

    fn value(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
        let key = typed.dictionary.keys().value(index).as_usize();
        V::value(&typed.values, key)
    }

    fn build(values: impl Iterator<Item = Option<Self::Owned>>) -> ArrayRef {
        let plain = V::build(values);
        arrow::compute::cast(&plain, &Self::datatype())
            .expect("Dictionary-encoding failed (too many unique values for the key type?)")
    }

    fn to_owned_value(value: Self::Value<'_>) -> Self::Owned {
        V::to_owned_value(value)
    }
}
