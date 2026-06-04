//! `String`: a logical type for columns of UTF-8 text.
//!
//! A `Column<String>` is a column of strings, stored as an
//! [`arrow::array::StringArray`] ([`DataType::Utf8`](arrow::datatypes::DataType::Utf8)).
//! Reading is zero-copy: the element values are `&str` borrows into the array.

use arrow::array::{Array, ArrayRef};
use arrow::datatypes::DataType;

use crate::datatype::{ColumnError, Datatype, downcast_array, impl_flat_datatype};

impl_flat_datatype!(String, arrow::array::StringArray, &'a str, DataType::Utf8);
