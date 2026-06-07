//! [`Map<K, V>`]: a logical type for columns of key-value maps.
//!
//! Each element (row) is a map from keys of logical type `K` to values of
//! logical type `V` — like a `Vec<(K, V)>`, stored as an
//! [`arrow::array::MapArray`] ([`DataType::Map`]): a list of `{key, value}`
//! struct entries, one flat keys array and one flat values array plus offsets.
//! Reading is zero-copy: each element is an iterator ([`MapValue`]) over the
//! `(key, value)` pairs.
//!
//! Arrow maps never have null keys, so `K` is a plain (non-`Option`) logical type.
//! Value nullability is `Map<K, Option<V>>`; whole-row nullability is
//! `Option<Map<K, V>>` (a null row is a missing map). Arrow does not guarantee
//! unique or sorted keys, and neither does quiver — duplicates are preserved.

use std::marker::PhantomData;

use arrow::array::{Array, ArrayRef, MapArray, StructArray};
use arrow::datatypes::ArrowNativeType as _;
use arrow::datatypes::{DataType, Field, Fields};

use crate::datatype::{ColumnError, Datatype, InfallibleBuild, downcast_array};

/// Marker for an arrow `Map` column from keys `K` to values `V`,
/// e.g. `Map<Utf8, i64>`.
///
/// Value nullability: `Map<K, Option<V>>`. Whole-row nullability:
/// `Option<Map<K, V>>`. Arrow map keys are never null, so `K` is non-`Option`.
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct Map<K, V> {
    _marker: PhantomData<fn() -> (K, V)>,
}

/// The validated representation of a `Map` column:
/// the map array plus its downcast keys and values.
pub struct TypedMap<K: Datatype, V: Datatype> {
    map: MapArray,
    keys: K::Typed,
    values: V::Typed,
}

impl<K: Datatype, V: Datatype> Clone for TypedMap<K, V> {
    fn clone(&self) -> Self {
        Self {
            map: self.map.clone(),
            keys: self.keys.clone(),
            values: self.values.clone(),
        }
    }
}

/// The arrow `Field`s of a map's `{key, value}` struct entries.
fn entry_fields<K: Datatype, V: Datatype>() -> Fields {
    Fields::from(vec![
        Field::new("keys", K::datatype(), false),
        Field::new("values", V::datatype(), V::NULLABLE),
    ])
}

impl<K: Datatype + 'static, V: Datatype + 'static> Datatype for Map<K, V> {
    type Typed = TypedMap<K, V>;
    type Value<'a>
        = MapValue<'a, K, V>
    where
        Self: 'a;
    type Owned = Vec<(K::Owned, V::Owned)>;

    fn datatype() -> DataType {
        DataType::Map(
            std::sync::Arc::new(Field::new(
                "entries",
                DataType::Struct(entry_fields::<K, V>()),
                false,
            )),
            false,
        )
    }

    fn matches(actual: &DataType) -> bool {
        let DataType::Map(entries, _ordered) = actual else {
            return false;
        };
        // The entries field name, the `{key, value}` field names, the
        // nullability flags, and the `ordered` flag are all ignored: only the
        // key and value datatypes are compared (structurally, recursively).
        match entries.data_type() {
            DataType::Struct(fields) if fields.len() == 2 => {
                K::matches(fields[0].data_type()) && V::matches(fields[1].data_type())
            }
            _ => false,
        }
    }

    fn downcast(array: &dyn Array) -> Result<Self::Typed, ColumnError> {
        let map = downcast_array::<MapArray>(array)?;

        // Keys are never null in a valid arrow map, but a sliced or null-row map
        // may carry physical nulls; only *reachable* nulls would be a real error.
        if !K::NULLABLE {
            let null_count = logical_child_null_count(&map, map.keys());
            if 0 < null_count {
                return Err(ColumnError::UnexpectedNulls { null_count });
            }
        }
        if !V::NULLABLE {
            let null_count = logical_child_null_count(&map, map.values());
            if 0 < null_count {
                return Err(ColumnError::UnexpectedNulls { null_count });
            }
        }

        let keys = K::downcast(&**map.keys())?;
        let values = V::downcast(&**map.values())?;
        Ok(TypedMap { map, keys, values })
    }

    fn is_null(typed: &Self::Typed, index: usize) -> bool {
        typed.map.is_null(index)
    }

    fn value(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
        let offsets = typed.map.value_offsets();
        MapValue {
            keys: &typed.keys,
            values: &typed.values,
            index: offsets[index].as_usize(),
            end: offsets[index + 1].as_usize(),
        }
    }

    fn build(values: impl Iterator<Item = Option<Self::Owned>>) -> Result<ArrayRef, ColumnError> {
        let mut lengths = Vec::new();
        let mut validity = Vec::new();
        let mut flat_keys = Vec::new();
        let mut flat_values = Vec::new();
        for entries in values {
            if let Some(pairs) = entries {
                lengths.push(pairs.len());
                validity.push(true);
                for (key, value) in pairs {
                    flat_keys.push(key);
                    flat_values.push(value);
                }
            } else {
                lengths.push(0);
                validity.push(false);
            }
        }

        let fields = entry_fields::<K, V>();
        let keys_array = K::build(flat_keys.into_iter().map(Some))?;
        let values_array = V::build(flat_values.into_iter().map(Some))?;
        let entries = StructArray::try_new(fields.clone(), vec![keys_array, values_array], None)
            .map_err(ColumnError::Build)?;

        let offsets = arrow::buffer::OffsetBuffer::<i32>::from_lengths(lengths);
        let nulls = validity
            .contains(&false)
            .then(|| arrow::buffer::NullBuffer::from(validity));
        let entries_field =
            std::sync::Arc::new(Field::new("entries", DataType::Struct(fields), false));

        let map = MapArray::try_new(entries_field, offsets, entries, nulls, false)
            .map_err(ColumnError::Build)?;
        Ok(std::sync::Arc::new(map))
    }

    fn to_owned_value(value: Self::Value<'_>) -> Self::Owned {
        value
            .map(|(key, val)| (K::to_owned_value(key), V::to_owned_value(val)))
            .collect()
    }
}

impl<K: InfallibleBuild + 'static, V: InfallibleBuild + 'static> InfallibleBuild for Map<K, V> {}

/// One map element of a `Column<Map<K, V>>`:
/// an iterator over the typed `(key, value)` pairs.
pub struct MapValue<'a, K: Datatype, V: Datatype> {
    keys: &'a K::Typed,
    values: &'a V::Typed,
    index: usize,
    end: usize,
}

impl<'a, K: Datatype + 'a, V: Datatype + 'a> Iterator for MapValue<'a, K, V> {
    type Item = (K::Value<'a>, V::Value<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.end {
            let pair = (
                K::value(self.keys, self.index),
                V::value(self.values, self.index),
            );
            self.index += 1;
            Some(pair)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.end - self.index;
        (remaining, Some(remaining))
    }
}

impl<'a, K: Datatype + 'a, V: Datatype + 'a> ExactSizeIterator for MapValue<'a, K, V> {}

/// Counts the nulls among the *reachable* entries of a map's `child` array
/// (its keys or values): entries inside the ranges of valid (non-null) rows.
///
/// The logical count, like [`crate::list`]'s: physical nulls outside the slice
/// window, or inside the ranges of null rows, don't count.
fn logical_child_null_count(map: &MapArray, child: &dyn Array) -> usize {
    let Some(child_nulls) = child.nulls() else {
        return 0;
    };

    let offsets = map.value_offsets();
    let window_start = offsets[0].as_usize();
    let window_end = offsets[map.len()].as_usize();
    if child_nulls
        .slice(window_start, window_end - window_start)
        .null_count()
        == 0
    {
        return 0; // Fast path: no nulls anywhere in the referenced window.
    }

    match map.nulls() {
        // All rows valid: every entry in the window is reachable.
        None => child_nulls
            .slice(window_start, window_end - window_start)
            .null_count(),

        // Only count entries of valid rows:
        Some(row_validity) => (0..map.len())
            .filter(|&row| row_validity.is_valid(row))
            .map(|row| {
                let start = offsets[row].as_usize();
                let end = offsets[row + 1].as_usize();
                if start == end {
                    0
                } else {
                    child_nulls.slice(start, end - start).null_count()
                }
            })
            .sum(),
    }
}
