//! Proc-macros for the `arrow-quiver` crate.
//!
//! Use the `arrow-quiver` crate with the `derive` feature instead of depending on this crate directly.

mod quiver;

use proc_macro::TokenStream;

/// Derives conversions between a struct of typed Arrow arrays and a
/// [`RecordBatch`](https://docs.rs/arrow/latest/arrow/record_batch/struct.RecordBatch.html).
///
/// Generates:
/// * `impl TryFrom<RecordBatch>` and `impl TryFrom<&RecordBatch>` — validates the schema
///   (column names, datatypes, nullability), then downcasts the columns (zero-copy)
/// * `impl TryFrom<Self> for RecordBatch` — fails on column length mismatch
/// * `fn from_record_batch()` and `fn into_record_batch()` — discoverable aliases for the above
/// * `COLUMN_*` constants (e.g. `COLUMN_TEMPERATURE`) — per-column descriptors with the column
///   name and an `extract(&batch)` method for pulling out one column
/// * `fn min_schema()` / `fn max_schema()` — the static arrow schema of the required columns /
///   of all declared columns (including optional ones); only generated when all columns have a
///   statically-known datatype (no `ArrayRef`, `ListArray`, …)
/// * `fn empty_record_batch()` — an infallible, zero-row record batch with the max schema
///
/// ## Field types
/// * `quiver::Column<L>` — a strongly-typed wrapper; validates the exact datatype
///   (including the inner types of nested arrays) and nullability from the logical type `L`
/// * A typed Arrow array (e.g. `StringArray`) — a required column with a specific datatype
/// * A parameterized Arrow array (e.g. `ListArray`, `StructArray`, `DictionaryArray<…>`) —
///   a required column, validated by downcast only (the inner types are NOT validated)
/// * `ArrayRef` — a required column of any datatype
/// * `Option<…>` of the above — the column is allowed to be missing
///
/// Use `quiver::Column<L>` for strong compile-time guarantees (exact datatypes, nullability),
/// and raw arrow types when you *want* things to be dynamic.
///
/// ## Struct attributes
/// * `#[quiver(crate = "path::to::arrow_quiver")]` — the path the generated code uses to refer
///   to the `arrow_quiver` crate (default `::arrow_quiver`), for renamed dependencies and
///   re-exports (proc-macros have no `$crate` equivalent)
/// * `#[quiver(exhaustive)]` — unknown columns are an error when parsing (the default, made explicit)
/// * `#[quiver(nonexhaustive)]` — unknown columns are silently ignored when parsing
///
/// Neither can be combined with an `extra_columns` field (which alone already means
/// unknown columns are collected).
///
/// ## Field attributes
/// * `#[quiver(name = "special:name")]` — the column name, when it isn't a valid Rust identifier
/// * `#[quiver(metadata("key" = "value", …))]` — *declared* field metadata, stamped onto the
///   emitted arrow `Field` when encoding (merged with the per-instance
///   [`Column::metadata`](https://docs.rs/arrow-quiver/latest/arrow_quiver/struct.Column.html#method.metadata);
///   the instance wins on key conflicts), and included in the static `schema()` and the
///   `COLUMN_*` descriptors. **Not validated when parsing** — metadata is an annotation,
///   not access semantics, and intermediaries routinely strip it. Side effect: a
///   parse → encode roundtrip re-stamps declared metadata even if the input lacked it.
/// * `#[quiver(metadata)]` — this `BTreeMap<String, String>` field holds the record batch metadata
/// * `#[quiver(extra_columns)]` — this `Vec<DynColumn>` field holds all columns not declared in the struct.
///   If absent, unknown columns are an error.
///
/// ## Column order
/// All column matching is done by *name*; column order never matters when parsing.
/// Encoding emits the columns in struct declaration order, with the
/// `#[quiver(extra_columns)]` appended at the end — the input column order
/// is not preserved on a parse → encode roundtrip.
#[proc_macro_derive(Quiver, attributes(quiver))]
pub fn derive_quiver(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    quiver::derive_quiver(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
