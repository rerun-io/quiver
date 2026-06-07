//! Difficult and exotic datatypes are explicitly unsupported.

use quiver::arrow::array::UnionArray;

#[derive(quiver::Quiver)]
struct Thing {
    onion: UnionArray,
}

fn main() {}
