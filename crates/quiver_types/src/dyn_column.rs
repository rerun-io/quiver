//! [`DynColumn`]: one column of a record batch, kept dynamically typed.

use arrow::array::{Array as _, ArrayRef};
use arrow::datatypes::FieldRef;

use crate::{Column, Error, ErrorKind, LogicalType};

/// A single dynamically-typed column of a record batch:
/// the field description plus the actual data.
///
/// The two halves always agree: [`try_new`](DynColumn::try_new) checks that the
/// array has exactly the field's data type, and that a non-nullable field holds
/// no nulls — the same two invariants a
/// [`RecordBatch`](arrow::record_batch::RecordBatch) demands of a column, so a
/// `DynColumn` can always be put into one.
///
/// The untyped counterpart of [`Column`]: it carries no logical type, and so
/// validates nothing about the *contents*. Use
/// [`try_into_column`](DynColumn::try_into_column) to get back to a typed
/// column.
#[derive(Clone, Debug)]
pub struct DynColumn {
    field: FieldRef,
    array: ArrayRef,
}

impl DynColumn {
    /// Pairs an arrow field with the array holding that column's data.
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use quiver::arrow::array::{ArrayRef, Int64Array};
    /// # use quiver::arrow::datatypes::{DataType, Field};
    /// # use quiver::DynColumn;
    /// let array: ArrayRef = Arc::new(Int64Array::from(vec![1, 2]));
    /// let field = Arc::new(Field::new("frame", DataType::Int64, false));
    /// let column = DynColumn::try_new(field, array)?;
    /// assert_eq!(column.array().len(), 2);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Errors
    /// Errors if the array's data type is not exactly the field's, or if the
    /// field is not nullable but the array holds nulls.
    pub fn try_new(field: FieldRef, array: ArrayRef) -> Result<Self, Error> {
        let column = field.name().clone();

        if array.data_type() != field.data_type() {
            return Err(Error::new(
                "DynColumn",
                ErrorKind::WrongDataType {
                    column,
                    expected: format!("{:?}", field.data_type()),
                    actual: array.data_type().clone(),
                },
            ));
        }

        let null_count = array.null_count();
        if 0 < null_count && !field.is_nullable() {
            return Err(Error::new(
                "DynColumn",
                ErrorKind::UnexpectedNulls { column, null_count },
            ));
        }

        Ok(Self::new_unvalidated(field, array))
    }

    /// [`try_new`](DynColumn::try_new) without the checks, for the callers that
    /// build the field and the array together and so cannot violate the
    /// invariants: [`Column::into_dyn`], and extraction from a record batch
    /// (which arrow has already validated the same two ways).
    pub(crate) fn new_unvalidated(field: FieldRef, array: ArrayRef) -> Self {
        Self { field, array }
    }

    /// The name of the column in the record batch.
    ///
    /// Shorthand for `self.field().name()`.
    #[must_use]
    pub fn name(&self) -> &str {
        self.field.name()
    }

    /// The arrow field: the column's name, data type, nullability, and metadata.
    #[must_use]
    pub fn field(&self) -> &FieldRef {
        &self.field
    }

    /// The column's data, of the [`field`](DynColumn::field)'s data type.
    #[must_use]
    pub fn array(&self) -> &ArrayRef {
        &self.array
    }

    /// The two halves, moved out — for building a record batch by hand.
    #[must_use]
    pub fn into_parts(self) -> (FieldRef, ArrayRef) {
        let Self { field, array } = self;
        (field, array)
    }

    /// Validates this column against the logical type `L` (data type and
    /// nullability, recursively) and downcasts it (zero-copy), carrying over
    /// the arrow field metadata.
    ///
    /// The inverse is [`Column::into_dyn`].
    ///
    /// # Errors
    /// Errors on data type mismatch, or on nulls at any non-`Option` nesting level.
    pub fn try_into_column<L: LogicalType>(self) -> Result<Column<L>, Error> {
        let Self { field, array } = self;

        let column = Column::<L>::try_new(array)
            .map_err(|err| Error::new("DynColumn", err.for_column(field.name().clone())))?;

        Ok(column.with_metadata(
            field
                .metadata()
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        ))
    }
}
