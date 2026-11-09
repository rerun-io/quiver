//! `#[quiver(null_if_missing)]` requires a nullable `Column<Option<…>>`.

#[derive(quiver::Quiver)]
struct Thing {
    #[quiver(null_if_missing)]
    name: quiver::Column<quiver::Utf8>,
}

fn main() {}
