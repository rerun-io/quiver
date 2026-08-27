//! `Column` needs its logical type: the descriptor is a `ColumnDesc<L>`.

use quiver::Column;

#[derive(quiver::Quiver)]
struct Thing {
    name: Column,
}

fn main() {}
