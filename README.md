# arrow-quiver

[![Latest version](https://img.shields.io/crates/v/arrow-quiver.svg)](https://crates.io/crates/arrow-quiver)
[![Documentation](https://docs.rs/arrow-quiver/badge.svg)](https://docs.rs/arrow-quiver)
![MIT](https://img.shields.io/badge/license-MIT-blue.svg)
![Apache](https://img.shields.io/badge/license-Apache-blue.svg)

A schema specification and validator for [Apache Arrow](https://arrow.apache.org/) record batches,
with codegen for integrating with [`arrow-rs`](https://github.com/apache/arrow-rs).

A quiver holds arrows; this crate holds typed Arrow arrays.

## Status
Work-in-progress

## TODO
* [x] Rust workspace with `arrow-quiver` and `arrow-quiver-derive` crates
* [x] `#[derive(Quiver)]` with `TryFrom<RecordBatch>` (validate + zero-copy downcast) and `TryFrom<Self> for RecordBatch`
* [x] Typed array columns with eager datatype validation (`StringArray`, primitives, binary, dates, timestamps)
* [x] `Option<…>` for columns that are allowed to be missing
* [x] `#[quiver(non_null)]` — eager `null_count == 0` check at the parse boundary
* [x] `ArrayRef` columns accepting any datatype
* [x] `#[quiver(name = "special:name")]` for column names that aren't Rust identifiers
* [x] `#[quiver(metadata)]` and `#[quiver(extra_columns)]` (absence ⇒ unknown columns are an error)
* [x] More datatypes: `Duration`, `Time`, `Float16`, string/binary views (exact match), plus `List`, `FixedSizeList`, `Struct`, `Dictionary` (downcast-only — inner types not validated)
* [x] Explicitly punt on difficult and exotic datatypes: `Decimal`, `Map`, `Union`, `Interval`, run-ends, … (clear compile error)
* [ ] Validate inner types of nested arrays (`List`, `Struct`, …)
* [ ] Field-level metadata requirements, e.g. `#[quiver(required_metadata("unit"))]`
* [x] Compile-fail tests of the derive macro (`trybuild`)
* [ ] `#[quiver(readonly)]` — invariant-by-construction variant (see `plan.md`)
* [x] Test error messages (should be helpful and actionable, and mention the struct type by name)
* [ ] Publish to crates.io

## Example
For a complete, compiling example, see [`example.rs`](crates/arrow-quiver/examples/example.rs).
Run it with `cargo run --example example`.

``` rust
/// Important thing
#[derive(arrow_quiver::Quiver)]
struct Thing {
    /// …of the record-batch
    #[quiver(metadata)]
    pub metadata: BTreeMap<String, String>,

    /// Name
    #[quiver(non_null)]
    pub name: StringArray,

    /// Date of birth
    pub dob: Option<TimestampNanosecondArray>,

    /// If missing, the proc-macro enforces no additional columns may exist
    #[quiver(extra_columns)]
    pub other_columns: Vec<Column>,
}

// Proc-macro generates:
// * `impl TryFrom<RecordBatch> for Thing` - validates the schema, then downcasts (zero-copy)
// * `impl TryFrom<Thing> for RecordBatch` - fails on column length mismatch
```

## Pros & cons

Pros:
* **Zero-copy**: columns stay as reference-counted Arrow arrays (structure-of-arrays), never transposed into `Vec<RowStruct>`
* **Parse, don't validate**: column names, datatypes, and nullability are all checked once, eagerly, at the `TryFrom<RecordBatch>` boundary
* **Struct literal = builder**: plain `pub` fields; no builder machinery, free pattern matching
* **Nothing is hidden**: record batch metadata and unknown columns are explicit fields, declared in the struct
* **Thin**: the derive expands to plain `arrow-rs` calls; no runtime machinery

Cons:
* **Invalid states are representable**: a column length mismatch is only caught when converting *to* a `RecordBatch`, possibly far from the mistake site
* **Fields stay mutable**: a parsed struct can be modified into invalidity after validation
* **Schema = Rust array types**: limited to the datatypes with a typed Arrow array (`List`/`Struct`/`Dictionary` are validated by downcast only — their inner types are not checked), and no per-row view
* **Nullability at runtime**: since we use the datatypes from `arrow-rs` there is no way to enforce that a column has no nulls
* **Rust only**: no IDL, no cross-language codegen (so far)

### Prior art (researched 2026-06-04)

#### Rust
| Crate            | Status            | What it does                                                              | Zero-copy SoA?                            |
|------------------|-------------------|---------------------------------------------------------------------------|-------------------------------------------|
| `typed-arrow`    | Active (tonbo-io) | `#[derive(Record)]` on *logical* types → builders, schema, lazy row views | **Yes** (`views` feature)                  |
| `arrow_convert`  | Active            | serde-style derive, Rust types ↔ Arrow arrays                             | No — transposes + copies into `Vec<T>`     |
| `serde_arrow`    | Very active       | `Vec<Struct>` ↔ RecordBatch via serde                                     | No — serde data model forces owned values  |
| `arrow2-convert` | Dead (2023)       | Predecessor of `arrow_convert`, targets legacy arrow2                     | No                                         |

`typed-arrow` is the closest match but misses the mold:

1. Positional column matching (index + datatype), not name-based. No `optional` columns, no `other_columns`.
2. Nullability validated lazily per-row, not eagerly at the parse boundary.
3. No metadata schema validation at all.
4. Schema declared as Rust *logical* types (`String`, `i64`); generates builder machinery we don't need. Our derive goes directly on *array* types (`StringArray`) — simpler, inherently zero-copy.


## Crates

* [`arrow-quiver`](crates/arrow-quiver) — the runtime crate: `Error`, `Column`, and the `arrow` re-export
* [`arrow-quiver-derive`](crates/arrow-quiver-derive) — the `#[derive(Quiver)]` proc-macro

## License

Dual-licensed under [MIT](LICENSE-MIT) and [Apache 2.0](LICENSE-APACHE).
