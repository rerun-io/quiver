//! Unknown `#[quiver(…)]` arguments are an error.

use quiver::arrow::array::StringArray;

#[derive(quiver::Quiver)]
struct Thing {
    #[quiver(nullable)]
    name: StringArray,
}

fn main() {}
