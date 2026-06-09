//! Logical types for columns of time-of-day values.
//!
//! Each element is a time since midnight, without a date or timezone:
//! e.g. a [`Time64Nanosecond`] element is the number of nanoseconds since
//! midnight, as an `i64`. Stored as the [`arrow::array::Time32SecondArray`]
//! family ([`DataType::Time32`] /
//! [`DataType::Time64`]).
//!
//! Following arrow, the 32-bit variants cover second/millisecond resolution
//! and the 64-bit variants microsecond/nanosecond.

use arrow::array::{Array, ArrayRef};
use arrow::datatypes::{DataType, TimeUnit};

use crate::datatype::{ColumnError, LogicalType, impl_marker_datatype, impl_primitive_datatype};

/// Seconds since midnight, as an `i32`.
///
/// ```
/// use quiver::{Column, Time32Second};
///
/// let column = Column::<Time32Second>::from_values([3_600, 7_200]); // 01:00, 02:00
/// assert_eq!(column.value(0), 3_600);
/// ```
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct Time32Second;

/// Milliseconds since midnight, as an `i32`.
///
/// ```
/// use quiver::{Column, Time32Millisecond};
///
/// let column = Column::<Time32Millisecond>::from_values([3_600_000]); // 01:00
/// assert_eq!(column.value(0), 3_600_000);
/// ```
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct Time32Millisecond;

/// Microseconds since midnight, as an `i64`.
///
/// ```
/// use quiver::{Column, Time64Microsecond};
///
/// let column = Column::<Time64Microsecond>::from_values([3_600_000_000_i64]); // 01:00
/// assert_eq!(column.value(0), 3_600_000_000);
/// ```
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct Time64Microsecond;

/// Nanoseconds since midnight, as an `i64`.
///
/// ```
/// use quiver::{Column, Time64Nanosecond};
///
/// let column = Column::<Time64Nanosecond>::from_values([3_600_000_000_000_i64]); // 01:00
/// assert_eq!(column.value(0), 3_600_000_000_000);
/// ```
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

impl_primitive_datatype!(Time32Second, i32);
impl_primitive_datatype!(Time32Millisecond, i32);
impl_primitive_datatype!(Time64Microsecond, i64);
impl_primitive_datatype!(Time64Nanosecond, i64);
