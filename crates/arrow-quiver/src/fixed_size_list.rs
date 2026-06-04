//! [`FixedSizeList`]: a logical type for columns where each element is a list
//! of exactly `N` items.
//!
//! In a `Column<FixedSizeList<f32, 3>>`, every element (row) holds exactly
//! three floats — e.g. 3D positions, fixed-width embeddings, or tensor rows.
//! Stored as an [`arrow::array::FixedSizeListArray`]
//! ([`DataType::FixedSizeList`]):
//! one flat child array, no offsets needed.
//! Reading is zero-copy: each element is an iterator
//! ([`ListValue`]) over the items.

use std::marker::PhantomData;

use arrow::array::{Array, ArrayRef};
use arrow::datatypes::DataType;

use crate::datatype::{ColumnError, Datatype, InfallibleBuild, downcast_array};
use crate::list::ListValue;

/// Marker for an arrow `FixedSizeList` column: each element holds exactly
/// `N` items of logical type `L`, e.g. `FixedSizeList<f32, 3>` for 3D positions.
///
/// Item nullability: `FixedSizeList<Option<L>, N>`.
/// This type is never instantiated — it only appears as a type parameter.
pub struct FixedSizeList<L, const N: usize> {
    _marker: PhantomData<fn() -> L>,
}

/// The validated representation of a `FixedSizeList` column:
/// the list array plus its downcast values.
pub struct TypedFixedSizeList<L: Datatype> {
    list: arrow::array::FixedSizeListArray,
    values: L::Typed,
}

impl<L: Datatype> Clone for TypedFixedSizeList<L> {
    fn clone(&self) -> Self {
        Self {
            list: self.list.clone(),
            values: self.values.clone(),
        }
    }
}

impl<L: Datatype + 'static, const N: usize> Datatype for FixedSizeList<L, N> {
    type Typed = TypedFixedSizeList<L>;
    type Value<'a>
        = ListValue<'a, L>
    where
        Self: 'a;
    type Owned = [L::Owned; N];

    fn datatype() -> DataType {
        const {
            assert!(N <= i32::MAX as usize, "FixedSizeList size too large");
        }
        #[expect(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        DataType::FixedSizeList(
            std::sync::Arc::new(arrow::datatypes::Field::new(
                "item",
                L::datatype(),
                L::NULLABLE,
            )),
            N as i32,
        )
    }

    fn downcast(array: &dyn Array) -> Result<Self::Typed, ColumnError> {
        let list = downcast_array::<arrow::array::FixedSizeListArray>(array)?;
        if !L::NULLABLE {
            // Only count *logical* nulls: items reachable through valid rows.
            // (Null rows have placeholder item slots; slicing leaves items
            // outside the window.)
            let null_count = logical_item_null_count(&list);
            if 0 < null_count {
                return Err(ColumnError::UnexpectedNulls { null_count });
            }
        }
        let values = L::downcast(&**list.values())?;
        Ok(TypedFixedSizeList { list, values })
    }

    fn is_null(typed: &Self::Typed, index: usize) -> bool {
        typed.list.is_null(index)
    }

    fn value(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
        #[expect(clippy::cast_sign_loss)]
        let start = typed.list.value_offset(index) as usize;
        ListValue::new(&typed.values, start, start + N)
    }

    fn build(values: impl Iterator<Item = Option<Self::Owned>>) -> Result<ArrayRef, ColumnError> {
        let mut validity = Vec::new();
        let mut flattened: Vec<Option<L::Owned>> = Vec::new();
        for row in values {
            if let Some(items) = row {
                validity.push(true);
                flattened.extend(items.map(Some));
            } else {
                // Null rows still need `N` (placeholder) item slots:
                validity.push(false);
                flattened.extend(std::iter::repeat_with(|| None).take(N));
            }
        }

        let field = std::sync::Arc::new(arrow::datatypes::Field::new(
            "item",
            L::datatype(),
            // The placeholder slots of null rows are physically null
            // (but masked by the row validity):
            true,
        ));
        let values_array = L::build(flattened.into_iter())?;
        let nulls = validity
            .contains(&false)
            .then(|| arrow::buffer::NullBuffer::from(validity));

        const {
            assert!(N <= i32::MAX as usize, "FixedSizeList size too large");
        }
        #[expect(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let list = arrow::array::FixedSizeListArray::try_new(field, N as i32, values_array, nulls)
            .map_err(ColumnError::Build)?;
        Ok(std::sync::Arc::new(list))
    }

    fn to_owned_value(value: Self::Value<'_>) -> Self::Owned {
        let mut items = value.map(L::to_owned_value);
        std::array::from_fn(|_| {
            items
                .next()
                .expect("Cannot fail: the element holds exactly N items")
        })
    }
}

impl<L: InfallibleBuild + 'static, const N: usize> InfallibleBuild for FixedSizeList<L, N> {}

/// Counts the nulls among the *reachable* items: items of valid (non-null) rows.
fn logical_item_null_count(list: &arrow::array::FixedSizeListArray) -> usize {
    let Some(item_nulls) = list.values().nulls() else {
        return 0;
    };

    #[expect(clippy::cast_sign_loss)]
    let size = list.value_length() as usize;
    #[expect(clippy::cast_sign_loss)]
    let window_start = list.value_offset(0) as usize;
    let window_len = list.len() * size;
    if list.len() == 0 || item_nulls.slice(window_start, window_len).null_count() == 0 {
        return 0; // Fast path: no nulls anywhere in the referenced window.
    }

    match list.nulls() {
        // All rows valid: every item in the window is reachable.
        None => item_nulls.slice(window_start, window_len).null_count(),

        // Only count the items of valid rows:
        Some(row_validity) => (0..list.len())
            .filter(|&row| row_validity.is_valid(row))
            .map(|row| {
                #[expect(clippy::cast_sign_loss)]
                let start = list.value_offset(row) as usize;
                item_nulls.slice(start, size).null_count()
            })
            .sum(),
    }
}
