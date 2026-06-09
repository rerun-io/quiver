//! [`AnyList<L>`]: a logical type that accepts *any* of arrow's list encodings.
//!
//! Arrow has five physically different ways to store the same logical thing — a
//! column of lists of `L`: [`List`], [`LargeList`], [`ListView`],
//! [`LargeListView`], and [`FixedSizeList`](crate::FixedSizeList).
//! `Column<AnyList<L>>` parses whichever one it is handed and reads them all
//! uniformly — each element is an iterator ([`ListValue`]) over the items,
//! exactly like the individual list types.
//!
//! Use it when the encoding is decided at runtime (e.g. data from an external
//! source) and you only care about the logical list. The concrete types stay
//! preferable when you *know* (and want to enforce) the encoding.
//!
//! `AnyList` is *parse-only*: it is not a [`ConcreteType`](crate::ConcreteType)
//! (it has no single arrow datatype), so it has no `from_values`/`Default`/schema.
//! To build, pick a concrete encoding such as `Column<List<L>>`.

use std::marker::PhantomData;

use arrow::array::{Array, FixedSizeListArray};
use arrow::datatypes::ArrowNativeType as _;
use arrow::datatypes::DataType;

use crate::datatype::{ColumnError, LogicalType, downcast_array};
use crate::fixed_size_list::logical_item_null_count;
use crate::list::ListValue;
use crate::{
    LargeList, LargeListView, List, ListView, TypedLargeList, TypedLargeListView, TypedList,
    TypedListView,
};

/// Marker for a list column in *any* of arrow's list encodings.
///
/// Accepts [`List`], [`LargeList`], [`ListView`], [`LargeListView`], or
/// [`FixedSizeList`](crate::FixedSizeList). Reads are uniform across all five;
/// building emits a plain [`List`]. Item nullability is `AnyList<Option<L>>`.
///
/// ```
/// use quiver::{AnyList, Column};
/// use quiver::arrow::array::{ArrayRef, LargeListArray};
/// use quiver::arrow::datatypes::Int64Type;
/// # use std::sync::Arc;
///
/// // Accepts a `List`, `LargeList`, `ListView`, `LargeListView`, or `FixedSizeList`
/// // — here a `LargeList` — and reads them all the same way:
/// let large = LargeListArray::from_iter_primitive::<Int64Type, _, _>(vec![Some(vec![Some(7)])]);
/// let column = Column::<AnyList<i64>>::try_from(Arc::new(large) as ArrayRef).unwrap();
/// assert_eq!(column.value(0).collect::<Vec<_>>(), [7]);
/// ```
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct AnyList<L> {
    _marker: PhantomData<fn() -> L>,
}

/// The validated representation of an [`AnyList`] column: one of the per-encoding
/// typed representations.
pub enum AnyTypedList<L: LogicalType> {
    List(TypedList<L>),
    LargeList(TypedLargeList<L>),
    ListView(TypedListView<L>),
    LargeListView(TypedLargeListView<L>),
    FixedSizeList {
        array: FixedSizeListArray,
        values: L::Typed,
    },
}

// Hand-written (not derived): `#[derive(Clone)]` would add a spurious `L: Clone`
// bound, but only `L::Typed: Clone` is needed — the markers (`Utf8`, …) are not
// `Clone`. Same reason `TypedList` & co. hand-write it.
impl<L: LogicalType> Clone for AnyTypedList<L> {
    fn clone(&self) -> Self {
        match self {
            Self::List(typed) => Self::List(typed.clone()),
            Self::LargeList(typed) => Self::LargeList(typed.clone()),
            Self::ListView(typed) => Self::ListView(typed.clone()),
            Self::LargeListView(typed) => Self::LargeListView(typed.clone()),
            Self::FixedSizeList { array, values } => Self::FixedSizeList {
                array: array.clone(),
                values: values.clone(),
            },
        }
    }
}

impl<L: LogicalType + 'static> LogicalType for AnyList<L> {
    type Typed = AnyTypedList<L>;
    type Value<'a>
        = ListValue<'a, L>
    where
        Self: 'a;
    type Owned = Vec<L::Owned>;

    fn downcast(array: &dyn Array) -> Result<Self::Typed, ColumnError> {
        match array.data_type() {
            DataType::List(_) => Ok(AnyTypedList::List(List::<L>::downcast(array)?)),
            DataType::LargeList(_) => Ok(AnyTypedList::LargeList(LargeList::<L>::downcast(array)?)),
            DataType::ListView(_) => Ok(AnyTypedList::ListView(ListView::<L>::downcast(array)?)),
            DataType::LargeListView(_) => Ok(AnyTypedList::LargeListView(
                LargeListView::<L>::downcast(array)?,
            )),
            DataType::FixedSizeList(_, _) => {
                let array = downcast_array::<FixedSizeListArray>(array)?;
                if !L::NULLABLE {
                    let null_count = logical_item_null_count(&array);
                    if 0 < null_count {
                        return Err(ColumnError::UnexpectedNulls { null_count });
                    }
                }
                let values = L::downcast(&**array.values())?;
                Ok(AnyTypedList::FixedSizeList { array, values })
            }
            actual => Err(ColumnError::WrongDatatype {
                actual: actual.clone(),
            }),
        }
    }

    fn is_null(typed: &Self::Typed, index: usize) -> bool {
        match typed {
            AnyTypedList::List(typed) => List::<L>::is_null(typed, index),
            AnyTypedList::LargeList(typed) => LargeList::<L>::is_null(typed, index),
            AnyTypedList::ListView(typed) => ListView::<L>::is_null(typed, index),
            AnyTypedList::LargeListView(typed) => LargeListView::<L>::is_null(typed, index),
            AnyTypedList::FixedSizeList { array, .. } => array.is_null(index),
        }
    }

    fn value(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
        match typed {
            AnyTypedList::List(typed) => List::<L>::value(typed, index),
            AnyTypedList::LargeList(typed) => LargeList::<L>::value(typed, index),
            AnyTypedList::ListView(typed) => ListView::<L>::value(typed, index),
            AnyTypedList::LargeListView(typed) => LargeListView::<L>::value(typed, index),
            AnyTypedList::FixedSizeList { array, values } => {
                let start = array.value_offset(index).as_usize();
                let size = array.value_length() as usize;
                ListValue::new(values, start, start + size)
            }
        }
    }

    fn to_owned_value(value: Self::Value<'_>) -> Self::Owned {
        value.map(L::to_owned_value).collect()
    }
}

// NOTE: `AnyList` deliberately does *not* implement `ConcreteType`: it accepts
// several arrow encodings, so it has no single `datatype()` to report or build.
// It is parse-only — read via `LogicalType`, but no `from_values`/`Default`/schema.
// To build, pick a concrete encoding such as `Column<List<L>>`.
