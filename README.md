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
* [ ] strong quiver datatypes
* [ ] support const-generics-based support for DataType::FixedSizeBinary(16) etc
  (should map to `[u8; 16]` in this case)
* [ ] `Struct` logical type for `quiver::Column` (punted for now)
* [ ] `Timestamp`/`Duration`/`Dictionary` logical types for `quiver::Column`
* [ ] Field-level metadata requirements, e.g. `#[quiver(required_metadata("unit"))]`
* [ ] `#[quiver(readonly)]` — invariant-by-construction variant (see `plan.md`)
* [ ] Publish to crates.io

## Example
For a complete, compiling example, see [`example.rs`](crates/arrow-quiver/examples/example.rs).
Run it with `cargo run --example example`.

Use the strongly-typed `quiver::Column<L>` for compile-time guarantees (exact datatype,
including nested types, and nullability), and raw `arrow` types when you _want_ things
to be dynamic:

``` rust
/// Important thing
#[derive(arrow_quiver::Quiver)]
struct Thing {
    /// …of the record-batch
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

    /// If missing, the proc-macro enforces no additional columns may exist
    #[quiver(extra_columns)]
    pub other_columns: Vec<DynColumn>,
}

// Proc-macro generates:
// * `impl TryFrom<RecordBatch> for Thing` - validates the schema, then downcasts (zero-copy)
// * `impl TryFrom<Thing> for RecordBatch` - fails on column length mismatch
```

`quiver::Column` is also usable standalone, without the derive:

``` rust
let column = quiver::Column::<List<String>>::try_from(dynamic_arrow_array)?;
for list in &column {
    for string in list {
        // `string` is a `&str`; validation already happened, up front
    }
}
```

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
| Crate            | Status            | What it does                                                              | Zero-copy SoA?                            |
|------------------|-------------------|---------------------------------------------------------------------------|-------------------------------------------|
| `typed-arrow`    | Active (tonbo-io) | `#[derive(Record)]` on *logical* types → builders, schema, lazy row views | **Yes** (`views` feature)                 |
| `arrow_convert`  | Active            | serde-style derive, Rust types ↔ Arrow arrays                             | No — transposes + copies into `Vec<T>`    |
| `serde_arrow`    | Very active       | `Vec<Struct>` ↔ RecordBatch via serde                                     | No — serde data model forces owned values |

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
