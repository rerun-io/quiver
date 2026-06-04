//! Declared metadata keys/values must be string literals.

use arrow_quiver::Column;

#[derive(arrow_quiver::Quiver)]
struct Thing {
    #[quiver(metadata(kind = "control"))]
    chunk_id: Column<[u8; 16]>,
}

fn main() {}
