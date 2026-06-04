//! `Dictionary<Option<K>, V>` is not a thing:
//! row nullability is `Option<Dictionary<K, V>>`.

use arrow_quiver::{Column, Dictionary};

fn main() {
    let _ = Column::<Dictionary<Option<i32>, String>>::datatype();
}
