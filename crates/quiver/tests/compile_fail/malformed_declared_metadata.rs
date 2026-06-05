//! Declared metadata keys/values must be string literals.

use quiver::Column;

#[derive(quiver::Quiver)]
struct Thing {
    #[quiver(metadata(kind = "control"))]
    chunk_id: Column<[u8; 16]>,
}

fn main() {}
