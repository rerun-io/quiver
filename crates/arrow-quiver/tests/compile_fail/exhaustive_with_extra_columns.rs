//! The `exhaustive`/`nonexhaustive` attributes cannot be combined
//! with an `extra_columns` field.

use arrow_quiver::DynColumn;

#[derive(arrow_quiver::Quiver)]
#[quiver(nonexhaustive)]
struct Thing {
    #[quiver(extra_columns)]
    extra: Vec<DynColumn>,
}

fn main() {}
