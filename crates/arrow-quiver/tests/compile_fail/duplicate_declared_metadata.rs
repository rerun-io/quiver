//! Only one `metadata(…)` attribute per field.

use arrow_quiver::Column;

#[derive(arrow_quiver::Quiver)]
struct Thing {
    #[quiver(metadata("a" = "1"))]
    #[quiver(metadata("b" = "2"))]
    chunk_id: Column<[u8; 16]>,
}

fn main() {}
