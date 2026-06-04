//! Generic structs are not supported.

#[derive(arrow_quiver::Quiver)]
struct Thing<T> {
    name: T,
}

fn main() {}
