//! Difficult and exotic datatypes are explicitly unsupported.

use quiver::arrow::array::MapArray;

#[derive(quiver::Quiver)]
struct Thing {
    map: MapArray,
}

fn main() {}
