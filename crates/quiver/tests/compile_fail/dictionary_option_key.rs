//! `Dictionary<Option<K>, V>` is not a thing:
//! row nullability is `Option<Dictionary<K, V>>`.

use quiver::{Datatype, Dictionary};

fn assert_datatype<L: Datatype>() {}

fn main() {
    assert_datatype::<Dictionary<Option<i32>, String>>();
}
