//! [`LargeList<L>`]: like [`List`](crate::List), with 64-bit offsets.
//!
//! A `Column<LargeList<Utf8>>` is stored as an [`arrow::array::LargeListArray`]
//! ([`DataType::LargeList`]): identical to a [`List`](crate::List) except that the
//! offsets are `i64` instead of `i32`, so the flattened items may exceed
//! what a 32-bit offset can address (more than ~2 billion items in one column).
//! Reading is zero-copy: each element is an iterator
//! ([`ListValue`](crate::ListValue)) over the items.

use std::marker::PhantomData;

use arrow::array::{Array, ArrayRef};
use arrow::datatypes::ArrowNativeType as _;
use arrow::datatypes::DataType;

use crate::datatype::{ColumnError, Datatype, InfallibleBuild, downcast_array};
use crate::list::{ListValue, impl_list_datatype, logical_item_null_count};

/// Marker for an arrow `LargeList` column with items of logical type `L`:
/// like [`List`](crate::List), with 64-bit offsets.
///
/// Item nullability: `LargeList<Option<L>>`.
///
/// ```
/// use quiver::{Column, LargeList};
///
/// let column = Column::<LargeList<i64>>::from_values([vec![1, 2], vec![3]]);
/// let second: Vec<i64> = column.value(1).collect();
/// assert_eq!(second, [3]);
/// ```
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct LargeList<L> {
    _marker: PhantomData<fn() -> L>,
}

/// The validated representation of a `LargeList` column:
/// the list array plus its downcast values.
pub struct TypedLargeList<L: Datatype> {
    list: arrow::array::LargeListArray,
    values: L::Typed,
}

impl_list_datatype!(
    LargeList,
    TypedLargeList,
    arrow::array::LargeListArray,
    LargeList
);
