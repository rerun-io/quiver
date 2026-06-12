//! Soundness tests for the `unsafe` iteration paths
//! ([`LogicalType::value_unchecked`] and the hand-written iterator
//! combinators), exercised over **sliced** arrays so the offset handling is
//! covered. Meant to be run under Miri (`cargo miri test -p quiver_types`),
//! which is why they live here — `quiver_types` holds all the `unsafe`, and its
//! dependency graph (no `trybuild`) is cheap for Miri to build.
//!
//! Each case drives every overridden combinator: forward/back `next`, `nth`,
//! `fold`/`rfold`, `last`, `count`, and a "meet in the middle" mix of `next` /
//! `next_back`. The values are checked against an independently-computed
//! reference so a wrong (but in-bounds) read is caught too, not just UB.

use std::sync::Arc;

use quiver_types::arrow::array::{ArrayRef, LargeListArray};
use quiver_types::arrow::datatypes::Int64Type;
use quiver_types::{AnyList, Column, Dictionary, FixedSizeBinary, List, Utf8};

/// Drive a fresh iterator (via `iter`, which doesn't consume) through every
/// overridden combinator and check each against `expected`.
fn check_iter<'a, L>(column: &'a Column<L>, expected: &[L::Value<'a>])
where
    L: quiver_types::LogicalType + 'a,
    L::Value<'a>: PartialEq + Clone + std::fmt::Debug,
{
    // Forward.
    assert_eq!(column.iter().collect::<Vec<_>>(), expected, "forward");

    // Reverse (`next_back`).
    let mut reversed = expected.to_vec();
    reversed.reverse();
    assert_eq!(column.iter().rev().collect::<Vec<_>>(), reversed, "reverse");

    // `count` / `last` / `size_hint`.
    assert_eq!(column.iter().count(), expected.len(), "count");
    assert_eq!(column.iter().last(), expected.last().copied_ref(), "last");
    assert_eq!(
        column.iter().size_hint(),
        (expected.len(), Some(expected.len())),
        "size_hint"
    );

    // `nth` from every starting offset, plus one past the end.
    for n in 0..=expected.len() {
        assert_eq!(
            column.iter().nth(n),
            expected.get(n).copied_ref(),
            "nth({n})"
        );
    }

    // Meet in the middle: alternate `next` and `next_back`.
    let mut it = column.iter();
    let mut front = 0;
    let mut back = expected.len();
    let mut take_front = true;
    while front < back {
        if take_front {
            assert_eq!(it.next(), Some(&expected[front]).copied_ref(), "mid next");
            front += 1;
        } else {
            back -= 1;
            assert_eq!(
                it.next_back(),
                Some(&expected[back]).copied_ref(),
                "mid back"
            );
        }
        take_front = !take_front;
    }
    assert_eq!(it.next(), None, "drained next");
    assert_eq!(it.next_back(), None, "drained back");
}

/// Tiny helper so `check_iter` can compare against `Option<&T>` regardless of
/// whether `Value` is a reference or a copy type.
trait CopiedRef<T> {
    fn copied_ref(self) -> Option<T>;
}
impl<T: Clone> CopiedRef<T> for Option<&T> {
    fn copied_ref(self) -> Option<T> {
        self.cloned()
    }
}

#[test]
fn primitive_column_sliced() {
    let column = Column::<i64>::from_values([0, 1, 2, 3, 4, 5, 6, 7]);

    check_iter(&column, &[0, 1, 2, 3, 4, 5, 6, 7]);
    check_iter(&column.slice(2, 4), &[2, 3, 4, 5]);
    check_iter(&column.slice(7, 1), &[7]);
    check_iter(&column.slice(8, 0), &[]);

    // `fold` / `rfold` (sum is order-independent, but exercises both paths).
    let sliced = column.slice(2, 4);
    assert_eq!(sliced.iter().sum::<i64>(), 2 + 3 + 4 + 5);
    assert_eq!(sliced.iter().rev().sum::<i64>(), 2 + 3 + 4 + 5);

    // `into_iter` (owned) — forward, `nth`, reverse.
    assert_eq!(
        column.slice(2, 4).into_iter().collect::<Vec<_>>(),
        [2, 3, 4, 5]
    );
    assert_eq!(column.slice(2, 4).into_iter().nth(2), Some(4));
    assert_eq!(
        column.slice(2, 4).into_iter().rev().collect::<Vec<_>>(),
        [5, 4, 3, 2]
    );
}

#[test]
fn string_column_sliced() {
    // Variable-length: exercises the offset-buffer reads in `value_unchecked`.
    let column = Column::<Utf8>::from_values(["a", "bb", "ccc", "dddd", "eeeee"]);

    check_iter(&column, &["a", "bb", "ccc", "dddd", "eeeee"]);
    check_iter(&column.slice(1, 3), &["bb", "ccc", "dddd"]);
    check_iter(&column.slice(4, 1), &["eeeee"]);
}

#[test]
fn nullable_column_sliced() {
    // Exercises `Option::value_unchecked`'s per-element `is_null` branch.
    let column =
        Column::<Option<i64>>::from_values([Some(0), None, Some(2), None, Some(4), Some(5)]);

    check_iter(&column, &[Some(0), None, Some(2), None, Some(4), Some(5)]);
    check_iter(&column.slice(1, 4), &[None, Some(2), None, Some(4)]);
}

#[test]
fn nullable_string_column_sliced() {
    // Like `nullable_column_sliced`, but over a variable-length (byte-buffer)
    // encoding, so `Option::value_unchecked`'s `is_null_unchecked` probe runs
    // against a sliced validity bitmap on a non-primitive leaf.
    let column = Column::<Option<Utf8>>::from_values([
        Some("a".to_owned()),
        None,
        Some("ccc".to_owned()),
        None,
        Some("eeeee".to_owned()),
        Some("f".to_owned()),
    ]);

    check_iter(
        &column,
        &[Some("a"), None, Some("ccc"), None, Some("eeeee"), Some("f")],
    );
    check_iter(
        &column.slice(1, 4),
        &[None, Some("ccc"), None, Some("eeeee")],
    );
}

#[test]
fn any_list_column_sliced() {
    // Exercises `AnyList::value_unchecked` (added so iteration skips the
    // per-row bounds check) over a sliced `LargeList` encoding.
    let rows = [
        Some(vec![Some(0), Some(1), Some(2)]),
        Some(vec![]),
        Some(vec![Some(3), Some(4)]),
        Some(vec![Some(5), Some(6), Some(7), Some(8)]),
        Some(vec![Some(9)]),
    ];
    let array = LargeListArray::from_iter_primitive::<Int64Type, _, _>(rows);
    let column = Column::<AnyList<i64>>::try_from(Arc::new(array) as ArrayRef)
        .expect("a LargeList of i64 parses as AnyList<i64>");

    // Forward, over a row-sliced column.
    let rows: Vec<Vec<i64>> = column
        .slice(2, 2)
        .iter()
        .map(|row| row.iter().collect())
        .collect();
    assert_eq!(rows, [vec![3, 4], vec![5, 6, 7, 8]]);

    // Reverse iteration of the row column (drives `next_back`).
    let rows_rev: Vec<Vec<i64>> = column.iter().rev().map(|row| row.to_vec()).collect();
    assert_eq!(
        rows_rev,
        [vec![9], vec![5, 6, 7, 8], vec![3, 4], vec![], vec![0, 1, 2]]
    );

    // Random access and `nth` over the sliced row column.
    assert_eq!(column.slice(2, 2).value(1).to_vec(), vec![5, 6, 7, 8]);
    assert_eq!(
        column.iter().nth(3).map(|row| row.to_vec()),
        Some(vec![5, 6, 7, 8])
    );
}

#[test]
fn nullable_any_list_column_sliced() {
    // Exercises `AnyList::is_null_unchecked` (the null-row probe) via
    // `Option<AnyList<…>>`, over a sliced `LargeList` with null rows.
    let rows = [
        Some(vec![Some(0), Some(1)]),
        None,
        Some(vec![Some(2)]),
        None,
        Some(vec![Some(3), Some(4), Some(5)]),
    ];
    let array = LargeListArray::from_iter_primitive::<Int64Type, _, _>(rows);
    let column = Column::<Option<AnyList<i64>>>::try_from(Arc::new(array) as ArrayRef)
        .expect("a nullable LargeList of i64 parses as Option<AnyList<i64>>");

    let collect = |column: &Column<Option<AnyList<i64>>>| -> Vec<Option<Vec<i64>>> {
        column
            .iter()
            .map(|row| row.map(|items| items.iter().collect()))
            .collect()
    };

    assert_eq!(
        collect(&column),
        [
            Some(vec![0, 1]),
            None,
            Some(vec![2]),
            None,
            Some(vec![3, 4, 5]),
        ]
    );
    assert_eq!(collect(&column.slice(1, 3)), [None, Some(vec![2]), None]);
}

#[test]
fn fixed_size_binary_column_sliced() {
    // Exercises the `first_chunk::<N>` path in `value_unchecked`.
    let column = Column::<FixedSizeBinary<2>>::from_values([[0, 1], [2, 3], [4, 5], [6, 7]]);

    check_iter(&column, &[&[0, 1], &[2, 3], &[4, 5], &[6, 7]]);
    check_iter(&column.slice(1, 2), &[&[2, 3], &[4, 5]]);
}

#[test]
fn dictionary_column_sliced() {
    // Exercises the dictionary key path: `keys().value_unchecked(i)` then a
    // bounds-checked value lookup.
    let column = Column::<Dictionary<i32, Utf8>>::try_from_values(["x", "y", "x", "z", "y"])
        .expect("dictionary fits an i32 key");

    check_iter(&column, &["x", "y", "x", "z", "y"]);
    check_iter(&column.slice(1, 3), &["y", "x", "z"]);
}

#[test]
fn list_column_and_list_value_sliced() {
    // Exercises both the list `value_unchecked` (`offsets.get_unchecked(i)` /
    // `(i + 1)`) and the `ListValue` iterator over a sliced row.
    let column = Column::<List<i64>>::from_values([
        vec![0, 1, 2],
        vec![],
        vec![3, 4],
        vec![5, 6, 7, 8],
        vec![9],
    ]);

    // The list column itself, sliced by row.
    let rows: Vec<Vec<i64>> = column
        .slice(2, 2)
        .iter()
        .map(|row| row.iter().collect())
        .collect();
    assert_eq!(rows, [vec![3, 4], vec![5, 6, 7, 8]]);

    // Reverse iteration of the row column.
    let rows_rev: Vec<Vec<i64>> = column.iter().rev().map(|row| row.to_vec()).collect();
    assert_eq!(
        rows_rev,
        [vec![9], vec![5, 6, 7, 8], vec![3, 4], vec![], vec![0, 1, 2]]
    );

    // Drive every `ListValue` combinator on a multi-item row.
    let row = column.value(3); // [5, 6, 7, 8]
    assert_eq!(row.len(), 4);
    assert_eq!(row.iter().collect::<Vec<_>>(), [5, 6, 7, 8]);
    assert_eq!(row.iter().rev().collect::<Vec<_>>(), [8, 7, 6, 5]);
    assert_eq!(row.iter().nth(2), Some(7));
    assert_eq!(row.iter().nth(4), None);
    assert_eq!(row.iter().rev().nth(1), Some(7)); // nth_back
    assert_eq!(row.iter().last(), Some(8));
    assert_eq!(row.iter().count(), 4);
    assert_eq!(row.iter().sum::<i64>(), 5 + 6 + 7 + 8); // fold
    assert_eq!(row.iter().rev().sum::<i64>(), 5 + 6 + 7 + 8); // rfold
    assert_eq!(row.as_slice(), &[5, 6, 7, 8]);

    // Meet in the middle within one row.
    let mut it = row.iter();
    assert_eq!(it.next(), Some(5));
    assert_eq!(it.next_back(), Some(8));
    assert_eq!(it.next(), Some(6));
    assert_eq!(it.next_back(), Some(7));
    assert_eq!(it.next(), None);
    assert_eq!(it.next_back(), None);
}
