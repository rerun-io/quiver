//! `#[quiver(null_if_missing)]` cannot be combined with an `Option<…>` field.

#[derive(quiver::Quiver)]
struct Thing {
    #[quiver(null_if_missing)]
    name: Option<quiver::Column<quiver::Utf8>>,
}

fn main() {}
