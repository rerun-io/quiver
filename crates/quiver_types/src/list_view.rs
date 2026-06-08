//! [`ListView<L>`] and [`LargeListView<L>`]: like [`List`](crate::List), in the
//! "list-view" layout.
//!
//! A list-view column ([`arrow::array::ListViewArray`],
//! [`DataType::ListView`]) stores, per row, an *offset* and a *size* into one
//! flat values array — unlike [`List`](crate::List)'s single offsets buffer.
//! That lets element ranges overlap or appear out of order in the values array
//! (quiver never produces such layouts when *building*, but accepts them when
//! parsing). Reading is zero-copy: each element is an iterator
//! ([`ListValue`]) over the items.
//!
//! [`LargeListView`] is the same with 64-bit offsets and sizes.

use std::marker::PhantomData;

use arrow::array::{Array, ArrayRef, OffsetSizeTrait};
use arrow::datatypes::ArrowNativeType as _;
use arrow::datatypes::DataType;

use crate::datatype::{ColumnError, Datatype, InfallibleBuild, downcast_array};
use crate::list::ListValue;

/// Marker for an arrow `ListView` column with items of logical type `L`:
/// like [`List`](crate::List), in the list-view layout (per-row offset + size).
///
/// Item nullability: `ListView<Option<L>>`.
///
/// ```
/// use quiver::{Column, ListView};
///
/// let column = Column::<ListView<i64>>::from_values([vec![1, 2], vec![3]]);
/// let first: Vec<i64> = column.value(0).collect();
/// assert_eq!(first, [1, 2]);
/// ```
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct ListView<L> {
    _marker: PhantomData<fn() -> L>,
}

/// The validated representation of a `ListView` column:
/// the list-view array plus its downcast values.
pub struct TypedListView<L: Datatype> {
    list: arrow::array::ListViewArray,
    values: L::Typed,
}

/// Marker for an arrow `LargeListView` column with items of logical type `L`:
/// like [`ListView`], with 64-bit offsets and sizes.
///
/// Item nullability: `LargeListView<Option<L>>`.
///
/// ```
/// use quiver::{Column, LargeListView};
///
/// let column = Column::<LargeListView<i64>>::from_values([vec![1, 2], vec![3]]);
/// let second: Vec<i64> = column.value(1).collect();
/// assert_eq!(second, [3]);
/// ```
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct LargeListView<L> {
    _marker: PhantomData<fn() -> L>,
}

/// The validated representation of a `LargeListView` column:
/// the list-view array plus its downcast values.
pub struct TypedLargeListView<L: Datatype> {
    list: arrow::array::LargeListViewArray,
    values: L::Typed,
}

/// Generates the [`Datatype`] (and friends) impl for a list-view logical type:
/// shared by [`ListView`] (32-bit) and [`LargeListView`] (64-bit).
macro_rules! impl_list_view_datatype {
    ($marker:ident, $typed:ident, $array:ty, $variant:ident, $offset:ty) => {
        impl<L: Datatype> Clone for $typed<L> {
            fn clone(&self) -> Self {
                Self {
                    list: self.list.clone(),
                    values: self.values.clone(),
                }
            }
        }

        impl<L: Datatype + 'static> Datatype for $marker<L> {
            type Typed = $typed<L>;
            type Value<'a>
                = ListValue<'a, L>
            where
                Self: 'a;
            type Owned = Vec<L::Owned>;

            fn datatype() -> DataType {
                DataType::$variant(std::sync::Arc::new(arrow::datatypes::Field::new(
                    "item",
                    L::datatype(),
                    L::NULLABLE,
                )))
            }

            fn matches(actual: &DataType) -> bool {
                match actual {
                    DataType::$variant(item) => L::matches(item.data_type()),
                    _ => false,
                }
            }

            fn downcast(array: &dyn Array) -> Result<Self::Typed, ColumnError> {
                let list = downcast_array::<$array>(array)?;
                if !L::NULLABLE {
                    // Only count *logical* nulls: items reachable through some
                    // valid row. List-view ranges can overlap or be unordered,
                    // so this is summed per row, not over a single window.
                    let null_count = logical_view_item_null_count(&list);
                    if 0 < null_count {
                        return Err(ColumnError::UnexpectedNulls { null_count });
                    }
                }
                let values = L::downcast(&**list.values())?;
                Ok($typed { list, values })
            }

            fn is_null(typed: &Self::Typed, index: usize) -> bool {
                typed.list.is_null(index)
            }

            fn value(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
                let start = typed.list.value_offset(index).as_usize();
                let size = typed.list.value_size(index).as_usize();
                ListValue::new(&typed.values, start, start + size)
            }

            fn build(
                values: impl Iterator<Item = Option<Self::Owned>>,
            ) -> Result<ArrayRef, ColumnError> {
                let mut offsets: Vec<$offset> = Vec::new();
                let mut sizes: Vec<$offset> = Vec::new();
                let mut validity = Vec::new();
                let mut flattened = Vec::new();
                // Build the simplest valid layout: contiguous, in order.
                let mut running: usize = 0;
                for list in values {
                    offsets.push(
                        <$offset>::from_usize(running)
                            .expect("List-view offset overflows the offset type"),
                    );
                    if let Some(items) = list {
                        let len = items.len();
                        sizes.push(
                            <$offset>::from_usize(len)
                                .expect("List-view size overflows the offset type"),
                        );
                        validity.push(true);
                        flattened.extend(items);
                        running += len;
                    } else {
                        sizes.push(<$offset>::from_usize(0).expect("0 always fits"));
                        validity.push(false);
                    }
                }

                let field = std::sync::Arc::new(arrow::datatypes::Field::new(
                    "item",
                    L::datatype(),
                    L::NULLABLE,
                ));
                let values_array = L::build(flattened.into_iter().map(Some))?;
                let nulls = validity
                    .contains(&false)
                    .then(|| arrow::buffer::NullBuffer::from(validity));

                let list = <$array>::try_new(
                    field,
                    arrow::buffer::ScalarBuffer::from(offsets),
                    arrow::buffer::ScalarBuffer::from(sizes),
                    values_array,
                    nulls,
                )
                .map_err(ColumnError::Build)?;
                Ok(std::sync::Arc::new(list))
            }

            fn to_owned_value(value: Self::Value<'_>) -> Self::Owned {
                value.map(L::to_owned_value).collect()
            }
        }

        impl<L: InfallibleBuild + 'static> InfallibleBuild for $marker<L> {}
    };
}

impl_list_view_datatype!(
    ListView,
    TypedListView,
    arrow::array::ListViewArray,
    ListView,
    i32
);
impl_list_view_datatype!(
    LargeListView,
    TypedLargeListView,
    arrow::array::LargeListViewArray,
    LargeListView,
    i64
);

/// Counts the nulls among the *reachable* items of a list-view array (`ListView`
/// or `LargeListView` — it is generic over the offset width):
/// items inside the ranges of valid (non-null) rows.
///
/// Unlike a plain list, list-view ranges can overlap or be unordered, so there
/// is no single contiguous window: each valid row's `offset..offset + size` is
/// counted separately.
fn logical_view_item_null_count<O: OffsetSizeTrait>(
    list: &arrow::array::GenericListViewArray<O>,
) -> usize {
    let Some(item_nulls) = list.values().nulls() else {
        return 0;
    };
    if item_nulls.null_count() == 0 {
        return 0; // Fast path: no nulls anywhere in the values array.
    }

    let offsets = list.value_offsets();
    let sizes = list.value_sizes();
    let count_row = |row: usize| {
        let start = offsets[row].as_usize();
        let size = sizes[row].as_usize();
        if size == 0 {
            0
        } else {
            item_nulls.slice(start, size).null_count()
        }
    };

    match list.nulls() {
        // All rows valid: every row's range is reachable.
        None => (0..list.len()).map(count_row).sum(),

        // Only count items of valid rows:
        Some(row_validity) => (0..list.len())
            .filter(|&row| row_validity.is_valid(row))
            .map(count_row)
            .sum(),
    }
}
