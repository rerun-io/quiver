use arrow::datatypes::DataType;
use arrow::error::ArrowError;

/// An error from converting between a record batch and a `#[derive(Quiver)]` struct.
#[derive(Debug, thiserror::Error)]
#[error("{record_type}: {kind}")]
pub struct Error {
    /// The name of the `#[derive(Quiver)]` struct that was converted to/from.
    pub record_type: &'static str,

    pub kind: ErrorKind,
}

/// What went wrong when converting between a record batch and a `#[derive(Quiver)]` struct.
#[derive(Debug, thiserror::Error)]
pub enum ErrorKind {
    #[error(
        "Missing required column {column:?}. If the column is allowed to be missing, declare the field as `Option<…>`"
    )]
    MissingColumn { column: String },

    #[error("Column {column:?}: expected datatype {}, found {actual:?}", fmt_expected(expected.as_ref()))]
    WrongDatatype {
        column: String,

        /// `None` when the expected logical type has no static datatype
        /// (it contains a [`Dyn`](crate::Dyn) leaf).
        expected: Option<DataType>,

        actual: DataType,
    },

    #[error(
        "Unexpected column {column:?}. Either add it to the struct, or accept unknown columns with a `#[quiver(extra_columns)]` field"
    )]
    UnexpectedColumn { column: String },

    #[error(
        "Column {column:?} has {null_count} null(s) at a non-nullable level. Use `Option<…>` in the logical type to allow nulls"
    )]
    UnexpectedNulls { column: String, null_count: usize },

    #[error("Column {column:?}: expected a {expected}, found datatype {actual:?}")]
    WrongArrayType {
        column: String,

        /// Name of the expected array type, e.g. `ListArray`.
        expected: String,

        actual: DataType,
    },

    #[error("Failed to build the record batch: {0}")]
    BuildRecordBatch(ArrowError),
}

/// Formats the `expected` datatype of a `WrongDatatype` error.
pub(crate) fn fmt_expected(expected: Option<&DataType>) -> String {
    match expected {
        Some(datatype) => format!("{datatype:?}"),
        None => "<dynamic>".to_owned(),
    }
}

/// Lets `?` convert quiver errors in functions returning arrow results.
///
/// The error is preserved (including its source chain),
/// wrapped as an [`ArrowError::ExternalError`] —
/// except [`ErrorKind::BuildRecordBatch`], which returns the original [`ArrowError`].
impl From<Error> for ArrowError {
    fn from(err: Error) -> Self {
        if let ErrorKind::BuildRecordBatch(arrow_err) = err.kind {
            arrow_err
        } else {
            Self::ExternalError(Box::new(err))
        }
    }
}
