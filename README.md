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

## Example
``` rust
/// Important thing
#[derive(Record)]
struct Thing {
    /// …of the record-batch
    pub metadata: BTreeMap<String, String>,

    /// Name
    #[non_null]
    pub name: StringArray,

    /// Date of birth
    pub dob: Option<TimestampNanosecondArray>,

    /// If missing, the proc-macro enforces no additional columns may exist
    pub other_columns: Vec<Column>,
}

// Proc-macro generates:
// * `impl TryFrom<RecordBatch> for Thing` - validates via the runtime schema, then downcasts
// * `impl TryInto<RecordBatch> for Thing` - fails on column length mismatch
```

## Goals
* Zero-copy conversion to and from arrow `RecordBatch`

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

* [`arrow-quiver`](crates/arrow-quiver) — the runtime `Schema` and validator
* [`arrow-quiver-derive`](crates/arrow-quiver-derive) — the `#[derive(Record)]` proc-macro

## License

Dual-licensed under [MIT](LICENSE-MIT) and [Apache 2.0](LICENSE-APACHE).
