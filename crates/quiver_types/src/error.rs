use arrow::datatypes::DataType;
use arrow::error::ArrowError;

/// An error from converting between a record batch and a `#[derive(Quiver)]` struct.
///
/// [`ErrorKind`] is boxed to keep this small: errors travel in the `Err` arm of
/// every `Result` in this crate, and the allocation only happens on the (cold)
/// error path.
#[derive(Debug, thiserror::Error)]
#[error("{record_type}: {kind}")]
pub struct Error {
    /// The name of the `#[derive(Quiver)]` struct that was converted to/from.
    pub record_type: &'static str,

    /// Both interpolated into the message and the [`source`](std::error::Error::source),
    /// so the cause of a failed conversion stays reachable programmatically.
    #[source]
    pub kind: Box<ErrorKind>,
}

impl Error {
    /// Boxes `kind`; the only way to build an [`Error`] without naming [`Box`].
    #[must_use]
    pub fn new(record_type: &'static str, kind: ErrorKind) -> Self {
        Self {
            record_type,
            kind: Box::new(kind),
        }
    }
}

// `Error` rides in the `Err` arm of every `Result` in this crate, and embedders
// assert on the size of error enums that hold it (see
// <https://github.com/rerun-io/quiver/issues/28>). Keep it pointer-sized-small.
const _: () = assert!(
    size_of::<Error>() <= 24,
    "`Error` grew; box the new payload instead"
);

/// What went wrong when converting between a record batch and a `#[derive(Quiver)]` struct.
#[derive(Debug, thiserror::Error)]
pub enum ErrorKind {
    #[error(
        "Missing required column {column:?}. If the column is allowed to be missing, declare the field as `Option<…>`"
    )]
    MissingColumn { column: String },

    #[error("Column {column:?}: expected {expected}, found {actual:?}")]
    WrongDataType {
        column: String,

        /// A description of the expected data type, e.g. `"Utf8"` or `"List(…)"`.
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

    #[error("Column {column:?}: expected a {expected}, found data type {actual:?}")]
    WrongArrayType {
        column: String,

        /// Name of the expected array type, e.g. `ListArray`.
        expected: String,

        actual: DataType,
    },

    /// A fallible domain conversion (`try_newtype_data_type!`) rejected a value
    /// while validating the column at construction.
    #[error("Column {column:?}: failed to convert value: {source}")]
    Conversion {
        column: String,

        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
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
        if let ErrorKind::BuildRecordBatch(arrow_err) = *err.kind {
            arrow_err
        } else {
            Self::ExternalError(Box::new(err))
        }
    }
}
