//! Proc-macros for the `arrow-quiver` crate.
//!
//! Use the `arrow-quiver` crate with the `derive` feature instead of depending on this crate directly.

use proc_macro::TokenStream;

/// Derives a strongly typed wrapper around a [`RecordBatch`](https://docs.rs/arrow/latest/arrow/record_batch/struct.RecordBatch.html).
///
/// Generates:
/// * `fn schema() -> arrow_quiver::Schema`
/// * `impl TryFrom<RecordBatch>` — validates via the runtime schema, then downcasts
/// * `impl TryInto<RecordBatch>` — fails on column length mismatch
#[proc_macro_derive(Record, attributes(record, field, non_null))]
pub fn derive_record(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);

    // TODO(emilk): implement the derive macro.
    syn::Error::new_spanned(&input.ident, "#[derive(Record)] is not yet implemented")
        .to_compile_error()
        .into()
}
