//! `Dictionary<Option<K>, V>` is not a thing:
//! row nullability is `Option<Dictionary<K, V>>`.

use quiver::{LogicalType, Dictionary, Utf8};

fn assert_datatype<L: LogicalType>() {}

fn main() {
    assert_datatype::<Dictionary<Option<i32>, Utf8>>();
}
