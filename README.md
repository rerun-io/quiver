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
// * `impl TryFrom<RecordBatch> for Thing` - validates the schema, then downcasts (zero-copy)
// * `impl TryFrom<Thing> for RecordBatch` - fails on column length mismatch
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

What is checked when parsing a `RecordBatch`:

|                | Raw `arrow` array                                                            | `quiver::Column<L>`                                              |
|----------------|------------------------------------------------------------------------------|------------------------------------------------------------------|
| Datatype       | Exact for flat arrays; parameterized arrays (`ListArray`, …) are downcast only — *any* inner types | Exact match, recursively (`List<String>` ≠ `List<i64>`)           |
| Nullability    | Not checked                                                                  | Non-`Option` levels must be null-free, at every nesting depth     |
| Timestamps     | Unit checked; the timezone must be `None` (`TimestampNanosecondArray`)       | Unit *and* timezone (`Timestamp<Nanosecond, Utc>`)                |
| Element access | The arrow APIs; manual downcasts for nested data                             | Typed, infallible, and zero-copy (`&str`, `i64`, item iterators)  |
| Cost           | None                                                                         | One eager validation pass at the parse boundary                   |

All validation happens once, when the record batch enters: after that, a `Column<L>` cannot
be invalid (its fields are private and immutable), so element access never returns a `Result`.

Structs whose columns all have a statically-known datatype also get a generated
`fn schema()` with the exact arrow schema, including optional columns.

The supported logical types:

| Logical type `L`                             | Arrow datatype               | Element value             |
|----------------------------------------------|------------------------------|---------------------------|
| `bool`, `i8`–`i64`, `u8`–`u64`, `f32`, `f64` | The same                     | By value                  |
| `String`                                     | `Utf8`                       | `&str`                    |
| `[u8; N]`                                    | `FixedSizeBinary(N)`         | `&[u8; N]`                |
| `Timestamp<Nanosecond, Utc>`                 | `Timestamp(Nanosecond, UTC)` | `i64`                     |
| `Duration<Millisecond>`                      | `Duration(Millisecond)`      | `i64`                     |
| `List<L>`                                    | `List(…)`, recursively       | An iterator over the items |
| `Option<L>`                                  | Nullable at this level       | `Option<…>`               |

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
Work-in-progress

## TODO
* [x] Const-generics-based support for `DataType::FixedSizeBinary(16)` etc: `Column<[u8; 16]>`
* [x] `Timestamp` logical type for `quiver::Column`: `Column<Timestamp<Nanosecond, Utc>>` — unit and timezone are part of the type, matched exactly
* [x] `Duration` logical type for `quiver::Column`: `Column<Duration<Millisecond>>`
* [ ] Add `exhaustive/nonexhaustive` attributes (whether or not extra columns are allowed).
* [ ] Forbid `#[quiver(extra_columns)]` if the `exhaustive` attribute is set
* [ ] Field-level metadata requirements, e.g. `#[quiver(required_metadata("unit"))]`
* [ ] `#[quiver(readonly)]` — invariant-by-construction variant: private fields, read-only accessors, and a validating constructor
* [ ] Publish to crates.io
