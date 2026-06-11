//! Benchmarks for summing the values of a [`Column`], comparing the access
//! strategies — so the cost of the typesafe iterator (which skips per-element
//! bounds checks, see `ColumnIter`) can be weighed against the alternatives.
//!
//! Self-contained: no benchmark framework, just a small timing harness, to keep
//! the dependency tree lean (`cargo-deny` denies duplicate versions).
//!
//! Run with `cargo bench -p quiver` (build with `--release`, as `cargo bench`
//! does by default).

#![expect(clippy::print_stdout, reason = "a benchmark binary prints its results")]

use std::hint::black_box;
use std::time::{Duration, Instant};

use quiver::{Column, List};

/// Number of `i64`s in the flat column.
const FLAT_LEN: usize = 1_000_000;

/// `LIST_ROWS` list rows of `LIST_ITEMS_PER_ROW` items each.
const LIST_ROWS: usize = 100_000;
const LIST_ITEMS_PER_ROW: usize = 10;

fn main() {
    bench_flat();
    bench_list();
}

/// A deterministic-but-not-trivial sequence of `i64`s, built without integer
/// casts (which would trip the workspace's `cast_possible_wrap` lint).
fn make_values(n: usize) -> Vec<i64> {
    let mut value = 0_i64;
    std::iter::repeat_with(|| {
        value = value.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        value
    })
    .take(n)
    .collect()
}

fn bench_flat() {
    let column = Column::<i64>::from_values(make_values(FLAT_LEN));
    let expected: i64 = column.as_slice().iter().sum();

    println!("\nSum of a `Column<i64>` ({FLAT_LEN} elements):");

    // The fast path: one contiguous, zero-copy slice — no per-element dispatch.
    run("as_slice().iter().sum()", FLAT_LEN, expected, || {
        black_box(&column).as_slice().iter().sum()
    });

    // The typesafe iterator. `sum` routes through the overridden `fold`, which
    // reads each element with `value_unchecked` (no bounds check).
    run("iter().sum()", FLAT_LEN, expected, || {
        black_box(&column).iter().sum()
    });

    #[expect(clippy::unnecessary_fold, reason = "benchmarking the explicit fold")]
    run("iter().fold(0, +)", FLAT_LEN, expected, || {
        black_box(&column)
            .iter()
            .fold(0_i64, |acc, value| acc + value)
    });

    // A plain `for` loop drives `next()` element by element (no `fold`
    // override), isolating the cost of the per-element `next`.
    run("for v in &column (next)", FLAT_LEN, expected, || {
        let mut sum = 0_i64;
        for value in black_box(&column) {
            sum += value;
        }
        sum
    });

    // Bounds-checked element access, for comparison.
    run("value(i) loop (checked)", FLAT_LEN, expected, || {
        let column = black_box(&column);
        let mut sum = 0_i64;
        for i in 0..column.len() {
            sum += column.value(i);
        }
        sum
    });

    run("get(i) loop (checked)", FLAT_LEN, expected, || {
        let column = black_box(&column);
        let mut sum = 0_i64;
        for i in 0..column.len() {
            if let Some(value) = column.get(i) {
                sum += value;
            }
        }
        sum
    });
}

fn bench_list() {
    let rows: Vec<Vec<i64>> = (0..LIST_ROWS)
        .map(|_| make_values(LIST_ITEMS_PER_ROW))
        .collect();
    let total_items = LIST_ROWS * LIST_ITEMS_PER_ROW;
    let column = Column::<List<i64>>::from_values(rows);
    let expected: i64 = column
        .iter()
        .map(|row| row.as_slice().iter().sum::<i64>())
        .sum();

    println!(
        "\nSum of all items of a `Column<List<i64>>` ({LIST_ROWS} rows × {LIST_ITEMS_PER_ROW} items):"
    );

    // Per row, the items are one contiguous slice.
    run("row.as_slice().iter().sum()", total_items, expected, || {
        black_box(&column)
            .iter()
            .map(|row| row.as_slice().iter().sum::<i64>())
            .sum()
    });

    // The `ListValue` iterator (`value_unchecked` per item).
    run("row.iter().sum()", total_items, expected, || {
        black_box(&column)
            .iter()
            .map(|row| row.iter().sum::<i64>())
            .sum()
    });

    // Bounds-checked per-item access.
    run("row.value(i) loop (checked)", total_items, expected, || {
        let mut sum = 0_i64;
        for row in black_box(&column) {
            for i in 0..row.len() {
                sum += row.value(i);
            }
        }
        sum
    });
}

/// Times `f` over enough iterations to be meaningful, checks its result against
/// `expected`, and prints the per-element cost.
fn run(name: &str, elements: usize, expected: i64, mut f: impl FnMut() -> i64) {
    // Warm up (and validate the strategy against the reference sum).
    for _ in 0..4 {
        assert_eq!(f(), expected, "strategy `{name}` produced the wrong sum");
    }

    let runs: u32 = 100;
    let mut checksum = 0_i64;
    let start = Instant::now();
    for _ in 0..runs {
        checksum = checksum.wrapping_add(black_box(f()));
    }
    let elapsed = start.elapsed();
    black_box(checksum);

    let per_run: Duration = elapsed / runs;
    let ns_per_element = per_run.as_nanos() as f64 / elements as f64;
    println!("  {name:<32} {per_run:>10.2?}  ({ns_per_element:.3} ns/element)");
}
