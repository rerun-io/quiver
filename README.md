# arrow-quiver

[![Latest version](https://img.shields.io/crates/v/arrow-quiver.svg)](https://crates.io/crates/arrow-quiver)
[![Documentation](https://docs.rs/arrow-quiver/badge.svg)](https://docs.rs/arrow-quiver)
![MIT](https://img.shields.io/badge/license-MIT-blue.svg)
![Apache](https://img.shields.io/badge/license-Apache-blue.svg)

A zero-copy, strongly typed interface for [Apache Arrow](https://arrow.apache.org/) record batches, for Rust's [`arrow-rs`](https://github.com/apache/arrow-rs).

## Example
For a complete, compiling example, see [`example.rs`](crates/arrow-quiver/examples/example.rs).
Run it with `cargo run --example example`.

Use the strongly-typed `quiver::Column<L>` for compile-time guarantees (exact datatype,
including nested types, and nullability), and raw `arrow` types when you _want_ things
to be dynamic:

``` rust
use std::collections::BTreeMap;

use arrow_quiver::arrow::array::ArrayRef;
use arrow_quiver::{Column, DynColumn, List, Quiver};

/// Important thing
#[derive(Quiver)]
struct Thing {
    /// Optional
    #[quiver(metadata)]
    pub metadata: BTreeMap<String, String>,

    /// Strongly typed: guaranteed to be Utf8, with no nulls
    pub name: Column<String>,

    /// Strongly typed: a List<Utf8> where the items may be null
    pub tags: Column<List<Option<String>>>,

    /// Strongly typed values; the whole *column* may be missing
    pub dob: Option<Column<i64>>,

    /// A raw arrow array: any datatype, any nullability — dynamically typed
    pub comment: ArrayRef,

    /// Optional: other, dynamic columns
    #[quiver(extra_columns)]
    pub other_columns: Vec<DynColumn>,
}

// Proc-macro generates:
// * `impl TryFrom<RecordBatch> for Thing` (and `&RecordBatch`) - validates the schema,
//   then downcasts (zero-copy)
// * `impl TryFrom<Thing> for RecordBatch` - fails on column length mismatch
// * `fn from_record_batch()` and `fn into_record_batch()` - discoverable aliases for the above
// * `COLUMN_*` descriptor constants - single-column access without hard-coding names
// * `fn min_schema()`/`fn max_schema()` and `fn empty_record_batch()` - when all columns are statically typed
```

Building columns from values is infallible:

``` rust
use arrow_quiver::{Column, List};

let names: Column<String> = vec!["Alice", "Bob"].into();
let scores = Column::<List<i64>>::from_values([vec![1, 2], vec![3]]);
let maybe: Column<Option<f64>> = [Some(1.5), None].into_iter().collect();
```

Single columns can be extracted without parsing the whole batch — the derive generates
a `COLUMN_*` descriptor per column, so no names are hard-coded:

``` rust
use arrow_quiver::{Column, Quiver};

#[derive(Quiver)]
struct Reading {
    sensor: Column<String>,
}

let batch = Reading {
    sensor: vec!["kitchen".to_owned()].into(),
}
.into_record_batch()
.unwrap();

// Single-column extraction, fully typed:
let sensors = Reading::COLUMN_SENSOR.extract(&batch).unwrap();
assert_eq!(sensors.to_vec(), ["kitchen"]); // `to_vec()` returns owned values
assert_eq!(Reading::COLUMN_SENSOR.name, "sensor");

// Static schema + infallible empty batches (when all columns are statically typed):
let empty = Reading::empty_record_batch(); // all declared columns, zero rows
assert_eq!(empty.num_rows(), 0);
```

`quiver::Column` is also usable standalone, without the derive:

``` rust
use std::sync::Arc;

use arrow_quiver::arrow::array::{ArrayRef, ListArray};
use arrow_quiver::arrow::datatypes::Int32Type;
use arrow_quiver::{Column, List};

let dynamic_arrow_array: ArrayRef = Arc::new(ListArray::from_iter_primitive::<Int32Type, _, _>(
    vec![Some(vec![Some(1), Some(2)]), Some(vec![Some(3)])],
));

let column = Column::<List<Option<i32>>>::try_from(dynamic_arrow_array).unwrap();
for list in &column {
    for number in list {
        // `number` is an `Option<i32>`; validation already happened, up front
    }
}
```

## Quiver types vs. arrow types

A `#[derive(Quiver)]` field can hold its column either as a raw `arrow` array
(e.g. `StringArray`, `ListArray`, `ArrayRef`) or as a strongly-typed `quiver::Column<L>`,
where `L` is a *logical type* like `List<Option<String>>`.
Use quiver types for compile-time guarantees; use arrow types when you _want_ things to be dynamic.

All column matching is done **by name** — column order never matters:
parsing accepts any input column order, and encoding emits the columns
in struct declaration order (with any `extra_columns` appended at the end),
regardless of the order they had when parsed.

What is checked when parsing a `RecordBatch`:

|                | Raw `arrow` array                                                            | `quiver::Column<L>`                                              |
|----------------|------------------------------------------------------------------------------|------------------------------------------------------------------|
| Datatype       | Exact for flat arrays; parameterized arrays (`ListArray`, …) are downcast only — *any* inner types | Structural match, recursively (`List<String>` ≠ `List<i64>`; inner field names/nullability flags/metadata are not compared — actual nulls are what matters) |
| Nullability    | Not checked                                                                  | Non-`Option` levels must be null-free, at every nesting depth     |
| Timestamps     | Unit checked; the timezone must be `None` (`TimestampNanosecondArray`)       | Unit *and* timezone (`Timestamp<Nanosecond, Utc>`)                |
| Element access | The arrow APIs; manual downcasts for nested data                             | Typed, infallible, and zero-copy (`&str`, `i64`, item iterators)  |
| Cost           | None                                                                         | One eager validation pass at the parse boundary                   |

All validation happens once, when the record batch enters: after that, a `Column<L>` cannot
be invalid (its fields are private and immutable), so element access never returns a `Result`.

Structs whose columns all have a statically-known datatype also get generated
`fn min_schema()` (the required columns) and `fn max_schema()` (all declared columns,
including optional ones), plus an infallible `fn empty_record_batch()` —
zero rows, every declared column present (optional ones too: parsing the result back
yields `Some(empty column)`, not `None`).

More of the `Column` API:

* Construction is infallible: `from_values`, `From<Vec<T>>`, `FromIterator`,
  `from_nullable_values` (for e.g. `Option<&str>` → `Option<String>`), and `Default` (empty).
  The one exception: building a `Dictionary` column can fail (key overflow),
  so it uses `try_from_values` instead
* Reading: `value`/`get`, `iter()` (borrowed), `iter_owned()`/`to_vec()` (owned)
* Per-column metadata: `metadata()`/`with_metadata()`, stored on the arrow `Field`
  when converting to/from a record batch. Statically known metadata can be *declared*:
  `#[quiver(metadata("rerun:kind" = "control"))]` — stamped on encode (instance metadata
  wins on key conflicts), included in `schema()`, never validated on parse
* Domain newtypes: `newtype_datatype!(SensorName, String)` makes `Column<SensorName>` work,
  with all of the above
* Interop: `as_arrow()`/`into_arrow()`, and quiver errors convert
  into `arrow::error::ArrowError` (as `ExternalError`), so `?` works in
  functions returning arrow results

The supported logical types:

| Logical type `L`                             | Arrow datatype               | Element value             |
|----------------------------------------------|------------------------------|---------------------------|
| `bool`, `i8`–`i64`, `u8`–`u64`, `f16`–`f64`  | The same                     | By value                  |
| `String`, `LargeUtf8`                        | `Utf8`, `LargeUtf8`          | `&str`                    |
| `[u8; N]`                                    | `FixedSizeBinary(N)`         | `&[u8; N]`                |
| `Binary`, `LargeBinary`                      | `Binary`, `LargeBinary`      | `&[u8]`                   |
| `Date32`, `Date64`                           | `Date32`, `Date64`           | `i32` days / `i64` ms     |
| `Time32Second` … `Time64Nanosecond`          | `Time32(…)`, `Time64(…)`     | `i32` / `i64`             |
| `TimestampNanosecond<Utc>`                   | `Timestamp(Nanosecond, UTC)` | `i64`                     |
| `DurationMillisecond`                        | `Duration(Millisecond)`      | `i64`                     |
| `Dictionary<i32, String>`                    | `Dictionary(Int32, Utf8)`    | Transparent: `&str`       |
| `List<L>`                                    | `List(…)`, recursively       | An iterator over the items |
| `FixedSizeList<f32, 3>`                      | `FixedSizeList(Float32, 3)`  | An iterator over the items |
| `Option<L>`                                  | Nullable at this level       | `Option<…>`               |

Not *yet* supported as logical types:

* `Struct` (parked; investigated 2026-06-04 — moderate effort: a new derive generating
  per-row view/owned/typed mirror structs; the `Datatype` trait needs no changes.
  The one subtle part is hierarchical null masking: when a struct *row* is null,
  arrow leaves the child values undefined, so child null-validation must be masked
  by the parent validity, on both parse and build)
* The string/binary *view* types
* `LargeList`

Most of these can still be used as raw, downcast-only arrow array fields
(`StructArray`, `DictionaryArray`, `LargeListArray`, …).
The difficult and exotic datatypes — `Decimal`, `Map`, `Union`, `Interval`,
and run-end arrays — are explicitly unsupported even as raw fields,
with a clear compile error.

Timezones are matched as exact strings: `Timestamp<Nanosecond, Utc>` ("UTC") will
not accept an array with the equivalent timezone `"+00:00"`.

## Pros & cons

Pros:
* **Zero-copy**: columns stay as reference-counted Arrow arrays (structure-of-arrays), never transposed into `Vec<RowStruct>`
* **Parse, don't validate**: column names, datatypes, and nullability are all checked once, eagerly, at the `TryFrom<RecordBatch>` boundary
* **Strong typing on demand**: `quiver::Column<L>` validates exact datatypes (including the inner types of nested arrays) and nullability, then gives infallible typed access; raw `arrow` types remain available when you _want_ dynamic
* **Struct literal = builder**: plain `pub` fields; no builder machinery, free pattern matching
* **Nothing is hidden**: record batch metadata and unknown columns are explicit fields, declared in the struct
* **Thin**: the derive expands to plain `arrow-rs` calls; no runtime machinery

Cons:
* **Invalid states are representable**: a column length mismatch is only caught when converting *to* a `RecordBatch`, possibly far from the mistake site
* **Fields stay mutable**: a parsed struct can be modified into invalidity after validation (`quiver::Column` itself stays valid — it is immutable after construction)
* **Raw arrow fields are unchecked by design**: nullability and the inner types of nested arrays are only validated for `quiver::Column<L>` fields
* **Column order is not preserved**: matching is by name; re-encoding emits struct declaration order, with `extra_columns` appended at the end — not the input order
* **No per-row view**: data is accessed column-wise (that's the point), but there is no generated row iterator
* **Rust only**: no IDL, no cross-language codegen (so far)

### Prior art (researched 2026-06-04)

#### Rust
| Crate            | Status            | What it does                                                              | Zero-copy `SoA`?                          |
|------------------|-------------------|---------------------------------------------------------------------------|-------------------------------------------|
| `typed-arrow`    | Active (tonbo-io) | `#[derive(Record)]` on *logical* types → builders, schema, lazy row views | **Yes** (`views` feature)                 |
| `arrow_convert`  | Active            | serde-style derive, Rust types ↔ Arrow arrays                             | No — transposes + copies into `Vec<T>`    |
| `serde_arrow`    | Very active       | `Vec<Struct>` ↔ `RecordBatch` via serde                                   | No — serde data model forces owned values |

`typed-arrow` is the closest match but misses the mold:

1. Positional column matching (index + datatype), not name-based. No `optional` columns, no `other_columns`.
2. Nullability validated lazily per-row, not eagerly at the parse boundary.
3. No metadata schema validation at all.
4. Schema declared as Rust *logical* types (`String`, `i64`); generates builder machinery we don't need. Our derive goes directly on *array* types (`StringArray`) — simpler, inherently zero-copy.


## Crates

* [`arrow-quiver`](crates/arrow-quiver) — the runtime crate: `Column<L>`, `DynColumn`, `Error`, and the `arrow` re-export
* [`arrow-quiver-derive`](crates/arrow-quiver-derive) — the `#[derive(Quiver)]` proc-macro

## License

Dual-licensed under [MIT](LICENSE-MIT) and [Apache 2.0](LICENSE-APACHE).


## Status
Work-in-progress.

⚠️ Most of the code in this repository was generated by an LLM (under human direction and review). Read it with the appropriate skepticism.

## TODO
* [ ] `#[quiver(flatten)]` — struct composition (parked; evaluated 2026-06-04: feasible, no
  stable-Rust blockers, ~2–3 sessions — the biggest derive feature so far). Spec highlights:
  a doc-hidden `QuiverRecord` trait (`COLUMN_NAMES`, `partial_from_record_batch`,
  `push_columns`) that the existing generated fns become wrappers over; flattened columns at
  the flatten field's position; outer owns strictness; const-assert that the inner has no
  `extra_columns`/`metadata` field; compile-time name-collision detection via const eval.
  One spec amendment needed: `min_fields`/`max_fields` must live in a *separate* trait
  implemented only for statically-typed structs (a mandatory method would force a lying
  impl or runtime panic when flattening a dynamic inner). First step when picked up:
  the `QuiverRecord` refactor, which is independently valuable.
* [ ] Look for TODOs
