//! Schema specification and validation for [Apache Arrow](https://arrow.apache.org/) record batches.
//!
//! A quiver holds arrows; this crate holds typed Arrow arrays.
//!
//! The runtime [`Schema`] is the engine; the `#[derive(Record)]` proc-macro
//! (behind the `derive` feature) is sugar on top.

use std::collections::BTreeSet;

#[cfg(feature = "derive")]
pub use arrow_quiver_derive::Record;

/// Describes the expected contents of a [`arrow::record_batch::RecordBatch`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Schema {
    pub name: String,
    pub docstring: String,
    pub metadata_schema: MetadataSchema,
    pub fields: Vec<FieldSchema>,

    /// Are unknown fields ok, or an error?
    pub allow_extra_fields: bool,
}

/// Describes a single expected column of a [`arrow::record_batch::RecordBatch`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldSchema {
    pub name: String,
    pub docstring: String,
    pub metadata_schema: MetadataSchema,

    /// Is this field allowed to be missing?
    pub optional: bool,

    pub datatype: DatatypeSchema,
}

/// Describes the expected datatype of a column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DatatypeSchema {
    /// Any datatype is accepted.
    Any,

    /// Only this exact datatype is accepted.
    Specific(arrow::datatypes::DataType),
}

/// Describes the expected metadata of a record batch or field.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MetadataSchema {
    pub required_fields: BTreeSet<String>,

    /// Are unknown fields ok, or an error?
    pub allow_extra_fields: bool,
}
