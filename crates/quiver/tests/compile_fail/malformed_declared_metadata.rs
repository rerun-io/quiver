//! Declared metadata keys/values must be string literals.

use quiver::{Column, FixedSizeBinary};

#[derive(quiver::Quiver)]
struct Thing {
    #[quiver(metadata(kind = "control"))]
    chunk_id: Column<FixedSizeBinary<16>>,
}

fn main() {}
