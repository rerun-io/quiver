use arrow::datatypes::DataType;
use arrow::error::ArrowError;

/// The errors that can happen when converting between a record batch and a typed record.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Missing column {column:?}")]
    MissingColumn { column: String },

    #[error("Column {column:?}: expected datatype {expected:?}, found {actual:?}")]
    WrongDatatype {
        column: String,
        expected: DataType,
        actual: DataType,
    },

    #[error("Unexpected column {column:?}")]
    UnexpectedColumn { column: String },

    #[error("Column {column:?} has {null_count} null(s), but was marked as non-null")]
    UnexpectedNulls { column: String, null_count: usize },

    #[error("Column {column:?} failed to downcast to the expected array type")]
    DowncastFailed { column: String },

    #[error(transparent)]
    Arrow(#[from] ArrowError),
}
