//! `#[quiver(null_if_missing)]` cannot be combined with an `Option<…>` field, even around a nullable column.

#[derive(quiver::Quiver)]
struct Thing {
    #[quiver(null_if_missing)]
    name: Option<quiver::Column<Option<quiver::Utf8>>>,
}

fn main() {}
