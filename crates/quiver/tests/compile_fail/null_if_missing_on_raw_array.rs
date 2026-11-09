//! `#[quiver(null_if_missing)]` is only supported on `quiver::Column<Option<…>>`.

#[derive(quiver::Quiver)]
struct Thing {
    #[quiver(null_if_missing)]
    name: quiver::arrow::array::StringArray,
}

fn main() {}
