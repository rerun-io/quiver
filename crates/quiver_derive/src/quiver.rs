//! Implementation of `#[derive(Quiver)]`.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields};

pub fn derive_quiver(input: &DeriveInput) -> syn::Result<TokenStream> {
    let quiver = Quiver::parse(input)?;
    let column_consts = quiver.column_consts();
    let schema_fn = quiver.schema_fn();
    let try_from_batch = quiver.try_from_batch();
    let try_into_batch = quiver.try_into_batch();
    Ok(quote! {
        #column_consts
        #schema_fn
        #try_from_batch
        #try_into_batch
    })
}

/// A struct with `#[derive(Quiver)]` on it.
struct Quiver {
    ident: syn::Ident,

    /// The path to the `quiver` crate (overridable with `#[quiver(crate = "…")]`).
    krate: syn::Path,

    /// How to treat unknown columns when parsing.
    exhaustiveness: Exhaustiveness,

    /// The field marked `#[quiver(metadata)]`, if any.
    metadata_field: Option<syn::Ident>,

    /// The field marked `#[quiver(extra_columns)]`, if any.
    extra_columns_field: Option<syn::Ident>,

    columns: Vec<ColumnField>,
}

/// How to treat unknown columns when parsing.
#[derive(Clone, Copy, PartialEq)]
enum Exhaustiveness {
    /// Unknown columns are an error (unless there is an `extra_columns` field).
    /// This is also the default.
    Exhaustive,

    /// Unknown columns are silently ignored.
    Nonexhaustive,
}

/// A struct field holding an Arrow array, i.e. a record batch column.
struct ColumnField {
    ident: syn::Ident,

    /// The name of the column in the record batch (may differ from `ident`).
    column_name: String,

    /// Is the column allowed to be missing? (the field is an `Option`)
    optional: bool,

    /// Field metadata declared with `#[quiver(metadata("key" = "value", …))]`.
    declared_metadata: Vec<(String, String)>,

    kind: ColumnKind,
}

enum ColumnKind {
    /// `ArrayRef` — any datatype is accepted.
    Any,

    /// A typed array (e.g. `StringArray`) — only the matching datatype is accepted.
    Typed {
        array_type: Box<syn::Type>,
        datatype: TokenStream,
    },

    /// A typed array whose exact datatype depends on runtime parameters
    /// (e.g. `ListArray`, `StructArray`, `DictionaryArray<…>`).
    ///
    /// Validated by downcasting; the inner types are NOT validated.
    Downcast {
        array_type: Box<syn::Type>,

        /// Name of the array type, e.g. `ListArray`. Used in error messages.
        type_name: String,
    },

    /// `quiver::Column<L>` — a strongly-typed wrapper. Validates itself
    /// (exact datatype incl. nested types, and nullability from the logical type).
    Wrapper { column_type: Box<syn::Type> },
}

impl Quiver {
    fn parse(input: &DeriveInput) -> syn::Result<Self> {
        if !input.generics.params.is_empty() {
            return Err(syn::Error::new_spanned(
                &input.generics,
                "#[derive(Quiver)] does not support generics",
            ));
        }
        let Data::Struct(data) = &input.data else {
            return Err(syn::Error::new_spanned(
                input,
                "#[derive(Quiver)] only supports structs",
            ));
        };
        let Fields::Named(fields) = &data.fields else {
            return Err(syn::Error::new_spanned(
                &data.fields,
                "#[derive(Quiver)] only supports structs with named fields",
            ));
        };

        let mut exhaustiveness = None;
        let mut krate: syn::Path = syn::parse_quote!(::quiver);
        for attr in &input.attrs {
            if attr.path().is_ident("quiver") {
                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("crate") {
                        let path: syn::LitStr = meta.value()?.parse()?;
                        krate = path.parse()?;
                        return Ok(());
                    }
                    let value = if meta.path.is_ident("exhaustive") {
                        Exhaustiveness::Exhaustive
                    } else if meta.path.is_ident("nonexhaustive") {
                        Exhaustiveness::Nonexhaustive
                    } else {
                        return Err(
                            meta.error("Expected `crate`, `exhaustive`, or `nonexhaustive`")
                        );
                    };
                    if exhaustiveness.is_some() {
                        return Err(
                            meta.error("Conflicting `exhaustive`/`nonexhaustive` attributes")
                        );
                    }
                    exhaustiveness = Some(value);
                    Ok(())
                })?;
            }
        }

        let mut quiver = Self {
            ident: input.ident.clone(),
            krate,
            exhaustiveness: exhaustiveness.unwrap_or(Exhaustiveness::Exhaustive),
            metadata_field: None,
            extra_columns_field: None,
            columns: Vec::new(),
        };

        for field in &fields.named {
            quiver.parse_field(field)?;
        }

        if quiver.extra_columns_field.is_some() && exhaustiveness.is_some() {
            return Err(syn::Error::new_spanned(
                &input.ident,
                "A #[quiver(extra_columns)] field cannot be combined with the \
                 `exhaustive`/`nonexhaustive` attributes: the field alone already \
                 means unknown columns are collected",
            ));
        }

        Ok(quiver)
    }

    fn parse_field(&mut self, field: &syn::Field) -> syn::Result<()> {
        let ident = field
            .ident
            .clone()
            .ok_or_else(|| syn::Error::new_spanned(field, "Expected a named field"))?;

        let mut column_name = ident.to_string();
        let mut is_metadata = false;
        let mut is_extra_columns = false;
        let mut declared_metadata: Vec<(String, String)> = Vec::new();

        for attr in &field.attrs {
            if attr.path().is_ident("quiver") {
                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("name") {
                        let name: syn::LitStr = meta.value()?.parse()?;
                        column_name = name.value();
                        Ok(())
                    } else if meta.path.is_ident("metadata") {
                        if meta.input.peek(syn::token::Paren) {
                            // #[quiver(metadata("key" = "value", …))]: declared field metadata.
                            if !declared_metadata.is_empty() {
                                return Err(meta.error("Duplicate `metadata(…)` attribute"));
                            }
                            declared_metadata = parse_metadata_pairs(&meta)?;
                        } else {
                            // #[quiver(metadata)]: this field holds the record batch metadata.
                            is_metadata = true;
                        }
                        Ok(())
                    } else if meta.path.is_ident("extra_columns") {
                        is_extra_columns = true;
                        Ok(())
                    } else {
                        Err(meta.error("Expected `name`, `metadata`, or `extra_columns`"))
                    }
                })?;
            }
        }

        if (is_metadata || is_extra_columns) && !declared_metadata.is_empty() {
            return Err(syn::Error::new_spanned(
                field,
                "Declared `metadata(…)` only applies to column fields",
            ));
        }

        if is_metadata {
            if self.metadata_field.is_some() {
                return Err(syn::Error::new_spanned(
                    field,
                    "Multiple fields marked #[quiver(metadata)]",
                ));
            }
            self.metadata_field = Some(ident);
        } else if is_extra_columns {
            if self.extra_columns_field.is_some() {
                return Err(syn::Error::new_spanned(
                    field,
                    "Multiple fields marked #[quiver(extra_columns)]",
                ));
            }
            self.extra_columns_field = Some(ident);
        } else {
            let (optional, kind) = classify_type(&self.krate.clone(), &field.ty)?;
            self.columns.push(ColumnField {
                ident,
                column_name,
                optional,
                declared_metadata,
                kind,
            });
        }

        Ok(())
    }

    /// Generates `COLUMN_*` descriptor constants for every column.
    fn column_consts(&self) -> TokenStream {
        let Self { ident, columns, .. } = self;
        let krate = &self.krate;
        let record_type = ident.to_string();

        let consts = columns.iter().map(|column| {
            let ColumnField {
                ident: field_ident,
                column_name,
                declared_metadata,
                kind,
                ..
            } = column;
            let const_ident = format_ident!("COLUMN_{}", field_ident.to_string().to_uppercase());
            let name_const_ident =
                format_ident!("COLUMN_{}_NAME", field_ident.to_string().to_uppercase());
            let doc = format!("The {column_name:?} column.");
            let name_doc = format!(
                "The name of the {column_name:?} column: a plain `&str` constant, \
                 usable in `match` patterns (unlike the field access `COLUMN_*.name`)."
            );
            let name_const = quote! {
                #[doc = #name_doc]
                pub const #name_const_ident: &'static str = #column_name;
            };
            let declared = declared_metadata
                .iter()
                .map(|(key, value)| quote! { (#key, #value) });
            let declared = quote! { &[#(#declared),*] };
            match kind {
                ColumnKind::Wrapper { column_type } => quote! {
                    #name_const

                    #[doc = #doc]
                    pub const #const_ident: #krate::ColumnDesc<#column_type> =
                        #krate::ColumnDesc::new(#record_type, Self::#name_const_ident, #declared);
                },
                ColumnKind::Any | ColumnKind::Typed { .. } | ColumnKind::Downcast { .. } => {
                    quote! {
                        #name_const

                        #[doc = #doc]
                        pub const #const_ident: #krate::DynColumnDesc =
                            #krate::DynColumnDesc::new(#record_type, Self::#name_const_ident);
                    }
                }
            }
        });

        quote! {
            #[automatically_derived]
            impl #ident {
                #(#consts)*
            }
        }
    }

    /// Generates `impl #ident { fn min_schema(); fn max_schema(); … }`,
    /// if all columns have a statically-known datatype.
    fn schema_fn(&self) -> Option<TokenStream> {
        let Self { ident, columns, .. } = self;
        let krate = &self.krate;

        let fields = columns
            .iter()
            .map(|column| {
                let ColumnField {
                    column_name,
                    declared_metadata,
                    kind,
                    optional,
                    ..
                } = column;
                let declared = declared_metadata.iter().map(|(key, value)| {
                    quote! { (#key.to_owned(), #value.to_owned()) }
                });
                let metadata = quote! {
                    .with_metadata([#(#declared),*].into_iter().collect())
                };
                let field = match kind {
                    ColumnKind::Wrapper { column_type } => quote! {
                        #krate::arrow::datatypes::Field::new(
                            #column_name,
                            <#column_type>::datatype(),
                            <#column_type>::NULLABLE,
                        )
                        #metadata
                    },
                    ColumnKind::Typed { datatype, .. } => quote! {
                        // The nullability of raw arrow arrays is not statically known:
                        #krate::arrow::datatypes::Field::new(#column_name, #datatype, true)
                            #metadata
                    },
                    // Not statically known:
                    ColumnKind::Any | ColumnKind::Downcast { .. } => return None,
                };
                Some((field, *optional))
            })
            .collect::<Option<Vec<_>>>()?;

        let max_fields: Vec<_> = fields.iter().map(|(field, _)| field).collect();
        let min_fields: Vec<_> = fields
            .iter()
            .filter(|(_, optional)| !optional)
            .map(|(field, _)| field)
            .collect();

        // `empty_record_batch` is only generated when every column is
        // required (`min_schema() == max_schema()`): with optional columns
        // there is no single obvious empty batch (include them or not?),
        // and a round-trip would silently turn `None` into `Some(empty)`.
        let empty_record_batch_fn = (min_fields.len() == max_fields.len()).then(|| {
            quote! {
                /// An empty (zero-row) record batch with every declared column
                /// present ([`Self::min_schema`], which here equals
                /// [`Self::max_schema`]).
                ///
                /// Only generated when all columns are required:
                /// structs with optional (`Option<…>`) columns don't get this fn.
                #[must_use]
                pub fn empty_record_batch() -> #krate::arrow::record_batch::RecordBatch {
                    #krate::arrow::record_batch::RecordBatch::new_empty(
                        ::std::sync::Arc::new(Self::max_schema()),
                    )
                }
            }
        });

        Some(quote! {
            #[automatically_derived]
            impl #ident {
                /// The static schema of the *required* columns:
                /// the guaranteed-present subset of every matching record batch.
                /// Optional (`Option<…>`) columns are excluded; see [`Self::max_schema`].
                ///
                /// Per-instance metadata is not included.
                #[must_use]
                pub fn min_schema() -> #krate::arrow::datatypes::Schema {
                    #krate::arrow::datatypes::Schema::new(::std::vec![
                        #(#min_fields),*
                    ])
                }

                /// The static schema of *all* declared columns,
                /// including optional (`Option<…>`) ones —
                /// which may be missing from an actual record batch;
                /// see [`Self::min_schema`].
                ///
                /// Per-instance metadata is not included.
                #[must_use]
                pub fn max_schema() -> #krate::arrow::datatypes::Schema {
                    #krate::arrow::datatypes::Schema::new(::std::vec![
                        #(#max_fields),*
                    ])
                }

                #empty_record_batch_fn
            }
        })
    }

    /// Generates `impl TryFrom<RecordBatch> for #ident`.
    fn try_from_batch(&self) -> TokenStream {
        let Self {
            ident,
            krate,
            exhaustiveness,
            metadata_field,
            extra_columns_field,
            columns,
        } = self;

        let known_columns = columns.iter().map(|column| column.column_name.as_str());
        let known_columns = quote! {
            const KNOWN_COLUMNS: &[&str] = &[#(#known_columns),*];
        };

        let extract_metadata = metadata_field.as_ref().map(|metadata_ident| {
            quote! {
                let #metadata_ident: ::std::collections::BTreeMap<::std::string::String, ::std::string::String> = batch
                    .schema_ref()
                    .metadata()
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect();
            }
        });

        let record_type = ident.to_string();

        // Collect the unknown columns, error on them, or ignore them:
        let extra_columns = if let Some(extra_ident) = extra_columns_field {
            quote! {
                #known_columns
                let #extra_ident: ::std::vec::Vec<#krate::DynColumn> =
                    ::std::iter::zip(batch.schema_ref().fields(), batch.columns())
                        .filter(|(field, _)| !KNOWN_COLUMNS.contains(&field.name().as_str()))
                        .map(|(field, array)| #krate::DynColumn {
                            field: ::std::sync::Arc::clone(field),
                            array: ::std::sync::Arc::clone(array),
                        })
                        .collect();
            }
        } else if *exhaustiveness == Exhaustiveness::Exhaustive {
            quote! {
                #known_columns
                for field in batch.schema_ref().fields() {
                    if !KNOWN_COLUMNS.contains(&field.name().as_str()) {
                        return ::core::result::Result::Err(#krate::Error {
                            record_type: #record_type,
                            kind: #krate::ErrorKind::UnexpectedColumn {
                                column: field.name().clone(),
                            },
                        });
                    }
                }
            }
        } else {
            // Nonexhaustive: unknown columns are silently ignored.
            quote! {}
        };

        let extractors = columns
            .iter()
            .map(|column| column.extractor(krate, &record_type));

        let field_idents = metadata_field
            .iter()
            .chain(columns.iter().map(|column| &column.ident))
            .chain(extra_columns_field.iter());

        quote! {
            #[automatically_derived]
            impl ::core::convert::TryFrom<#krate::arrow::record_batch::RecordBatch> for #ident {
                type Error = #krate::Error;

                fn try_from(
                    batch: #krate::arrow::record_batch::RecordBatch,
                ) -> ::core::result::Result<Self, Self::Error> {
                    #extract_metadata
                    #extra_columns
                    #(#extractors)*
                    ::core::result::Result::Ok(Self { #(#field_idents),* })
                }
            }

            #[automatically_derived]
            impl ::core::convert::TryFrom<&#krate::arrow::record_batch::RecordBatch> for #ident {
                type Error = #krate::Error;

                fn try_from(
                    batch: &#krate::arrow::record_batch::RecordBatch,
                ) -> ::core::result::Result<Self, Self::Error> {
                    // Cloning a record batch is cheap (the columns are reference-counted):
                    Self::try_from(::core::clone::Clone::clone(batch))
                }
            }

            #[automatically_derived]
            impl #ident {
                /// Parses an arrow record batch:
                /// validates the schema, then downcasts the columns (zero-copy).
                ///
                /// # Errors
                /// Errors on missing or unexpected columns, datatype mismatches,
                /// or unexpected nulls.
                pub fn from_record_batch(
                    batch: #krate::arrow::record_batch::RecordBatch,
                ) -> ::core::result::Result<Self, #krate::Error> {
                    Self::try_from(batch)
                }
            }
        }
    }

    /// Generates `impl TryFrom<#ident> for RecordBatch`,
    /// plus the discoverable `fn into_record_batch()` alias.
    fn try_into_batch(&self) -> TokenStream {
        let Self {
            ident,
            krate,
            exhaustiveness: _, // only affects parsing
            metadata_field,
            extra_columns_field,
            columns,
        } = self;

        let record_type = ident.to_string();
        let pushes = columns.iter().map(|column| column.push(krate));

        let push_extra = extra_columns_field.as_ref().map(|extra_ident| {
            quote! {
                for column in value.#extra_ident {
                    fields.push(column.field);
                    columns.push(column.array);
                }
            }
        });

        let schema = if let Some(metadata_ident) = metadata_field {
            quote! {
                #krate::arrow::datatypes::Schema::new(fields)
                    .with_metadata(value.#metadata_ident.into_iter().collect())
            }
        } else {
            quote! { #krate::arrow::datatypes::Schema::new(fields) }
        };

        quote! {
            #[automatically_derived]
            impl ::core::convert::TryFrom<#ident> for #krate::arrow::record_batch::RecordBatch {
                type Error = #krate::Error;

                fn try_from(value: #ident) -> ::core::result::Result<Self, Self::Error> {
                    let mut fields: ::std::vec::Vec<#krate::arrow::datatypes::FieldRef> =
                        ::std::vec::Vec::new();
                    let mut columns: ::std::vec::Vec<#krate::arrow::array::ArrayRef> =
                        ::std::vec::Vec::new();
                    #(#pushes)*
                    #push_extra
                    let schema = #schema;
                    #krate::arrow::record_batch::RecordBatch::try_new(
                        ::std::sync::Arc::new(schema),
                        columns,
                    )
                    .map_err(|err| #krate::Error {
                        record_type: #record_type,
                        kind: #krate::ErrorKind::BuildRecordBatch(err),
                    })
                }
            }

            #[automatically_derived]
            impl #ident {
                /// Converts into an arrow record batch.
                ///
                /// # Errors
                /// Errors on column length mismatch.
                pub fn into_record_batch(
                    self,
                ) -> ::core::result::Result<
                    #krate::arrow::record_batch::RecordBatch,
                    #krate::Error,
                > {
                    #krate::arrow::record_batch::RecordBatch::try_from(self)
                }
            }
        }
    }
}

impl ColumnField {
    /// Generates `let #ident = …;`, extracting the column from `batch`.
    fn extractor(&self, krate: &syn::Path, record_type: &str) -> TokenStream {
        let Self {
            ident,
            column_name,
            optional,
            declared_metadata: _, // encode-side only; parsing does not validate metadata
            kind,
        } = self;

        // Wrappers also need the arrow `Field` (for the metadata), so they bind differently:
        if let ColumnKind::Wrapper { column_type } = kind {
            // `array` is a `&ArrayRef`, `field` is a `&Field`:
            let convert = quote! {
                <#column_type>::try_new(::std::sync::Arc::clone(array))
                    .map_err(|err| #krate::Error {
                        record_type: #record_type,
                        kind: err.for_column(#column_name.to_owned()),
                    })?
                    .with_metadata(
                        field
                            .metadata()
                            .iter()
                            .map(|(key, value)| (key.clone(), value.clone()))
                            .collect(),
                    )
            };
            return if *optional {
                quote! {
                    let #ident = match batch.schema_ref().column_with_name(#column_name) {
                        ::core::option::Option::Some((index, field)) => {
                            let array = batch.column(index);
                            ::core::option::Option::Some(#convert)
                        }
                        ::core::option::Option::None => ::core::option::Option::None,
                    };
                }
            } else {
                quote! {
                    let #ident = {
                        let (index, field) = batch
                            .schema_ref()
                            .column_with_name(#column_name)
                            .ok_or_else(|| #krate::Error {
                                record_type: #record_type,
                                kind: #krate::ErrorKind::MissingColumn {
                                    column: #column_name.to_owned(),
                                },
                            })?;
                        let array = batch.column(index);
                        #convert
                    };
                }
            };
        }

        // `array` is a `&ArrayRef`:
        let convert = match kind {
            ColumnKind::Any => quote! { ::std::sync::Arc::clone(array) },
            ColumnKind::Typed {
                array_type,
                datatype,
            } => {
                let downcast = downcast(
                    krate,
                    record_type,
                    column_name,
                    array_type,
                    "a matching array",
                );
                quote! {
                    {
                        let actual = #krate::arrow::array::Array::data_type(&**array);
                        if actual != &#datatype {
                            return ::core::result::Result::Err(#krate::Error {
                                record_type: #record_type,
                                kind: #krate::ErrorKind::WrongDatatype {
                                    column: #column_name.to_owned(),
                                    expected: #datatype,
                                    actual: actual.clone(),
                                },
                            });
                        }
                        #downcast
                    }
                }
            }
            ColumnKind::Downcast {
                array_type,
                type_name,
            } => downcast(krate, record_type, column_name, array_type, type_name),
            ColumnKind::Wrapper { .. } => unreachable!("Handled above"),
        };

        if *optional {
            quote! {
                let #ident = match batch.column_by_name(#column_name) {
                    ::core::option::Option::Some(array) => {
                        ::core::option::Option::Some(#convert)
                    }
                    ::core::option::Option::None => ::core::option::Option::None,
                };
            }
        } else {
            quote! {
                let #ident = {
                    let array = batch
                        .column_by_name(#column_name)
                        .ok_or_else(|| #krate::Error {
                            record_type: #record_type,
                            kind: #krate::ErrorKind::MissingColumn {
                                column: #column_name.to_owned(),
                            },
                        })?;
                    #convert
                };
            }
        }
    }

    /// Generates code pushing this column of `value` onto `fields` and `columns`.
    fn push(&self, krate: &syn::Path) -> TokenStream {
        let Self {
            ident,
            column_name,
            optional,
            declared_metadata,
            kind,
        } = self;

        let declared = declared_metadata.iter().map(|(key, value)| {
            quote! { (#key.to_owned(), #value.to_owned()) }
        });
        let declared = quote! {
            [#(#declared),*]
        };

        // Raw arrow arrays are dynamically typed; we don't know if they contain nulls:
        let nullable = true;

        // `array` is the (typed) array by value:
        let push_one = match kind {
            ColumnKind::Any => quote! {
                fields.push(::std::sync::Arc::new(
                    #krate::arrow::datatypes::Field::new(
                        #column_name,
                        #krate::arrow::array::Array::data_type(&array).clone(),
                        #nullable,
                    )
                    .with_metadata(#declared.into_iter().collect()),
                ));
                columns.push(array);
            },
            ColumnKind::Typed { datatype, .. } => quote! {
                fields.push(::std::sync::Arc::new(
                    #krate::arrow::datatypes::Field::new(
                        #column_name,
                        #datatype,
                        #nullable,
                    )
                    .with_metadata(#declared.into_iter().collect()),
                ));
                columns.push(::std::sync::Arc::new(array));
            },
            ColumnKind::Downcast { .. } => quote! {
                fields.push(::std::sync::Arc::new(
                    #krate::arrow::datatypes::Field::new(
                        #column_name,
                        #krate::arrow::array::Array::data_type(&array).clone(),
                        #nullable,
                    )
                    .with_metadata(#declared.into_iter().collect()),
                ));
                columns.push(::std::sync::Arc::new(array));
            },
            ColumnKind::Wrapper { column_type } => quote! {
                // Declared metadata first; the per-instance metadata wins on key conflicts:
                let mut metadata: ::std::collections::HashMap<::std::string::String, ::std::string::String> =
                    #declared.into_iter().collect();
                metadata.extend(
                    array
                        .metadata()
                        .iter()
                        .map(|(key, value)| (key.clone(), value.clone())),
                );
                fields.push(::std::sync::Arc::new(
                    #krate::arrow::datatypes::Field::new(
                        #column_name,
                        <#column_type>::datatype(),
                        <#column_type>::NULLABLE,
                    )
                    .with_metadata(metadata),
                ));
                columns.push(array.into_arrow());
            },
        };

        if *optional {
            quote! {
                if let ::core::option::Option::Some(array) = value.#ident {
                    #push_one
                }
            }
        } else {
            quote! {
                {
                    let array = value.#ident;
                    #push_one
                }
            }
        }
    }
}

/// Generates an expression downcasting `array` (a `&ArrayRef`) to `array_type`.
fn downcast(
    krate: &syn::Path,
    record_type: &str,
    column_name: &str,
    array_type: &syn::Type,
    expected: &str,
) -> TokenStream {
    quote! {
        #krate::arrow::array::Array::as_any(&**array)
            .downcast_ref::<#array_type>()
            .ok_or_else(|| #krate::Error {
                record_type: #record_type,
                kind: #krate::ErrorKind::WrongArrayType {
                    column: #column_name.to_owned(),
                    expected: #expected.to_owned(),
                    actual: #krate::arrow::array::Array::data_type(&**array).clone(),
                },
            })?
            .clone()
    }
}

/// Parses the `("key" = "value", …)` pairs of a declared-metadata attribute.
fn parse_metadata_pairs(
    meta: &syn::meta::ParseNestedMeta<'_>,
) -> syn::Result<Vec<(String, String)>> {
    let mut pairs = Vec::new();
    let content;
    syn::parenthesized!(content in meta.input);
    while !content.is_empty() {
        let key: syn::LitStr = content.parse()?;
        content.parse::<syn::Token![=]>()?;
        let value: syn::LitStr = content.parse()?;
        pairs.push((key.value(), value.value()));
        if !content.is_empty() {
            content.parse::<syn::Token![,]>()?;
        }
    }
    Ok(pairs)
}

/// Splits an optional `Option` wrapper from the inner array type.
fn classify_type(krate: &syn::Path, ty: &syn::Type) -> syn::Result<(bool, ColumnKind)> {
    if let Some(inner) = option_inner(ty) {
        Ok((true, classify_array_type(krate, inner)?))
    } else {
        Ok((false, classify_array_type(krate, ty)?))
    }
}

fn classify_array_type(krate: &syn::Path, ty: &syn::Type) -> syn::Result<ColumnKind> {
    let unsupported = |ty: &syn::Type| {
        syn::Error::new_spanned(
            ty,
            "Unsupported column type. Expected a typed Arrow array (e.g. `StringArray` or `ListArray`), \
             or `ArrayRef` for any datatype",
        )
    };

    let syn::Type::Path(type_path) = ty else {
        return Err(unsupported(ty));
    };
    let segment = type_path
        .path
        .segments
        .last()
        .ok_or_else(|| unsupported(ty))?;

    let type_name = segment.ident.to_string();
    if type_name == "ArrayRef" {
        Ok(ColumnKind::Any)
    } else if type_name == "Column" {
        Ok(ColumnKind::Wrapper {
            column_type: Box::new(ty.clone()),
        })
    } else if let Some(datatype) = datatype_of_array(krate, &type_name) {
        Ok(ColumnKind::Typed {
            array_type: Box::new(ty.clone()),
            datatype,
        })
    } else if is_downcast_only_array(&type_name) {
        Ok(ColumnKind::Downcast {
            array_type: Box::new(ty.clone()),
            type_name,
        })
    } else if is_punted_array(&type_name) {
        Err(syn::Error::new_spanned(
            ty,
            format!("`{type_name}` is explicitly not supported (yet)"),
        ))
    } else {
        Err(unsupported(ty))
    }
}

/// Array types whose exact datatype depends on runtime parameters,
/// so we can only validate them by downcasting.
fn is_downcast_only_array(array_type_name: &str) -> bool {
    matches!(
        array_type_name,
        "DictionaryArray"
            | "FixedSizeBinaryArray"
            | "FixedSizeListArray"
            | "LargeListArray"
            | "LargeListViewArray"
            | "ListArray"
            | "ListViewArray"
            | "MapArray"
            | "RunArray"
            | "StructArray"
    )
}

/// Array types we do not yet support, not even as raw downcast-only fields.
fn is_punted_array(array_type_name: &str) -> bool {
    matches!(
        array_type_name,
        "Decimal32Array"
            | "Decimal64Array"
            | "Decimal128Array"
            | "Decimal256Array"
            | "IntervalDayTimeArray"
            | "IntervalMonthDayNanoArray"
            | "IntervalYearMonthArray"
            | "UnionArray"
    )
}

/// Returns the type the array is `Option`-wrapping, if it is.
fn option_inner(ty: &syn::Type) -> Option<&syn::Type> {
    let syn::Type::Path(type_path) = ty else {
        return None;
    };
    let segment = type_path.path.segments.last()?;
    if segment.ident != "Option" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    if args.args.len() != 1 {
        return None;
    }
    let syn::GenericArgument::Type(inner) = args.args.first()? else {
        return None;
    };
    Some(inner)
}

/// The Arrow datatype of the given array type, e.g. `StringArray` → `DataType::Utf8`.
fn datatype_of_array(krate: &syn::Path, array_type_name: &str) -> Option<TokenStream> {
    let datatype = quote! { #krate::arrow::datatypes::DataType };
    let timestamp = |unit: TokenStream| {
        quote! {
            #datatype::Timestamp(
                #krate::arrow::datatypes::TimeUnit::#unit,
                ::core::option::Option::None,
            )
        }
    };

    let time_unit = |unit: TokenStream| {
        quote! { #krate::arrow::datatypes::TimeUnit::#unit }
    };

    Some(match array_type_name {
        "BooleanArray" => quote! { #datatype::Boolean },
        "Int8Array" => quote! { #datatype::Int8 },
        "Int16Array" => quote! { #datatype::Int16 },
        "Int32Array" => quote! { #datatype::Int32 },
        "Int64Array" => quote! { #datatype::Int64 },
        "UInt8Array" => quote! { #datatype::UInt8 },
        "UInt16Array" => quote! { #datatype::UInt16 },
        "UInt32Array" => quote! { #datatype::UInt32 },
        "UInt64Array" => quote! { #datatype::UInt64 },
        "Float16Array" => quote! { #datatype::Float16 },
        "Float32Array" => quote! { #datatype::Float32 },
        "Float64Array" => quote! { #datatype::Float64 },
        "StringArray" => quote! { #datatype::Utf8 },
        "LargeStringArray" => quote! { #datatype::LargeUtf8 },
        "StringViewArray" => quote! { #datatype::Utf8View },
        "BinaryArray" => quote! { #datatype::Binary },
        "LargeBinaryArray" => quote! { #datatype::LargeBinary },
        "BinaryViewArray" => quote! { #datatype::BinaryView },
        "Date32Array" => quote! { #datatype::Date32 },
        "Date64Array" => quote! { #datatype::Date64 },
        "TimestampSecondArray" => timestamp(quote! { Second }),
        "TimestampMillisecondArray" => timestamp(quote! { Millisecond }),
        "TimestampMicrosecondArray" => timestamp(quote! { Microsecond }),
        "TimestampNanosecondArray" => timestamp(quote! { Nanosecond }),
        "Time32SecondArray" => {
            let unit = time_unit(quote! { Second });
            quote! { #datatype::Time32(#unit) }
        }
        "Time32MillisecondArray" => {
            let unit = time_unit(quote! { Millisecond });
            quote! { #datatype::Time32(#unit) }
        }
        "Time64MicrosecondArray" => {
            let unit = time_unit(quote! { Microsecond });
            quote! { #datatype::Time64(#unit) }
        }
        "Time64NanosecondArray" => {
            let unit = time_unit(quote! { Nanosecond });
            quote! { #datatype::Time64(#unit) }
        }
        "DurationSecondArray" => {
            let unit = time_unit(quote! { Second });
            quote! { #datatype::Duration(#unit) }
        }
        "DurationMillisecondArray" => {
            let unit = time_unit(quote! { Millisecond });
            quote! { #datatype::Duration(#unit) }
        }
        "DurationMicrosecondArray" => {
            let unit = time_unit(quote! { Microsecond });
            quote! { #datatype::Duration(#unit) }
        }
        "DurationNanosecondArray" => {
            let unit = time_unit(quote! { Nanosecond });
            quote! { #datatype::Duration(#unit) }
        }
        _ => return None,
    })
}
