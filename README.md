# quiver

[![Latest version](https://img.shields.io/crates/v/quiver.svg)](https://crates.io/crates/quiver)
[![Documentation](https://docs.rs/quiver/badge.svg)](https://docs.rs/quiver)
![MIT](https://img.shields.io/badge/license-MIT-blue.svg)
![Apache](https://img.shields.io/badge/license-Apache-blue.svg)

A zero-copy, strongly typed interface for [Apache Arrow](https://arrow.apache.org/) columns and record batches, for Rust's [`arrow-rs`](https://github.com/apache/arrow-rs).

## What
[`arrow-rs`](https://github.com/apache/arrow-rs) is to a large extent dynamically typed.
For instance, you cannot know until runtime if an [`arrow::ListArray`](https://docs.rs/arrow/latest/arrow/array/type.ListArray.html) will contain strings or numbers, and whether or not the values in it can be `null`.

`quiver` provides strongly typed (and zero-copy) wrappers around these arrays, with compile-time guarantees that are checked only once, during the construction of the columns. For instance, `quiver::Column<quiver::List<Utf8>>` is a `ListArray` that is guaranteed to contain strings, with no nulls.

Additionally, `quiver` provides a proc-macro for easily converting a `struct` of many arrays to and from arrow `RecordBatch`es (needs the `derive` feature to be enabled).

A struct marked with `#[derive(Quiver)]` can contain either dynamically typed arrow arrays (`ArrayRef`, `ListArray`, …) or strongly typed `quiver` types (or a mix of both!).

## The main types

* [`Column<L>`](https://docs.rs/quiver/latest/quiver/struct.Column.html):
  one column of a record batch, i.e. an arrow array validated against the logical type `L`,
  plus that column's field metadata.
* [`TypedArray<L>`](https://docs.rs/quiver/latest/quiver/struct.TypedArray.html):
  the data half of a `Column<L>`, i.e. the same validated array with the same value API,
  minus the metadata. A `Column<L>` is a `TypedArray<L>` plus metadata; use a `TypedArray<L>`
  directly for arrays that aren't record batch columns.
* [`ColumnDesc<L>`](https://docs.rs/quiver/latest/quiver/struct.ColumnDesc.html):
  a named handle on one column, holding no data. It knows both the column name and `L`,
  so it can `extract` a `Column<L>` from a record batch, or validate a loose `ArrayRef`
  into a `TypedArray<L>`, without you naming either at the call site.
  `#[derive(Quiver)]` generates one per field as a `COLUMN_*` constant, and `ColumnDesc::new`
  is a `const fn`, so you can just as well declare your own.

Dynamically typed columns get the same pair without the `L`:
[`DynColumn`](https://docs.rs/quiver/latest/quiver/struct.DynColumn.html) and
[`DynColumnDesc`](https://docs.rs/quiver/latest/quiver/struct.DynColumnDesc.html).

## Supported `arrow` versions

| `quiver`    | `arrow` |
|-------------|---------|
| `main`      | 58      |
| 0.4 – 0.6   | 58      |
| 0.2 – 0.3   | 57 – 59 |
| 0.1         | 57 – 58 |

## Example
For a complete, compiling example, see [`example.rs`](crates/quiver/examples/example.rs).


``` rust
use std::collections::BTreeMap;

use quiver::arrow::array::ArrayRef;
use quiver::{Column, DynColumn, List, Quiver, Utf8};

/// Important thing
#[derive(Quiver)]
struct Thing {
    /// Optional
    #[quiver(metadata)]
    pub metadata: BTreeMap<String, String>,

    /// Strongly typed: guaranteed to be Utf8, with no nulls
    pub name: Column<Utf8>,

    /// Strongly typed: a List<Utf8> where the items may be null
    pub tags: Column<List<Option<Utf8>>>,

    /// The column name defaults to the field name;
    /// override it when it isn't a valid Rust identifier:
    #[quiver(name = "special:kind")]
    pub kind: Column<Utf8>,

    /// Strongly typed values; the whole *column* may be missing
    pub dob: Option<Column<i64>>,

    /// A raw arrow array: any data type, any nullability — dynamically typed
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
// * `fn min_schema()`/`fn max_schema()` - when all columns are statically typed
// * `fn empty_record_batch()` - when, additionally, all columns are required (min == max)
```

Building columns from values is infallible.
Every column has a name.
A column without a name is just an array.

``` rust
use quiver::{Column, List, TypedArray, Utf8};

let names = Column::<Utf8>::from_values("name", ["Alice", "Bob"]);
let scores = Column::<List<i64>>::from_values("scores", [vec![1, 2], vec![3]]);
let maybe = Column::<Option<f64>>::from_values("weight", [Some(1.5), None]);

// `TypedArray` is the unnamed data half, and has the `Default`, `From<Vec<_>>`,
// `FromIterator`, and `TryFrom<ArrayRef>` impls that cannot name a column:
let unnamed: TypedArray<Utf8> = vec!["Alice", "Bob"].into();
let named = Column::new("name", unnamed);
```

Single columns can be extracted without parsing the whole batch — two ways:

``` rust
use quiver::{Column, Quiver, Utf8};

#[derive(Quiver)]
struct Reading {
    sensor: Column<Utf8>,
}

let batch = Reading {
    sensor: Reading::COLUMN_SENSOR.new_from_values(["kitchen"]),
}
.into_record_batch()?;

// 1. With the derive's `COLUMN_*` descriptor — no column name hard-coded,
//    and errors carry the struct + column name for free:
let sensors = Reading::COLUMN_SENSOR.extract(&batch)?;
assert_eq!(sensors.to_vec(), ["kitchen"]); // `to_vec()` returns owned values
assert_eq!(Reading::COLUMN_SENSOR.name, "sensor");

// 2. Without the derive — by name. A missing column gives a helpful
//    `MissingColumn` error; the data type and nullability are validated too:
let sensors = Column::<Utf8>::from_record_batch_and_name(&batch, "sensor")?;
assert_eq!(sensors.to_vec(), ["kitchen"]);

// Static schema + infallible empty batches
// (when all columns are statically typed and required):
let empty = Reading::empty_record_batch(); // all declared columns, zero rows
assert_eq!(empty.num_rows(), 0);
# Ok::<(), Box<dyn std::error::Error>>(())
```

`quiver::Column` is also usable standalone, without the derive:

``` rust
use std::sync::Arc;

use quiver::arrow::array::{ArrayRef, ListArray};
use quiver::arrow::datatypes::Int32Type;
use quiver::{Column, List, Utf8};

let dynamic_arrow_array: ArrayRef = Arc::new(ListArray::from_iter_primitive::<Int32Type, _, _>(
    vec![Some(vec![Some(1), Some(2)]), Some(vec![Some(3)])],
));

let column = Column::<List<Option<i32>>>::try_new("scores", dynamic_arrow_array)?;
for list in &column {
    for number in list {
        // `number` is an `Option<i32>`; validation already happened, up front
    }
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

For an array that isn't a record batch column, and so has neither a name nor field
metadata to carry, use `quiver::TypedArray<L>`: the same validated, zero-copy view,
with the same value API. A `Column<L>` is a `TypedArray<L>` plus the column's name and
per-column metadata (`Column::new` names one; `column.as_typed_array()` and
`column.into_typed_array()` get you the data half back).

## Quiver types vs. arrow types

A `#[derive(Quiver)]` field can hold its column either as a raw `arrow` array
(e.g. `StringArray`, `ListArray`, `ArrayRef`) or as a strongly-typed `quiver::Column<L>`,
where `L` is a *logical type* like `List<Option<Utf8>>`.
Use quiver types for compile-time guarantees; use arrow types when you _want_ things to be dynamic.

All column matching is done **by name** — column order never matters:
parsing accepts any input column order, and encoding emits the columns
in struct declaration order (with any `extra_columns` appended at the end),
regardless of the order they had when parsed.
The column name defaults to the field name; `#[quiver(name = "special:kind")]`
overrides it, e.g. for column names that aren't valid Rust identifiers.

What is checked when parsing a `RecordBatch`:

|                | Raw `arrow` array                                                            | `quiver::Column<L>`                                              |
|----------------|------------------------------------------------------------------------------|------------------------------------------------------------------|
| Data type      | Exact for flat arrays; parameterized arrays (`ListArray`, …) are downcast only — *any* inner types | Structural match, recursively (`List<Utf8>` ≠ `List<i64>`; inner field names/nullability flags/metadata are not compared — actual nulls are what matters) |
| Nullability    | Not checked                                                                  | Non-`Option` levels must be null-free, at every nesting depth     |
| Timestamps     | Unit checked; the timezone must be `None` (`TimestampNanosecondArray`)       | Unit *and* timezone (`Timestamp<Nanosecond, Utc>`)                |
| Element access | The arrow APIs; manual downcasts for nested data                             | Typed, infallible, and zero-copy (`&str`, `i64`, item iterators)  |
| Cost           | None                                                                         | One eager validation at the parse boundary; cheap (see below)     |

All validation happens once, when the record batch enters: after that, a `Column<L>` cannot
be invalid (its fields are private and immutable), so element access never returns a `Result`.

The validation is cheap — the values themselves are never read.
It compares data types (proportional to schema depth, not row count) and checks
null counts, which arrow caches, so the cost is O(1) per nesting level.
The one exception: when a non-`Option` nesting level (e.g. the items of a `List<Utf8>`)
sits on an inner array that carries a null buffer, quiver counts only the nulls
*reachable* through valid rows, which scans that validity bitmap —
still independent of the value bytes.

Structs whose columns all have a statically-known data type also get generated
`fn min_schema()` (the required columns) and `fn max_schema()` (all declared columns,
including optional ones).
When additionally every column is required (`min_schema() == max_schema()`),
an infallible `fn empty_record_batch()` is generated too — zero rows, every column present.
Structs with optional (`Option<…>`) columns don't get it: there would be no single
obvious empty batch, and a round-trip would silently turn `None` into `Some(empty)`.

More of the `Column` API:

* Construction is infallible, and always names the column: `from_values`,
  `from_nullable_values` (for e.g. `Option<&str>` → `Option<String>`), `empty`,
  `new_null` (all nulls, e.g. to pad a batch that is missing the column), and
  `Column::new` (naming a `TypedArray`). The exceptions: building a `Dictionary`
  (key overflow) or `Run` (run-end overflow) column can fail, so those use
  `try_from_values` instead. The `COLUMN_*` descriptors have `new_from_values`
  and `new_null` too, taking the name and declared metadata from the descriptor
* The name: `name()`/`with_name()`, stored on the arrow `Field` alongside the
  metadata. `TypedArray<L>` is the same data with neither
* Single-column extraction from a `RecordBatch`, no derive needed:
  `Column::<L>::from_record_batch_and_name(&batch, name)` — looks the column up by
  name (a missing one gives a helpful `MissingColumn` error), validates it against `L`,
  and carries over the field metadata. The `COLUMN_*` descriptors do the same without
  hard-coding the name
* Reading: `value/get`, `iter()` (borrowed), `value_owned/iter_owned/to_vec` (owned)
* Bulk zero-copy reads: `as_slice()` — `&[f32]`, `&[[u8; 16]]`, … — for primitive
  and fixed-size binary non-nullable columns. Those columns also `Deref`/`AsRef`
  to the slice, so `&column` is a drop-in for an arrow `ScalarBuffer`
* Bulk writes, for the same columns: `from_slice()` is the inverse of `as_slice()`,
  copying the values in one `memcpy` instead of walking them through an arrow
  builder, and `from_buffer()` wraps an arrow `Buffer` zero-copy
* Per-column metadata: `metadata()`/`with_metadata()`, stored on the arrow `Field`
  when converting to/from a record batch. Statically known metadata can be *declared*:
  `#[quiver(metadata("sorted" = "true"))]` — stamped on encode (instance metadata
  wins on key conflicts), included in `min_schema()`/`max_schema()`, never validated on parse
* Domain newtypes: `newtype_data_type!(SensorName, Utf8)` makes `Column<SensorName>` work,
  with all of the above; for *foreign* types (orphan rule), use the `As` adapter:
  `Column<As<Ipv4Addr, u32>>`. The `Transparent` adapter tags a column with a domain
  type without converting anything: `Column<Transparent<Even, i64>>` reads as plain
  `i64`, so a type that is only `TryFrom` its representation costs no up-front validation
* Interop: `as_arrow()`/`into_arrow()`, and quiver errors convert
  into `arrow::error::ArrowError` (as `ExternalError`), so `?` works in
  functions returning arrow results

The supported logical types:

| Logical type `L`                             | Arrow data type              | Element value             |
|----------------------------------------------|------------------------------|---------------------------|
| `bool`, `i8`–`i64`, `u8`–`u64`, `f16`–`f64`  | The same                     | By value                  |
| `Utf8`, `LargeUtf8`, `Utf8View`              | The same                     | `&str`                    |
| `AnyUtf8`                                    | *any* UTF-8 encoding above   | `&str`                    |
| `FixedSizeBinary<N>`                         | `FixedSizeBinary(N)`         | `&[u8; N]`                |
| `Binary`, `LargeBinary`, `BinaryView`        | The same                     | `&[u8]`                   |
| `AnyBinary`                                  | *any* binary encoding (incl. `FixedSizeBinary`) | `&[u8]`        |
| `Date32`, `Date64`                           | `Date32`, `Date64`           | `i32` days / `i64` ms     |
| `Time32Second` … `Time64Nanosecond`          | `Time32(…)`, `Time64(…)`     | `i32` / `i64`             |
| `TimestampNanosecond<Utc>`                   | `Timestamp(Nanosecond, UTC)` | `i64`                     |
| `DurationMillisecond`                        | `Duration(Millisecond)`      | `i64`                     |
| `Dictionary<i32, Utf8>`                      | `Dictionary(Int32, Utf8)`    | Transparent: `&str`       |
| `Run<i32, Utf8>`                             | `RunEndEncoded(Int32, Utf8)` | Transparent: `&str`       |
| `List<L>`, `LargeList<L>`                    | `List(…)`/`LargeList(…)`, recursively | An iterator over the items |
| `ListView<L>`, `LargeListView<L>`            | `ListView(…)`/`LargeListView(…)`, recursively | An iterator over the items |
| `FixedSizeList<f32, 3>`                      | `FixedSizeList(Float32, 3)`  | An iterator over the items |
| `AnyList<L>`                                 | *any* list encoding above    | An iterator over the items |
| `Map<K, V>`                                  | `Map(…)`, recursively        | An iterator over `(key, value)` pairs |
| `Option<L>`                                  | Nullable at this level       | `Option<…>`               |

### Semi-dynamic logical types

Arrow has five physically different ways to store the same logical thing — a
column of lists of `L`: `List`, `LargeList`, `ListView`, `LargeListView`, and
`FixedSizeList`. `AnyList<L>` is a quiver-only logical type (no single arrow
data type of its own) that **accepts whichever of those a column happens to use**
and reads them all uniformly — handy when the encoding is decided at runtime
(e.g. data from an external source).

```rust
# use std::sync::Arc;
# use quiver::arrow::array::{ArrayRef, LargeListArray};
# use quiver::arrow::datatypes::Int64Type;
use quiver::{AnyList, Column};

# let array: ArrayRef = Arc::new(LargeListArray::from_iter_primitive::<Int64Type, _, _>(
#     vec![Some(vec![Some(1), Some(2)])],
# ));
// `array` may be a List / LargeList / ListView / LargeListView / FixedSizeList:
let column = Column::<AnyList<i64>>::try_new("scores", array)?;
for list in &column {
    for _item in list { /* i64 */ }
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

Because it has no single arrow data type, `AnyList` is **parse-only**: it implements
`LogicalType` (so `try_new`/reading work) but not `ConcreteType`, so it has no
`data_type()`, `from_values`, `empty()`, or schema generation. To *build* a column,
pick a concrete encoding such as `Column<List<L>>`.

`AnyBinary` is the same idea for byte strings: it accepts any of `Binary`,
`LargeBinary`, `BinaryView`, or `FixedSizeBinary` (any size) and reads them all
as `&[u8]`. `AnyUtf8` likewise accepts any of `Utf8`, `LargeUtf8`, or `Utf8View`
and reads them as `&str`. Both are also parse-only.

### What is *not* supported

These data types have no logical type yet, so there is no `Column<L>` for them:

* `Struct` — but usable as a raw, downcast-only `arrow` field (`StructArray`).
  (Parked; investigated 2026-06-04 — moderate effort: a new derive generating
  per-row view/owned/typed mirror structs; the `LogicalType` trait needs no changes.
  The one subtle part is hierarchical null masking: when a struct *row* is null,
  arrow leaves the child values undefined, so child null-validation must be masked
  by the parent validity, on both parse and build.)
* `Decimal` (`Decimal32`/`Decimal64`/`Decimal128`/`Decimal256`)
* `Interval` (`IntervalDayTime`/`IntervalMonthDayNano`/`IntervalYearMonth`)
* `Union`

Everything except `Struct` is rejected with a clear compile error even as a raw
`arrow` field; `Struct` is the one that still works as a raw downcast-only field.

Timezones are matched as exact strings: `Timestamp<Nanosecond, Utc>` ("UTC") will
not accept an array with the equivalent timezone `"+00:00"`.

## Pros & cons

Pros:
* **Zero-copy**: columns stay as reference-counted Arrow arrays (structure-of-arrays), never transposed into `Vec<RowStruct>`
* **Parse, don't validate**: column names, data types, and nullability are all checked once, eagerly, at the `TryFrom<RecordBatch>` boundary
* **Strong typing on demand**: `quiver::Column<L>` validates exact data types (including the inner types of nested arrays) and nullability, then gives infallible typed access; raw `arrow` types remain available when you _want_ dynamic
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

1. Positional column matching (index + data type), not name-based. No `optional` columns, no `other_columns`.
2. Nullability validated lazily per-row, not eagerly at the parse boundary.
3. No metadata schema validation at all.
4. Schema declared as Rust *logical* types (`String`, `i64`); generates builder machinery we don't need. Our derive goes directly on *array* types (`StringArray`) — simpler, inherently zero-copy.


## Crates

* [`quiver`](crates/quiver) — the runtime crate: `Column<L>`, `DynColumn`, `Error`, and the `arrow` re-export
* [`quiver_derive`](crates/quiver_derive) — the `#[derive(Quiver)]` proc-macro

## License

Dual-licensed under [MIT](LICENSE-MIT) and [Apache 2.0](LICENSE-APACHE).


## Status
Ready for production.

⚠️ Most of the code in this repository was generated by an LLM (under human direction and review). Read it with the appropriate skepticism.

## Future work
### `#[quiver(flatten)]`
struct composition (parked; evaluated 2026-06-04: feasible, no
stable-Rust blockers, ~2–3 sessions — the biggest derive feature so far). Spec highlights:
a doc-hidden `QuiverRecord` trait (`COLUMN_NAMES`, `partial_from_record_batch`,
`push_columns`) that the existing generated fns become wrappers over; flattened columns at
the flatten field's position; outer owns strictness; const-assert that the inner has no
`extra_columns`/`metadata` field; compile-time name-collision detection via const eval.
One spec amendment needed: `min_fields`/`max_fields` must live in a *separate* trait
implemented only for statically-typed structs (a mandatory method would force a lying
impl or runtime panic when flattening a dynamic inner). First step when picked up:
the `QuiverRecord` refactor, which is independently valuable.
