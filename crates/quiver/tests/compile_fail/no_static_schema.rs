//! Structs with dynamically-typed columns (`ArrayRef`, `ListArray`, …)
//! get no static schema functions.

use quiver::arrow::array::ArrayRef;

#[derive(quiver::Quiver)]
struct Thing {
    anything: ArrayRef,
}

fn main() {
    let _ = Thing::max_schema();
}
