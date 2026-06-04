//! Proc-macros for the `arrow-quiver` crate.
//!
//! Use the `arrow-quiver` crate with the `derive` feature instead of depending on this crate directly.

mod quiver;

use proc_macro::TokenStream;

/// Derives conversions between a struct of typed Arrow arrays and a
/// [`RecordBatch`](https://docs.rs/arrow/latest/arrow/record_batch/struct.RecordBatch.html).
///
/// Generates:
/// * `impl TryFrom<RecordBatch>` — validates the schema (column names, datatypes, nullability),
///   then downcasts the columns (zero-copy)
/// * `impl TryFrom<Self> for RecordBatch` — fails on column length mismatch
///
/// ## Field types
/// * A typed Arrow array (e.g. `StringArray`) — a required column with a specific datatype
/// * `ArrayRef` — a required column of any datatype
/// * `Option<…>` of the above — the column is allowed to be missing
///
/// ## Attributes
/// * `#[quiver(non_null)]` — eagerly check that the column contains no nulls
/// * `#[quiver(name = "special:name")]` — the column name, when it isn't a valid Rust identifier
/// * `#[quiver(metadata)]` — this `BTreeMap<String, String>` field holds the record batch metadata
/// * `#[quiver(extra_columns)]` — this `Vec<Column>` field holds all columns not declared in the struct.
///   If absent, unknown columns are an error.
#[proc_macro_derive(Quiver, attributes(quiver))]
pub fn derive_quiver(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    quiver::derive_quiver(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
