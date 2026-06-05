//! `exhaustive` and `nonexhaustive` conflict.

use quiver::arrow::array::StringArray;

#[derive(quiver::Quiver)]
#[quiver(exhaustive, nonexhaustive)]
struct Thing {
    name: StringArray,
}

fn main() {}
