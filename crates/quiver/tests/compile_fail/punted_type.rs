//! Some datatypes are not yet supported, not even as raw arrow fields.

use quiver::arrow::array::UnionArray;

#[derive(quiver::Quiver)]
struct Thing {
    onion: UnionArray,
}

fn main() {}
