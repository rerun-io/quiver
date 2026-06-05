//! The `exhaustive`/`nonexhaustive` attributes cannot be combined
//! with an `extra_columns` field.

use quiver::DynColumn;

#[derive(quiver::Quiver)]
#[quiver(nonexhaustive)]
struct Thing {
    #[quiver(extra_columns)]
    extra: Vec<DynColumn>,
}

fn main() {}
