//! Only one field may be marked `#[quiver(metadata)]`.

use std::collections::BTreeMap;

#[derive(arrow_quiver::Quiver)]
struct Thing {
    #[quiver(metadata)]
    metadata: BTreeMap<String, String>,

    #[quiver(metadata)]
    more_metadata: BTreeMap<String, String>,
}

fn main() {}
