//! Benchmarks for the bulk constructors ([`TypedArray::from_slice`] and
//! [`TypedArray::from_buffer`]) against the per-element
//! [`TypedArray::from_values`].
//!
//! Self-contained: no benchmark framework, just a small timing harness, to keep
//! the dependency tree lean (`cargo-deny` denies duplicate versions).
//!
//! Run with `cargo bench -p quiver --bench build`.

#![expect(clippy::print_stdout, reason = "a benchmark binary prints its results")]

use std::hint::black_box;
use std::time::{Duration, Instant};

use quiver::arrow::buffer::Buffer;
use quiver::{FixedSizeBinary, TypedArray};

const LEN: usize = 100_000;

fn main() {
    let integers = make_values(LEN);
    run("i64, from_values", || {
        TypedArray::<i64>::from_values(black_box(&integers).iter().copied()).len()
    });
    run("i64, from_slice", || {
        TypedArray::<i64>::from_slice(black_box(&integers)).len()
    });

    let buffer = Buffer::from_slice_ref(&integers);
    run("i64, from_buffer", || {
        TypedArray::<i64>::from_buffer(black_box(&buffer).clone())
            .expect("A whole number of aligned `i64`s")
            .len()
    });

    // The case that motivated this: 16-byte ids, which `from_values` builds
    // through `FixedSizeBinaryArray::try_from_sparse_iter_with_size`.
    let ids: Vec<[u8; 16]> = std::iter::zip(make_values(LEN), make_values(LEN))
        .map(|(low, high)| {
            let mut id = [0_u8; 16];
            id[..8].copy_from_slice(&low.to_le_bytes());
            id[8..].copy_from_slice(&high.to_le_bytes());
            id
        })
        .collect();
    run("FixedSizeBinary<16>, from_values", || {
        TypedArray::<FixedSizeBinary<16>>::from_values(black_box(&ids).iter().copied()).len()
    });
    run("FixedSizeBinary<16>, from_slice", || {
        TypedArray::<FixedSizeBinary<16>>::from_slice(black_box(&ids)).len()
    });
}

/// A deterministic-but-not-trivial sequence of `i64`s.
fn make_values(n: usize) -> Vec<i64> {
    let mut value = 0_i64;
    std::iter::repeat_with(|| {
        value = value.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        value
    })
    .take(n)
    .collect()
}

/// Times one build, over enough iterations to be meaningful.
fn run(name: &str, mut f: impl FnMut() -> usize) {
    let expected = f();
    assert_eq!(expected, LEN, "`{name}` built the wrong length");

    let runs: u32 = 200;
    let mut checksum = 0_usize;
    let start = Instant::now();
    for _ in 0..runs {
        checksum = checksum.wrapping_add(black_box(f()));
    }
    let elapsed = start.elapsed();
    black_box(checksum);

    let per_call: Duration = elapsed / runs;
    println!("  build of {name:<34} {per_call:>10.2?} per call");
}
