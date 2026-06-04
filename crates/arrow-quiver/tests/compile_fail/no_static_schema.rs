//! Structs with dynamically-typed columns (`ArrayRef`, `ListArray`, …)
//! get no static `schema()`.

use arrow_quiver::arrow::array::ArrayRef;

#[derive(arrow_quiver::Quiver)]
struct Thing {
    anything: ArrayRef,
}

fn main() {
    let _ = Thing::schema();
}
