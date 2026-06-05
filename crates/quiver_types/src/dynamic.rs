//! [`Dyn`]: a dynamically-typed *leaf* in an otherwise statically-typed logical type.
//!
//! `Column<List<Dyn>>` validates the structure (the list shape, and nulls at
//! every non-`Option` level) but accepts *any* item datatype; the items are
//! read as [`ArrayRef`]s. This gives dynamic-leaf dataframes quiver's
//! structural validation, dictionary key→values indirection, and logical-null
//! handling for sliced arrays — without committing to a leaf datatype.

use arrow::array::{Array, ArrayRef, make_array};
use arrow::datatypes::DataType;

use crate::{ColumnError, Datatype};

/// A dynamically-typed leaf: accepts *any* arrow datatype.
///
/// The structure *around* it is still validated, e.g. `Column<List<Dyn>>`
/// requires a list array, but the item datatype is unconstrained.
///
/// Nullability works like for every logical type: `Dyn` itself is
/// *never* null — a `Column<Dyn>` must be null-free, and a `Column<List<Dyn>>`
/// must have no null items. Wrap in `Option<…>` to allow nulls:
/// `Column<Option<Dyn>>`, `Column<List<Option<Dyn>>>`.
///
/// Reading yields [`ArrayRef`]s: [`Column::value`](crate::Column::value)
/// on a `Column<Dyn>` is a one-row zero-copy slice, and a whole
/// `List<Dyn>` row can be taken as one array with
/// [`ListValue::as_arrow`](crate::ListValue::as_arrow).
///
/// Limitations, since the datatype is unknown until runtime:
/// * Building from values (`from_values`, `try_from_values`, …) is not
///   supported — construct the arrow array directly and use
///   [`Column::try_new`](crate::Column::try_new). `Default` (the empty
///   column) is unavailable for the same reason, and panics.
/// * [`Datatype::datatype`] returns `None`, for `Dyn` and anything
///   containing it. Error messages show the expected datatype as
///   `<dynamic>`, and the generated `min_schema`/`max_schema`/
///   `empty_record_batch` of a `#[derive(Quiver)]` struct panic if a column
///   has no static datatype — prefer raw [`ArrayRef`] fields in derive
///   structs if you need static schemas. Encoding (`into_record_batch`)
///   works: the field datatype is taken from the actual array.
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct Dyn {
    _marker: std::marker::PhantomData<fn() -> Self>,
}

impl Datatype for Dyn {
    type Typed = ArrayRef;
    type Value<'a> = ArrayRef;
    type Owned = ArrayRef;

    /// `None`: `Dyn` has no static datatype.
    /// Validation goes through [`Dyn::matches`] instead, which accepts everything.
    fn datatype() -> Option<DataType> {
        None
    }

    /// Everything matches.
    fn matches(_actual: &DataType) -> bool {
        true
    }

    fn downcast(array: &dyn Array) -> Result<Self::Typed, ColumnError> {
        Ok(make_array(array.to_data()))
    }

    fn is_null(typed: &Self::Typed, index: usize) -> bool {
        typed.is_null(index)
    }

    /// A one-row zero-copy slice of the underlying array.
    fn value(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
        typed.slice(index, 1)
    }

    /// Not supported: there is no datatype to build with
    /// (and none can be inferred from empty input).
    ///
    /// Construct the arrow array directly and use
    /// [`Column::try_new`](crate::Column::try_new) instead.
    fn build(_values: impl Iterator<Item = Option<Self::Owned>>) -> Result<ArrayRef, ColumnError> {
        Err(ColumnError::Build(
            arrow::error::ArrowError::NotYetImplemented(
                "Cannot build a `Dyn` column from values: the datatype is unknown. \
                 Construct the arrow array directly and use `Column::try_new`"
                    .to_owned(),
            ),
        ))
    }

    fn to_owned_value(value: Self::Value<'_>) -> Self::Owned {
        value
    }
}
