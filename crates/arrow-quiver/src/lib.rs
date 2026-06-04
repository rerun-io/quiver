//! Schema specification and validation for [Apache Arrow](https://arrow.apache.org/) record batches.
//!
//! A quiver holds arrows; this crate holds typed Arrow arrays.
//!
//! Put `#[derive(Quiver)]` (behind the `derive` feature) on a struct of typed Arrow arrays
//! to generate conversions to/from [`arrow::record_batch::RecordBatch`].

pub use arrow;

mod error;

pub use self::error::Error;

#[cfg(feature = "derive")]
pub use arrow_quiver_derive::Quiver;

/// A single column of a record batch: the field description plus the actual data.
#[derive(Clone, Debug)]
pub struct Column {
    pub field: arrow::datatypes::FieldRef,
    pub array: arrow::array::ArrayRef,
}
