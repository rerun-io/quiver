//! Only one `metadata(…)` attribute per field.

use quiver::{Column, FixedSizeBinary};

#[derive(quiver::Quiver)]
struct Thing {
    #[quiver(metadata("a" = "1"))]
    #[quiver(metadata("b" = "2"))]
    chunk_id: Column<FixedSizeBinary<16>>,
}

fn main() {}
