//! `#[quiver(null_if_missing)]` requires nullable values.

#[derive(quiver::Quiver)]
struct Thing {
    #[quiver(null_if_missing)]
    name: quiver::Column<quiver::Utf8>,
}

fn main() {}
