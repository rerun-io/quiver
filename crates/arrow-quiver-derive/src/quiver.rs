//! Implementation of `#[derive(Quiver)]`.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields};

pub fn derive_quiver(input: &DeriveInput) -> syn::Result<TokenStream> {
    let quiver = Quiver::parse(input)?;
    let try_from_batch = quiver.try_from_batch();
    let try_into_batch = quiver.try_into_batch();
    Ok(quote! {
        #try_from_batch
        #try_into_batch
    })
}

/// A struct with `#[derive(Quiver)]` on it.
struct Quiver {
    ident: syn::Ident,

    /// The field marked `#[quiver(metadata)]`, if any.
    metadata_field: Option<syn::Ident>,

    /// The field marked `#[quiver(extra_columns)]`, if any.
    extra_columns_field: Option<syn::Ident>,

    columns: Vec<ColumnField>,
}

/// A struct field holding an Arrow array, i.e. a record batch column.
struct ColumnField {
    ident: syn::Ident,

    /// The name of the column in the record batch (may differ from `ident`).
    column_name: String,

    /// Is the column allowed to be missing? (the field is an `Option`)
    optional: bool,

    /// Eagerly check that the column contains no nulls?
    non_null: bool,

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

        let mut quiver = Self {
            ident: input.ident.clone(),
            metadata_field: None,
            extra_columns_field: None,
            columns: Vec::new(),
        };

        for field in &fields.named {
            quiver.parse_field(field)?;
        }

        Ok(quiver)
    }

    fn parse_field(&mut self, field: &syn::Field) -> syn::Result<()> {
        let ident = field
            .ident
            .clone()
            .ok_or_else(|| syn::Error::new_spanned(field, "Expected a named field"))?;

        let mut non_null = false;
        let mut column_name = ident.to_string();
        let mut is_metadata = false;
        let mut is_extra_columns = false;

        for attr in &field.attrs {
            if attr.path().is_ident("quiver") {
                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("non_null") {
                        non_null = true;
                        Ok(())
                    } else if meta.path.is_ident("name") {
                        let name: syn::LitStr = meta.value()?.parse()?;
                        column_name = name.value();
                        Ok(())
                    } else if meta.path.is_ident("metadata") {
                        is_metadata = true;
                        Ok(())
                    } else if meta.path.is_ident("extra_columns") {
                        is_extra_columns = true;
                        Ok(())
                    } else {
                        Err(meta
                            .error("Expected `non_null`, `name`, `metadata`, or `extra_columns`"))
                    }
                })?;
            }
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
            let (optional, kind) = classify_type(&field.ty)?;
            self.columns.push(ColumnField {
                ident,
                column_name,
                optional,
                non_null,
                kind,
            });
        }

        Ok(())
    }

    /// Generates `impl TryFrom<RecordBatch> for #ident`.
    fn try_from_batch(&self) -> TokenStream {
        let Self {
            ident,
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

        // Either collect the unknown columns, or error on them:
        let extra_columns = if let Some(extra_ident) = extra_columns_field {
            quote! {
                let #extra_ident: ::std::vec::Vec<::arrow_quiver::Column> =
                    ::std::iter::zip(batch.schema_ref().fields(), batch.columns())
                        .filter(|(field, _)| !KNOWN_COLUMNS.contains(&field.name().as_str()))
                        .map(|(field, array)| ::arrow_quiver::Column {
                            field: ::std::sync::Arc::clone(field),
                            array: ::std::sync::Arc::clone(array),
                        })
                        .collect();
            }
        } else {
            quote! {
                for field in batch.schema_ref().fields() {
                    if !KNOWN_COLUMNS.contains(&field.name().as_str()) {
                        return ::core::result::Result::Err(::arrow_quiver::Error {
                            record_type: #record_type,
                            kind: ::arrow_quiver::ErrorKind::UnexpectedColumn {
                                column: field.name().clone(),
                            },
                        });
                    }
                }
            }
        };

        let extractors = columns.iter().map(|column| column.extractor(&record_type));

        let field_idents = metadata_field
            .iter()
            .chain(columns.iter().map(|column| &column.ident))
            .chain(extra_columns_field.iter());

        quote! {
            #[automatically_derived]
            impl ::core::convert::TryFrom<::arrow_quiver::arrow::record_batch::RecordBatch> for #ident {
                type Error = ::arrow_quiver::Error;

                fn try_from(
                    batch: ::arrow_quiver::arrow::record_batch::RecordBatch,
                ) -> ::core::result::Result<Self, Self::Error> {
                    #known_columns
                    #extract_metadata
                    #extra_columns
                    #(#extractors)*
                    ::core::result::Result::Ok(Self { #(#field_idents),* })
                }
            }
        }
    }

    /// Generates `impl TryFrom<#ident> for RecordBatch`.
    fn try_into_batch(&self) -> TokenStream {
        let Self {
            ident,
            metadata_field,
            extra_columns_field,
            columns,
        } = self;

        let record_type = ident.to_string();

        let pushes = columns.iter().map(|column| column.push(&record_type));

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
                ::arrow_quiver::arrow::datatypes::Schema::new(fields)
                    .with_metadata(value.#metadata_ident.into_iter().collect())
            }
        } else {
            quote! { ::arrow_quiver::arrow::datatypes::Schema::new(fields) }
        };

        quote! {
            #[automatically_derived]
            impl ::core::convert::TryFrom<#ident> for ::arrow_quiver::arrow::record_batch::RecordBatch {
                type Error = ::arrow_quiver::Error;

                fn try_from(value: #ident) -> ::core::result::Result<Self, Self::Error> {
                    let mut fields: ::std::vec::Vec<::arrow_quiver::arrow::datatypes::FieldRef> =
                        ::std::vec::Vec::new();
                    let mut columns: ::std::vec::Vec<::arrow_quiver::arrow::array::ArrayRef> =
                        ::std::vec::Vec::new();
                    #(#pushes)*
                    #push_extra
                    let schema = #schema;
                    ::arrow_quiver::arrow::record_batch::RecordBatch::try_new(
                        ::std::sync::Arc::new(schema),
                        columns,
                    )
                    .map_err(|err| ::arrow_quiver::Error {
                        record_type: #record_type,
                        kind: ::arrow_quiver::ErrorKind::BuildRecordBatch(err),
                    })
                }
            }
        }
    }
}

impl ColumnField {
    /// Generates `let #ident = …;`, extracting the column from `batch`.
    fn extractor(&self, record_type: &str) -> TokenStream {
        let Self {
            ident,
            column_name,
            optional,
            non_null: _, // handled by `null_check`
            kind,
        } = self;

        // `array` is a `&ArrayRef`:
        let convert = match kind {
            ColumnKind::Any => quote! { ::std::sync::Arc::clone(array) },
            ColumnKind::Typed {
                array_type,
                datatype,
            } => {
                let downcast = downcast(record_type, column_name, array_type, "a matching array");
                quote! {
                    {
                        let actual = ::arrow_quiver::arrow::array::Array::data_type(&**array);
                        if actual != &#datatype {
                            return ::core::result::Result::Err(::arrow_quiver::Error {
                                record_type: #record_type,
                                kind: ::arrow_quiver::ErrorKind::WrongDatatype {
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
            } => downcast(record_type, column_name, array_type, type_name),
        };

        let null_check = self.null_check(record_type);

        if *optional {
            quote! {
                let #ident = match batch.column_by_name(#column_name) {
                    ::core::option::Option::Some(array) => {
                        let array = #convert;
                        #null_check
                        ::core::option::Option::Some(array)
                    }
                    ::core::option::Option::None => ::core::option::Option::None,
                };
            }
        } else {
            quote! {
                let #ident = {
                    let array = batch
                        .column_by_name(#column_name)
                        .ok_or_else(|| ::arrow_quiver::Error {
                            record_type: #record_type,
                            kind: ::arrow_quiver::ErrorKind::MissingColumn {
                                column: #column_name.to_owned(),
                            },
                        })?;
                    let array = #convert;
                    #null_check
                    array
                };
            }
        }
    }

    /// Generates a check that `array` contains no nulls, if the field is marked `#[quiver(non_null)]`.
    fn null_check(&self, record_type: &str) -> Option<TokenStream> {
        let Self {
            column_name,
            non_null,
            ..
        } = self;

        non_null.then(|| {
            quote! {
                let null_count = ::arrow_quiver::arrow::array::Array::null_count(&array);
                if 0 < null_count {
                    return ::core::result::Result::Err(::arrow_quiver::Error {
                        record_type: #record_type,
                        kind: ::arrow_quiver::ErrorKind::UnexpectedNulls {
                            column: #column_name.to_owned(),
                            null_count,
                        },
                    });
                }
            }
        })
    }

    /// Generates code pushing this column of `value` onto `fields` and `columns`.
    fn push(&self, record_type: &str) -> TokenStream {
        let Self {
            ident,
            column_name,
            optional,
            non_null,
            kind,
        } = self;

        let nullable = !non_null;
        let null_check = self.null_check(record_type);

        // `array` is the (typed) array by value:
        let push_one = match kind {
            ColumnKind::Any => quote! {
                fields.push(::std::sync::Arc::new(::arrow_quiver::arrow::datatypes::Field::new(
                    #column_name,
                    ::arrow_quiver::arrow::array::Array::data_type(&array).clone(),
                    #nullable,
                )));
                columns.push(array);
            },
            ColumnKind::Typed { datatype, .. } => quote! {
                fields.push(::std::sync::Arc::new(::arrow_quiver::arrow::datatypes::Field::new(
                    #column_name,
                    #datatype,
                    #nullable,
                )));
                columns.push(::std::sync::Arc::new(array));
            },
            ColumnKind::Downcast { .. } => quote! {
                fields.push(::std::sync::Arc::new(::arrow_quiver::arrow::datatypes::Field::new(
                    #column_name,
                    ::arrow_quiver::arrow::array::Array::data_type(&array).clone(),
                    #nullable,
                )));
                columns.push(::std::sync::Arc::new(array));
            },
        };

        if *optional {
            quote! {
                if let ::core::option::Option::Some(array) = value.#ident {
                    #null_check
                    #push_one
                }
            }
        } else {
            quote! {
                {
                    let array = value.#ident;
                    #null_check
                    #push_one
                }
            }
        }
    }
}

/// Generates an expression downcasting `array` (a `&ArrayRef`) to `array_type`.
fn downcast(
    record_type: &str,
    column_name: &str,
    array_type: &syn::Type,
    expected: &str,
) -> TokenStream {
    quote! {
        ::arrow_quiver::arrow::array::Array::as_any(&**array)
            .downcast_ref::<#array_type>()
            .ok_or_else(|| ::arrow_quiver::Error {
                record_type: #record_type,
                kind: ::arrow_quiver::ErrorKind::WrongArrayType {
                    column: #column_name.to_owned(),
                    expected: #expected.to_owned(),
                    actual: ::arrow_quiver::arrow::array::Array::data_type(&**array).clone(),
                },
            })?
            .clone()
    }
}

/// Splits an optional `Option` wrapper from the inner array type.
fn classify_type(ty: &syn::Type) -> syn::Result<(bool, ColumnKind)> {
    if let Some(inner) = option_inner(ty) {
        Ok((true, classify_array_type(inner)?))
    } else {
        Ok((false, classify_array_type(ty)?))
    }
}

fn classify_array_type(ty: &syn::Type) -> syn::Result<ColumnKind> {
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
    } else if let Some(datatype) = datatype_of_array(&type_name) {
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
            | "ListArray"
            | "StructArray"
    )
}

/// Difficult and exotic array types we explicitly do not support (yet).
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
            | "MapArray"
            | "RunArray"
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
fn datatype_of_array(array_type_name: &str) -> Option<TokenStream> {
    let datatype = quote! { ::arrow_quiver::arrow::datatypes::DataType };
    let timestamp = |unit: TokenStream| {
        quote! {
            #datatype::Timestamp(
                ::arrow_quiver::arrow::datatypes::TimeUnit::#unit,
                ::core::option::Option::None,
            )
        }
    };

    let time_unit = |unit: TokenStream| {
        quote! { ::arrow_quiver::arrow::datatypes::TimeUnit::#unit }
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
