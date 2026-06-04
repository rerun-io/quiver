//! Unknown `#[quiver(…)]` arguments are an error.

use arrow_quiver::arrow::array::StringArray;

#[derive(arrow_quiver::Quiver)]
struct Thing {
    #[quiver(nullable)]
    name: StringArray,
}

fn main() {}
