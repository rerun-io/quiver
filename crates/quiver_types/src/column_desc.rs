//! Column descriptors: extracting single, named columns from a record batch.
//!
//! `#[derive(Quiver)]` generates these as `COLUMN_*` constants, and they are
//! equally usable standalone: [`ColumnDesc::new`] is a `const fn`.

use std::marker::PhantomData;

use arrow::array::ArrayRef;

use crate::{Column, DynColumn, Error, ErrorKind, LogicalType, TypedArray};

// Column descriptors

/// Identifies a strongly-typed column by name.
///
/// `#[derive(Quiver)]` generates one per field, as `COLUMN_*` constants, e.g.
/// `Measurements::COLUMN_TEMPERATURE` — but a descriptor is an ordinary value:
/// declare your own with [`ColumnDesc::new`], in a `const` if you like, and use
/// it with no derive in sight.
///
/// ```
/// # use quiver::{Column, ColumnDesc, Utf8};
/// const SENSOR: ColumnDesc<Column<Utf8>> = ColumnDesc::new("Measurements", "sensor", &[]);
///
/// # let batch = quiver::arrow::record_batch::RecordBatch::try_from_iter([(
/// #     "sensor",
/// #     std::sync::Arc::new(quiver::arrow::array::StringArray::from(vec!["a"]))
/// #         as quiver::arrow::array::ArrayRef,
/// # )])?;
/// let sensors = SENSOR.extract(&batch)?;
/// assert_eq!(sensors.value(0), "a");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// # Relationship to the other main types
/// A descriptor is the *name and type* of a column, holding no data. It carries
/// the logical type `L` of its [`Column<L>`](Column), so it can
/// [`extract`](ColumnDesc::extract) one from a record batch (metadata included),
/// or validate a bare arrow array into a [`TypedArray<L>`](TypedArray) with
/// [`typed_array`](ColumnDesc::typed_array) — in both cases without you naming
/// the column or its type at the call site.
/// [`arrow_field`](ColumnDesc::arrow_field) goes the other way, describing the
/// column to arrow.
///
/// Dynamically-typed columns get a [`DynColumnDesc`] and a [`DynColumn`] instead.
pub struct ColumnDesc<C> {
    /// What owns the column — the `#[derive(Quiver)]` struct, or whatever you
    /// want error messages to name.
    pub record_type: &'static str,

    /// The name of the column in the record batch.
    pub name: &'static str,

    /// The column metadata; the derive fills this in from
    /// `#[quiver(metadata("key" = "value", …))]`.
    pub metadata: &'static [(&'static str, &'static str)],

    _marker: PhantomData<fn() -> C>,
}

impl<L: LogicalType> ColumnDesc<Column<L>> {
    /// Describes the column `name` of `record_type`, which labels the errors
    /// (the name of the `#[derive(Quiver)]` struct, when there is one).
    ///
    /// `const`, so a descriptor can live in a constant, exactly like the
    /// `COLUMN_*` constants the derive generates. Pass `&[]` for no metadata.
    #[must_use]
    pub const fn new(
        record_type: &'static str,
        name: &'static str,
        metadata: &'static [(&'static str, &'static str)],
    ) -> Self {
        Self {
            record_type,
            name,
            metadata,
            _marker: PhantomData,
        }
    }

    /// Extracts and validates this single column of a record batch.
    ///
    /// # Errors
    /// Errors if the column is missing, has the wrong datatype, or unexpected nulls.
    pub fn extract(&self, batch: &arrow::record_batch::RecordBatch) -> Result<Column<L>, Error> {
        let Self {
            record_type, name, ..
        } = *self;
        Column::extract_named(batch, name, record_type)
    }

    /// Validates a loose arrow array against this column's logical type,
    /// then downcasts it (zero-copy).
    ///
    /// The descriptor supplies the logical type, so you don't have to name it,
    /// and labels any error with the column and record type.
    ///
    /// The result is a [`TypedArray`], not a [`Column`]: a bare array carries no
    /// field metadata. Use [`ColumnDesc::extract`] when you have the whole
    /// record batch, and want the metadata too.
    ///
    /// # Errors
    /// Errors on datatype mismatch, or on nulls at any non-`Option` nesting level.
    pub fn typed_array(&self, array: ArrayRef) -> Result<TypedArray<L>, Error> {
        let Self {
            record_type, name, ..
        } = *self;

        TypedArray::try_new(array).map_err(|err| Error {
            record_type,
            kind: err.for_column(name.to_owned()),
        })
    }
}

impl<L: crate::ConcreteType> ColumnDesc<Column<L>> {
    /// The arrow field of this column, including the declared metadata.
    #[must_use]
    pub fn arrow_field(&self) -> arrow::datatypes::Field {
        arrow::datatypes::Field::new(self.name, L::datatype(), L::NULLABLE).with_metadata(
            self.metadata
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect(),
        )
    }
}

impl<C> ColumnDesc<C> {
    /// Forgets the static type: the same column, described dynamically.
    ///
    /// The declared [`metadata`](ColumnDesc::metadata) is dropped, since
    /// [`DynColumnDesc`] does not carry any.
    #[must_use]
    pub const fn to_dyn(&self) -> DynColumnDesc {
        DynColumnDesc::new(self.record_type, self.name)
    }
}

impl<C> From<&ColumnDesc<C>> for DynColumnDesc {
    fn from(desc: &ColumnDesc<C>) -> Self {
        desc.to_dyn()
    }
}

/// Identifies a dynamically-typed column by name.
///
/// `#[derive(Quiver)]` generates one per raw arrow array field, as a `COLUMN_*`
/// constant, but — like [`ColumnDesc`] — it works standalone too:
/// [`DynColumnDesc::new`] is `const`.
///
/// The untyped counterpart of [`ColumnDesc`]: it extracts a
/// [`DynColumn`] (field plus array), with no datatype or
/// nullability validation.
pub struct DynColumnDesc {
    /// The name of the `#[derive(Quiver)]` struct, for error messages.
    pub record_type: &'static str,

    /// The name of the column in the record batch.
    pub name: &'static str,
}

impl DynColumnDesc {
    /// Describes the column `name` of `record_type`, which labels the errors
    /// (the name of the `#[derive(Quiver)]` struct, when there is one).
    #[must_use]
    pub const fn new(record_type: &'static str, name: &'static str) -> Self {
        Self { record_type, name }
    }

    /// Extracts this single column of a record batch.
    ///
    /// # Errors
    /// Errors if the column is missing.
    pub fn extract(&self, batch: &arrow::record_batch::RecordBatch) -> Result<DynColumn, Error> {
        let Self { record_type, name } = *self;

        let index = batch
            .schema_ref()
            .index_of(name)
            .map_err(|_not_found| Error {
                record_type,
                kind: ErrorKind::MissingColumn {
                    column: name.to_owned(),
                },
            })?;

        Ok(DynColumn {
            field: std::sync::Arc::clone(&batch.schema_ref().fields()[index]),
            array: ArrayRef::clone(batch.column(index)),
        })
    }
}
