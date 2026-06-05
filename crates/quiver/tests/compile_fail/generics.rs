//! Generic structs are not supported.

#[derive(quiver::Quiver)]
struct Thing<T> {
    name: T,
}

fn main() {}
