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

    #[error("Column {column:?}: expected {expected}, found {actual:?}")]
    WrongDatatype {
        column: String,

        /// A human description of the expected datatype(s).
        expected: String,

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
