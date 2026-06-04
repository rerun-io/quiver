//! `List<L>`: a logical type for columns where each element is itself a list.
//!
//! In a `Column<List<String>>`, every element (row) holds a variable number of
//! strings — like a `Vec<Vec<String>>`, but stored contiguously as one flat
//! values array plus offsets: an [`arrow::array::ListArray`]
//! ([`DataType::List`](arrow::datatypes::DataType::List)).
//! Reading is zero-copy: each element is an iterator ([`ListValue`]) over the items.
//!
//! Lists nest: `List<List<i64>>` is a column of lists of lists of integers.
//! Item nullability is `List<Option<L>>`; see [`crate::Column`] for the
//! column/value nullability axes.

use std::marker::PhantomData;

use arrow::array::{Array, ArrayRef};
use arrow::datatypes::ArrowNativeType as _;
use arrow::datatypes::DataType;

use crate::datatype::{ColumnError, Datatype, downcast_array};

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

impl<L: Datatype + 'static> Datatype for List<L> {
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

    fn to_owned_value(value: Self::Value<'_>) -> Self::Owned {
        value.map(L::to_owned_value).collect()
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
