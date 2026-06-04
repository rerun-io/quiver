//! Difficult and exotic datatypes are explicitly unsupported.

use arrow_quiver::arrow::array::MapArray;

#[derive(arrow_quiver::Quiver)]
struct Thing {
    map: MapArray,
}

fn main() {}
