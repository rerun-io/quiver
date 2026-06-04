//! `exhaustive` and `nonexhaustive` conflict.

use arrow_quiver::arrow::array::StringArray;

#[derive(arrow_quiver::Quiver)]
#[quiver(exhaustive, nonexhaustive)]
struct Thing {
    name: StringArray,
}

fn main() {}
