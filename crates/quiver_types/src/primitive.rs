//! Primitive logical types: `bool`, integers, and floating point numbers.
//!
//! These use the plain Rust types directly: a `Column<i64>` is a column of
//! 64-bit integers, stored as an [`arrow::array::Int64Array`]
//! ([`DataType::Int64`]), and so on.
//! `f16` comes from the [`half`] crate (re-exported as [`crate::half`]),
//! matching its use in [`arrow::array::Float16Array`].
//!
//! See [`Datatype`](crate::Datatype) for a usage example.

use arrow::array::{Array, ArrayRef};
use arrow::datatypes::DataType;

use crate::datatype::{
    ColumnError, Datatype, downcast_array, impl_flat_datatype, impl_primitive_datatype,
};

impl_flat_datatype!(bool, arrow::array::BooleanArray, bool, DataType::Boolean);
impl_flat_datatype!(i8, arrow::array::Int8Array, i8, DataType::Int8);
impl_flat_datatype!(i16, arrow::array::Int16Array, i16, DataType::Int16);
impl_flat_datatype!(i32, arrow::array::Int32Array, i32, DataType::Int32);
impl_flat_datatype!(i64, arrow::array::Int64Array, i64, DataType::Int64);
impl_flat_datatype!(u8, arrow::array::UInt8Array, u8, DataType::UInt8);
impl_flat_datatype!(u16, arrow::array::UInt16Array, u16, DataType::UInt16);
impl_flat_datatype!(u32, arrow::array::UInt32Array, u32, DataType::UInt32);
impl_flat_datatype!(u64, arrow::array::UInt64Array, u64, DataType::UInt64);
impl_flat_datatype!(
    half::f16,
    arrow::array::Float16Array,
    half::f16,
    DataType::Float16
);
impl_flat_datatype!(f32, arrow::array::Float32Array, f32, DataType::Float32);
impl_flat_datatype!(f64, arrow::array::Float64Array, f64, DataType::Float64);

// `bool` is excluded: arrow bit-packs booleans, so there is no `&[bool]` to expose.
impl_primitive_datatype!(i8, i8);
impl_primitive_datatype!(i16, i16);
impl_primitive_datatype!(i32, i32);
impl_primitive_datatype!(i64, i64);
impl_primitive_datatype!(u8, u8);
impl_primitive_datatype!(u16, u16);
impl_primitive_datatype!(u32, u32);
impl_primitive_datatype!(u64, u64);
impl_primitive_datatype!(half::f16, half::f16);
impl_primitive_datatype!(f32, f32);
impl_primitive_datatype!(f64, f64);
