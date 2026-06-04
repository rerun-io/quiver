//! Logical types for columns of time-of-day values.
//!
//! Each element is a time since midnight, without a date or timezone:
//! e.g. a [`Time64Nanosecond`] element is the number of nanoseconds since
//! midnight, as an `i64`. Stored as the [`arrow::array::Time32SecondArray`]
//! family ([`DataType::Time32`](arrow::datatypes::DataType::Time32) /
//! [`DataType::Time64`](arrow::datatypes::DataType::Time64)).
//!
//! Following arrow, the 32-bit variants cover second/millisecond resolution
//! and the 64-bit variants microsecond/nanosecond.

use arrow::array::{Array, ArrayRef};
use arrow::datatypes::{DataType, TimeUnit};

use crate::datatype::{ColumnError, Datatype, impl_marker_datatype};

/// Seconds since midnight, as an `i32`.
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct Time32Second;

/// Milliseconds since midnight, as an `i32`.
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct Time32Millisecond;

/// Microseconds since midnight, as an `i64`.
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct Time64Microsecond;

/// Nanoseconds since midnight, as an `i64`.
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct Time64Nanosecond;

impl_marker_datatype!(
    Time32Second,
    arrow::array::Time32SecondArray,
    i32,
    i32,
    DataType::Time32(TimeUnit::Second)
);
impl_marker_datatype!(
    Time32Millisecond,
    arrow::array::Time32MillisecondArray,
    i32,
    i32,
    DataType::Time32(TimeUnit::Millisecond)
);
impl_marker_datatype!(
    Time64Microsecond,
    arrow::array::Time64MicrosecondArray,
    i64,
    i64,
    DataType::Time64(TimeUnit::Microsecond)
);
impl_marker_datatype!(
    Time64Nanosecond,
    arrow::array::Time64NanosecondArray,
    i64,
    i64,
    DataType::Time64(TimeUnit::Nanosecond)
);
