//! [`Date32`] and [`Date64`]: logical types for columns of calendar dates.
//!
//! Each element is a date without a time-of-day or timezone:
//! [`Date32`] counts *days* since the Unix epoch (1970-01-01) as an `i32`,
//! stored as an [`arrow::array::Date32Array`]
//! ([`DataType::Date32`]);
//! [`Date64`] counts *milliseconds* since the epoch as an `i64`
//! (expected to be a multiple of a day; not validated),
//! stored as an [`arrow::array::Date64Array`].

use arrow::array::{Array, ArrayRef};
use arrow::datatypes::DataType;

use crate::datatype::{ColumnError, Datatype, impl_marker_datatype, impl_primitive_datatype};

/// Days since the Unix epoch, as an `i32`.
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct Date32;

/// Milliseconds since the Unix epoch, as an `i64`
/// (expected to be a multiple of a day; not validated).
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct Date64;

impl_marker_datatype!(
    Date32,
    arrow::array::Date32Array,
    i32,
    i32,
    DataType::Date32
);
impl_marker_datatype!(
    Date64,
    arrow::array::Date64Array,
    i64,
    i64,
    DataType::Date64
);

impl_primitive_datatype!(Date32, i32);
impl_primitive_datatype!(Date64, i64);
