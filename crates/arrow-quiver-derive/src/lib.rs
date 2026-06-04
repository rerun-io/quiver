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
/// * `fn schema()` — the static arrow schema of the declared columns; only generated when all
///   columns have a statically-known datatype (no `ArrayRef`, `ListArray`, …)
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
/// ## Attributes
/// * `#[quiver(name = "special:name")]` — the column name, when it isn't a valid Rust identifier
/// * `#[quiver(metadata)]` — this `BTreeMap<String, String>` field holds the record batch metadata
/// * `#[quiver(extra_columns)]` — this `Vec<DynColumn>` field holds all columns not declared in the struct.
///   If absent, unknown columns are an error.
///
/// ## Roundtrip caveat
/// Column *order* is not always preserved: encoding emits the declared columns first
/// (in declaration order) and appends the `#[quiver(extra_columns)]` at the end —
/// even if they appeared between the declared columns in the original record batch.
#[proc_macro_derive(Quiver, attributes(quiver))]
pub fn derive_quiver(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    quiver::derive_quiver(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
