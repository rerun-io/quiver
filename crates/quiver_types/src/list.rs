//! `List<L>` and [`LargeList`](crate::LargeList): logical types for columns where
//! each element is itself a list.
//!
//! In a `Column<List<Utf8>>`, every element (row) holds a variable number of
//! strings — like a `Vec<Vec<String>>`, but stored contiguously as one flat
//! values array plus offsets: an [`arrow::array::ListArray`]
//! ([`DataType::List`]).
//! Reading is zero-copy: each element is an iterator ([`ListValue`]) over the items.
//!
//! Lists nest: `List<List<i64>>` is a column of lists of lists of integers.
//! Item nullability is `List<Option<L>>`; see [`crate::Column`] for the
//! column/value nullability axes.
//!
//! [`LargeList`](crate::LargeList) is the same with 64-bit offsets, for columns
//! whose flattened items exceed what an `i32` offset can address.

use std::marker::PhantomData;

use arrow::array::{Array, ArrayRef, OffsetSizeTrait};
use arrow::datatypes::ArrowNativeType as _;
use arrow::datatypes::DataType;

use crate::datatype::{
    ColumnError, InfallibleBuild, LogicalType, PrimitiveType, RefType, downcast_array,
};

/// Marker for an arrow `List` column with items of logical type `L`.
///
/// Item nullability: `List<Option<L>>`.
///
/// ```
/// use quiver::{Column, List};
///
/// let column = Column::<List<i64>>::from_values([vec![1, 2], vec![3]]);
/// let first: Vec<i64> = column.value(0).collect();
/// assert_eq!(first, [1, 2]);
/// ```
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct List<L> {
    _marker: PhantomData<fn() -> L>,
}

/// The validated representation of a `List` column: the list array plus its downcast values.
pub struct TypedList<L: LogicalType> {
    list: arrow::array::ListArray,
    values: L::Typed,
}

/// Generates the [`LogicalType`] (and friends) impl for a list logical type:
/// shared by [`List`] (32-bit offsets) and [`LargeList`](crate::LargeList)
/// (64-bit offsets).
macro_rules! impl_list_datatype {
    ($marker:ident, $typed:ident, $array:ty, $variant:ident) => {
        impl<L: LogicalType> Clone for $typed<L> {
            fn clone(&self) -> Self {
                Self {
                    list: self.list.clone(),
                    values: self.values.clone(),
                }
            }
        }

        impl<L: LogicalType + 'static> LogicalType for $marker<L> {
            type Typed = $typed<L>;
            type Value<'a>
                = ListValue<'a, L>
            where
                Self: 'a;
            type Owned = Vec<L::Owned>;

            fn downcast(array: &dyn Array) -> Result<Self::Typed, ColumnError> {
                let list =
                    downcast_array::<$array>(array, || format!("{}(…)", stringify!($variant)))?;
                if !L::NULLABLE {
                    // Only count *logical* nulls: items that can actually be reached
                    // through some valid row. Sliced arrays may have nulls outside the
                    // referenced range, and null rows may cover garbage item ranges.
                    let null_count = logical_item_null_count(&list);
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
                let offsets = typed.list.value_offsets();
                ListValue::new(
                    &typed.values,
                    offsets[index].as_usize(),
                    offsets[index + 1].as_usize(),
                )
            }

            fn to_owned_value(value: Self::Value<'_>) -> Self::Owned {
                value.map(L::to_owned_value).collect()
            }
        }

        impl<L: crate::ConcreteType + 'static> crate::ConcreteType for $marker<L> {
            fn datatype() -> DataType {
                DataType::$variant(std::sync::Arc::new(arrow::datatypes::Field::new(
                    "item",
                    L::datatype(),
                    L::NULLABLE,
                )))
            }

            fn build(
                values: impl Iterator<Item = Option<Self::Owned>>,
            ) -> Result<ArrayRef, ColumnError> {
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
                let values_array = L::build(flattened.into_iter().map(Some))?;
                let nulls = validity
                    .contains(&false)
                    .then(|| arrow::buffer::NullBuffer::from(validity));

                Ok(std::sync::Arc::new(<$array>::new(
                    field,
                    offsets,
                    values_array,
                    nulls,
                )))
            }
        }

        impl<L: InfallibleBuild + 'static> InfallibleBuild for $marker<L> {}
    };
}

pub(crate) use impl_list_datatype;

impl_list_datatype!(List, TypedList, arrow::array::ListArray, List);

/// One list element of a list column (`List`, [`LargeList`](crate::LargeList),
/// [`FixedSizeList`](crate::FixedSizeList), …): a zero-copy, random-access view
/// of that row's typed items.
///
/// It mirrors [`Column`](crate::Column)'s read API — the items behave like a
/// borrowed slice of a single-typed column:
///
/// - length: [`len`](ListValue::len), [`is_empty`](ListValue::is_empty)
/// - by-item access: [`get`](ListValue::get) / [`value`](ListValue::value)
///   (zero-copy views) and [`get_owned`](ListValue::get_owned) /
///   [`value_owned`](ListValue::value_owned) (owned), plus `list[i]` where the
///   item can be borrowed from the array (see the [`Index`](std::ops::Index) impl)
/// - bulk: [`to_vec`](ListValue::to_vec), and [`as_slice`](ListValue::as_slice)
///   for primitive items (one contiguous zero-copy slice)
/// - iteration: [`iter`](ListValue::iter) / [`iter_owned`](ListValue::iter_owned)
///
/// `ListValue` is itself an [`Iterator`] over the items, so `.map(…)` /
/// `.collect()` / `.sum()` work directly on it. It is [`Copy`]: consuming it as
/// an iterator advances a cursor (the random-access methods then see the
/// remaining items), while [`iter`](ListValue::iter) hands out a fresh cursor
/// without consuming the original.
///
/// ```
/// use quiver::{Column, List};
///
/// let column = Column::<List<i64>>::from_values([vec![10, 20, 30], vec![]]);
/// let row = column.value(0);
///
/// assert_eq!(row.len(), 3);
/// assert_eq!(row.value(1), 20);     // by item index
/// assert_eq!(row[2], 30);           // borrowed (primitive items)
/// assert_eq!(row.as_slice(), &[10, 20, 30]); // contiguous, zero-copy
/// assert_eq!(row.to_vec(), vec![10, 20, 30]);
///
/// let sum: i64 = row.iter().sum();  // `iter` does not consume `row`
/// assert_eq!(sum, 60);
///
/// assert!(column.value(1).is_empty());
/// ```
pub struct ListValue<'a, L: LogicalType> {
    values: &'a L::Typed,
    index: usize,
    end: usize,
}

impl<L: LogicalType> Clone for ListValue<'_, L> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<L: LogicalType> Copy for ListValue<'_, L> {}

impl<'a, L: LogicalType + 'a> ListValue<'a, L> {
    /// `index..end` into `values`.
    pub(crate) fn new(values: &'a L::Typed, index: usize, end: usize) -> Self {
        Self { values, index, end }
    }

    /// The number of items in this list element.
    #[must_use]
    pub fn len(&self) -> usize {
        self.end - self.index
    }

    /// Is this list element empty?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.index == self.end
    }

    /// The item at `index`, or `None` if out of bounds.
    ///
    /// See [`ListValue::value`] for the returned view;
    /// [`ListValue::get_owned`] returns the owned value instead.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<L::Value<'a>> {
        let item = self.index.checked_add(index)?;
        (item < self.end).then(|| L::value(self.values, item))
    }

    /// The owned item at `index`, or `None` if out of bounds —
    /// e.g. `String` where [`ListValue::get`] returns `&str`.
    #[must_use]
    pub fn get_owned(&self, index: usize) -> Option<L::Owned> {
        self.get(index).map(L::to_owned_value)
    }

    /// The item at `index`, returning the zero-copy view
    /// ([`LogicalType::Value`]); for the owned value see [`ListValue::value_owned`].
    ///
    /// Where the item can be borrowed from the array, `list[index]` works too
    /// (see the [`Index`](std::ops::Index) impl).
    ///
    /// Panics if out of bounds.
    #[must_use]
    pub fn value(&self, index: usize) -> L::Value<'a> {
        assert!(
            index < self.len(),
            "ListValue index {index} out of bounds for length {}",
            self.len()
        );
        L::value(self.values, self.index + index)
    }

    /// The owned item at `index` — e.g. `String` where [`ListValue::value`]
    /// returns `&str`.
    ///
    /// Panics if out of bounds.
    #[must_use]
    pub fn value_owned(&self, index: usize) -> L::Owned {
        L::to_owned_value(self.value(index))
    }

    /// Iterates over the zero-copy views ([`LogicalType::Value`]) of the
    /// remaining items, without consuming `self`.
    ///
    /// For owned values, see [`ListValue::iter_owned`].
    #[must_use]
    pub fn iter(&self) -> Self {
        *self
    }

    /// Iterates over the owned values of the remaining items —
    /// e.g. `String` where [`ListValue::iter`] yields `&str`.
    pub fn iter_owned(&self) -> impl Iterator<Item = L::Owned> + 'a {
        self.iter().map(L::to_owned_value)
    }

    /// Copies the items into a `Vec` of owned values,
    /// e.g. `Vec<String>` for a `List<Utf8>` element.
    #[must_use]
    pub fn to_vec(self) -> Vec<L::Owned> {
        self.iter_owned().collect()
    }
}

/// `for item in &list` — iterates the items without consuming the view.
impl<'a, L: LogicalType + 'a> IntoIterator for &ListValue<'a, L> {
    type Item = L::Value<'a>;
    type IntoIter = ListValue<'a, L>;

    fn into_iter(self) -> Self::IntoIter {
        *self
    }
}

/// `list[index]`: like [`ListValue::value`], but borrows from the array —
/// `&list[i]` is `&str` for a `List<Utf8>` element, `&i64` for `List<i64>`.
///
/// Available for items that can be borrowed from the array: strings, binaries,
/// and primitives — but not `bool`, `Option<…>`, or nested `List<…>` items.
///
/// Panics if out of bounds (like [`ListValue::value`]).
impl<L: RefType> std::ops::Index<usize> for ListValue<'_, L> {
    type Output = L::Ref;

    fn index(&self, index: usize) -> &Self::Output {
        assert!(
            index < self.len(),
            "ListValue index {index} out of bounds for length {}",
            self.len()
        );
        L::value_ref(self.values, self.index + index)
    }
}

impl<'a, L: PrimitiveType> ListValue<'a, L> {
    /// The items as a contiguous zero-copy slice,
    /// e.g. `&[f32]` for a `List<f32>` element.
    ///
    /// Only available for primitive and fixed-size binary items
    /// (`bool` is excluded: arrow bit-packs it).
    #[must_use]
    pub fn as_slice(&self) -> &'a [L::Native] {
        &L::values(self.values)[self.index..self.end]
    }
}

// Iteration mirrors a slice's: the items live in `self.index..self.end`, all
// in bounds, so the combinators are overridden to skip the per-element `Option`
// plumbing of the default `next`-based implementations. (Primitive items have
// an even faster path: [`ListValue::as_slice`].)
impl<'a, L: LogicalType + 'a> Iterator for ListValue<'a, L> {
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

    fn count(self) -> usize {
        self.end - self.index
    }

    fn last(self) -> Option<Self::Item> {
        (self.index < self.end).then(|| L::value(self.values, self.end - 1))
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        match self.index.checked_add(n) {
            Some(target) if target < self.end => {
                self.index = target + 1;
                Some(L::value(self.values, target))
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
        let Self { values, index, end } = self;
        let mut acc = init;
        for i in index..end {
            acc = f(acc, L::value(values, i));
        }
        acc
    }
}

impl<'a, L: LogicalType + 'a> DoubleEndedIterator for ListValue<'a, L> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.index < self.end {
            self.end -= 1;
            Some(L::value(self.values, self.end))
        } else {
            None
        }
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        match self.end.checked_sub(n + 1) {
            Some(target) if self.index <= target => {
                self.end = target;
                Some(L::value(self.values, target))
            }
            _ => {
                self.end = self.index;
                None
            }
        }
    }

    fn rfold<B, F>(self, init: B, mut f: F) -> B
    where
        F: FnMut(B, Self::Item) -> B,
    {
        let Self { values, index, end } = self;
        let mut acc = init;
        for i in (index..end).rev() {
            acc = f(acc, L::value(values, i));
        }
        acc
    }
}

impl<'a, L: LogicalType + 'a> ExactSizeIterator for ListValue<'a, L> {}

impl<'a, L: LogicalType + 'a> std::iter::FusedIterator for ListValue<'a, L> {}

/// Counts the nulls among the *reachable* items of a list array (`List` or
/// `LargeList` — it is generic over the offset width):
/// items inside the ranges of valid (non-null) rows.
///
/// This is the logical count: physical nulls outside the slice window,
/// or inside the ranges of null rows, don't count.
pub(crate) fn logical_item_null_count<O: OffsetSizeTrait>(
    list: &arrow::array::GenericListArray<O>,
) -> usize {
    let Some(item_nulls) = list.values().nulls() else {
        return 0;
    };

    let offsets = list.value_offsets();
    let window_start = offsets[0].as_usize();
    let window_end = offsets[list.len()].as_usize();
    if item_nulls
        .slice(window_start, window_end - window_start)
        .null_count()
        == 0
    {
        return 0; // Fast path: no nulls anywhere in the referenced window.
    }

    match list.nulls() {
        // All rows valid: every item in the window is reachable.
        None => item_nulls
            .slice(window_start, window_end - window_start)
            .null_count(),

        // Only count items of valid rows:
        Some(row_validity) => (0..list.len())
            .filter(|&row| row_validity.is_valid(row))
            .map(|row| {
                let start = offsets[row].as_usize();
                let end = offsets[row + 1].as_usize();
                if start == end {
                    0
                } else {
                    item_nulls.slice(start, end - start).null_count()
                }
            })
            .sum(),
    }
}
