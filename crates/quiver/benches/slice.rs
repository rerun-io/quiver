//! Benchmarks for [`TypedArray::slice`], which re-runs the logical type's
//! `downcast` on the sliced arrow array — including, for nested types, a scan of
//! the children's validity bitmaps.
//!
//! Self-contained: no benchmark framework, just a small timing harness, to keep
//! the dependency tree lean (`cargo-deny` denies duplicate versions).
//!
//! Run with `cargo bench -p quiver --bench slice`.

#![expect(clippy::print_stdout, reason = "a benchmark binary prints its results")]

use std::hint::black_box;
use std::time::{Duration, Instant};

use quiver::{List, TypedArray, Utf8};

const FLAT_LEN: usize = 1_000_000;
const LIST_ROWS: usize = 100_000;
const LIST_ITEMS_PER_ROW: usize = 10;

fn main() {
    let flat = TypedArray::<i64>::from_values(make_values(FLAT_LEN));
    run("i64 (1M)", || black_box(&flat).slice(1, FLAT_LEN - 2).len());

    let strings =
        TypedArray::<Utf8>::from_values((0..LIST_ROWS).map(|index| format!("row {index}")));
    run("Utf8 (100k)", || {
        black_box(&strings).slice(1, LIST_ROWS - 2).len()
    });

    let rows: Vec<Vec<i64>> = (0..LIST_ROWS)
        .map(|_| make_values(LIST_ITEMS_PER_ROW))
        .collect();
    let lists = TypedArray::<List<i64>>::from_values(rows);
    run("List<i64> (100k rows)", || {
        black_box(&lists).slice(1, LIST_ROWS - 2).len()
    });

    // The worst case: non-nullable items whose child array still carries a
    // validity buffer, so every `downcast` scans that bitmap to count the nulls
    // reachable through valid rows.
    let with_buffer = list_with_all_valid_item_buffer();
    run("List<i64>, item null buffer", || {
        black_box(&with_buffer).slice(1, LIST_ROWS - 2).len()
    });

    let nested: Vec<Vec<Vec<i64>>> = (0..LIST_ROWS)
        .map(|_| vec![make_values(LIST_ITEMS_PER_ROW)])
        .collect();
    let nested = TypedArray::<List<List<i64>>>::from_values(nested);
    run("List<List<i64>> (100k rows)", || {
        black_box(&nested).slice(1, LIST_ROWS - 2).len()
    });
}

/// A `List<i64>` whose items are non-nullable, but whose values array carries an
/// all-valid null buffer — which quiver has to scan, since a sliced array may
/// hold nulls outside the range its rows reach.
fn list_with_all_valid_item_buffer() -> TypedArray<List<i64>> {
    use std::sync::Arc;

    use quiver::arrow::array::{Int64Array, ListArray};
    use quiver::arrow::buffer::{NullBuffer, OffsetBuffer};
    use quiver::arrow::datatypes::{DataType, Field};

    let items = LIST_ROWS * LIST_ITEMS_PER_ROW;
    let values = Int64Array::new(
        make_values(items).into(),
        Some(NullBuffer::new_valid(items)),
    );
    let offsets = OffsetBuffer::from_lengths(std::iter::repeat_n(LIST_ITEMS_PER_ROW, LIST_ROWS));
    let field = Arc::new(Field::new("item", DataType::Int64, false));
    let array = ListArray::new(field, offsets, Arc::new(values), None);
    TypedArray::try_new(Arc::new(array)).expect("valid `List<i64>`")
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

/// Times one `slice` call, over enough iterations to be meaningful.
fn run(name: &str, mut f: impl FnMut() -> usize) {
    let expected = f();
    for _ in 0..4 {
        assert_eq!(f(), expected, "`{name}` changed length");
    }

    let runs: u32 = 10_000;
    let mut checksum = 0_usize;
    let start = Instant::now();
    for _ in 0..runs {
        checksum = checksum.wrapping_add(black_box(f()));
    }
    let elapsed = start.elapsed();
    black_box(checksum);

    let per_call: Duration = elapsed / runs;
    println!("  slice of {name:<28} {per_call:>10.2?} per call");
}
