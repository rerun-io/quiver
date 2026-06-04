//! Schema specification and validation for [Apache Arrow](https://arrow.apache.org/) record batches.
//!
//! A quiver holds arrows; this crate holds typed Arrow arrays.
//!
//! Put `#[derive(Quiver)]` (behind the `derive` feature) on a struct of typed Arrow arrays
//! to generate conversions to/from [`arrow::record_batch::RecordBatch`].
//!
//! For strong typing beyond what the raw arrow array types can express
//! (nested datatypes, nullability), use the [`Column`] wrapper —
//! either as struct fields, or standalone:
//!
//! ```
//! # use std::sync::Arc;
//! # use arrow_quiver::arrow::array::{ArrayRef, StringArray};
//! let dynamic_array: ArrayRef = Arc::new(StringArray::from(vec!["foo", "bar"]));
//! let column = arrow_quiver::Column::<String>::try_from(dynamic_array).unwrap();
//! let strings: Vec<&str> = column.iter().collect();
//! assert_eq!(strings, ["foo", "bar"]);
//! ```

pub use arrow;

mod column;
mod error;

pub use self::column::{
    Column, ColumnError, ColumnIter, Datatype, List, ListValue, Microsecond, Millisecond,
    Nanosecond, NoTimezone, Second, TimeUnitSpec, Timestamp, TimezoneSpec, TypedList, Utc,
};
pub use self::error::{Error, ErrorKind};

#[cfg(feature = "derive")]
pub use arrow_quiver_derive::Quiver;

/// A single dynamically-typed column of a record batch:
/// the field description plus the actual data.
#[derive(Clone, Debug)]
pub struct DynColumn {
    pub field: arrow::datatypes::FieldRef,
    pub array: arrow::array::ArrayRef,
}
