//! `Option<L>`: nullability as part of the logical type.
//!
//! Arrow arrays can contain *nulls* (missing values, tracked in a validity bitmap —
//! see [`arrow::array::Array::nulls`]). In quiver, that possibility is expressed in
//! the type: a `Column<Option<i64>>` may contain nulls and its elements read as
//! `Option<i64>`, while a `Column<i64>` is guaranteed null-free at construction.
//!
//! This works at every nesting level, e.g. `List<Option<String>>` for a list
//! column whose *items* may be null.

use arrow::array::{Array, ArrayRef};
use arrow::datatypes::DataType;

use crate::datatype::InfallibleBuild;
use crate::{ColumnError, Datatype};

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

    fn matches(actual: &DataType) -> bool {
        L::matches(actual)
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

    fn build(values: impl Iterator<Item = Option<Self::Owned>>) -> Result<ArrayRef, ColumnError> {
        L::build(values.map(Option::flatten))
    }

    fn to_owned_value(value: Self::Value<'_>) -> Self::Owned {
        value.map(L::to_owned_value)
    }
}

impl<L: InfallibleBuild> InfallibleBuild for Option<L> {}
