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
/// # use quiver::{ColumnDesc, Utf8};
/// const SENSOR: ColumnDesc<Utf8> = ColumnDesc::new("Measurements", "sensor");
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
/// A descriptor is the *name and logical type* of a column, holding no data.
/// The type parameter is the logical type, e.g. `ColumnDesc<Utf8>`, because a
/// descriptor yields both of the data types: it can
/// [`extract`](ColumnDesc::extract) a [`Column<L>`](Column) from a record batch
/// (metadata included), or validate a bare arrow array into a
/// [`TypedArray<L>`](TypedArray) with
/// [`typed_array`](ColumnDesc::typed_array) — in both cases without you naming
/// the column or its type at the call site.
/// [`arrow_field`](ColumnDesc::arrow_field) goes the other way, describing the
/// column to arrow.
///
/// Dynamically-typed columns get a [`DynColumnDesc`] and a [`DynColumn`] instead.
pub struct ColumnDesc<L> {
    /// What owns the column — the `#[derive(Quiver)]` struct, or whatever you
    /// want error messages to name.
    pub record_type: &'static str,

    /// The name of the column in the record batch.
    pub name: &'static str,

    /// The column metadata; the derive fills this in from
    /// `#[quiver(metadata("key" = "value", …))]`.
    pub metadata: &'static [(&'static str, &'static str)],

    _marker: PhantomData<fn() -> L>,
}

// Hand-written rather than derived: `#[derive]` would put the trait's own bound
// on `L`, but a descriptor holds no `L` — only a `PhantomData<fn() -> L>`, which
// is `Copy`, `Eq`, and `Debug` whatever `L` is.
impl<L> Clone for ColumnDesc<L> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<L> Copy for ColumnDesc<L> {}

impl<L> std::fmt::Debug for ColumnDesc<L> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self {
            record_type,
            name,
            metadata,
            _marker,
        } = self;
        f.debug_struct("ColumnDesc")
            .field("record_type", record_type)
            .field("name", name)
            .field("metadata", metadata)
            .finish()
    }
}

/// The declared metadata is compared as the map it becomes on the arrow field,
/// not as the slice it is written as: the order of the keys does not matter, and
/// a repeated key takes its last value — so two descriptors compare equal
/// exactly when their [`arrow_field`](ColumnDesc::arrow_field)s do.
impl<L> PartialEq for ColumnDesc<L> {
    fn eq(&self, other: &Self) -> bool {
        let Self {
            record_type,
            name,
            metadata,
            _marker,
        } = self;
        *record_type == other.record_type
            && *name == other.name
            && metadata_eq(metadata, other.metadata)
    }
}

/// Compares two declared-metadata slices as maps; see [`ColumnDesc`]'s
/// [`PartialEq`].
fn metadata_eq(left: &[(&str, &str)], right: &[(&str, &str)]) -> bool {
    // The last value of a repeated key wins, exactly as collecting the pairs
    // into the arrow field's map does.
    fn value_of<'a>(pairs: &[(&str, &'a str)], key: &str) -> Option<&'a str> {
        pairs
            .iter()
            .rev()
            .find_map(|(candidate, value)| (*candidate == key).then_some(*value))
    }

    left.iter()
        .chain(right)
        .all(|(key, _)| value_of(left, key) == value_of(right, key))
}

impl<L> Eq for ColumnDesc<L> {}

impl<L: LogicalType> ColumnDesc<L> {
    /// Describes the column `name` of `record_type`, which labels the errors
    /// (the name of the `#[derive(Quiver)]` struct, when there is one).
    ///
    /// `const`, so a descriptor can live in a constant, exactly like the
    /// `COLUMN_*` constants the derive generates.
    /// For a column with metadata, see [`ColumnDesc::new_with_metadata`].
    #[must_use]
    pub const fn new(record_type: &'static str, name: &'static str) -> Self {
        Self::new_with_metadata(record_type, name, &[])
    }

    /// Like [`ColumnDesc::new`], plus the column metadata that
    /// [`arrow_field`](ColumnDesc::arrow_field) puts on the arrow field.
    #[must_use]
    pub const fn new_with_metadata(
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

    /// The same column, read and declared as nullable.
    ///
    /// A column can be declared non-nullable and still hold nulls on some code
    /// path — concatenating a batch that has the column with one that does not,
    /// for instance. `optional` gives you a descriptor that reads such a batch,
    /// carrying the [`record_type`](ColumnDesc::record_type),
    /// [`name`](ColumnDesc::name), and [`metadata`](ColumnDesc::metadata) over,
    /// so the name stays single-sourced.
    ///
    /// Nullability is idempotent: `ColumnDesc<Option<L>>` is its own
    /// `optional()`, so this never nests into `Option<Option<…>>`.
    /// [`required`](ColumnDesc::required) goes the other way.
    ///
    /// ```
    /// # use quiver::{Binary, ColumnDesc};
    /// const CHUNK_KEY: ColumnDesc<Binary> = ColumnDesc::new("Manifest", "chunk_key");
    ///
    /// # let batch = quiver::arrow::record_batch::RecordBatch::try_from_iter([(
    /// #     "chunk_key",
    /// #     std::sync::Arc::new(quiver::arrow::array::BinaryArray::from(
    /// #         vec![Some(b"k".as_slice()), None],
    /// #     )) as quiver::arrow::array::ArrayRef,
    /// # )])?;
    /// // The strict descriptor rejects the nulls, the optional one reads them:
    /// assert!(CHUNK_KEY.extract(&batch).is_err());
    /// let keys: quiver::Column<Option<Binary>> = CHUNK_KEY.optional().extract(&batch)?;
    /// assert_eq!(keys.to_vec(), [Some(b"k".to_vec()), None]);
    ///
    /// // And it declares the column nullable, under the same name:
    /// assert!(CHUNK_KEY.optional().arrow_field().is_nullable());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    #[doc(alias = "nullable")]
    pub const fn optional(self) -> ColumnDesc<L::Optional> {
        let Self {
            record_type,
            name,
            metadata,
            _marker,
        } = self;
        ColumnDesc::new_with_metadata(record_type, name, metadata)
    }

    /// The same column, read and declared as non-nullable.
    ///
    /// The inverse of [`optional`](ColumnDesc::optional), for the code paths
    /// where a column declared nullable is known to be filled in, and you want
    /// values rather than `Option`s — with the same
    /// [`record_type`](ColumnDesc::record_type), [`name`](ColumnDesc::name),
    /// and [`metadata`](ColumnDesc::metadata). Every `Option` layer comes off,
    /// so this is idempotent too.
    ///
    /// Nothing is checked here: it is [`extract`](ColumnDesc::extract) that
    /// errors, with [`ErrorKind::UnexpectedNulls`], if the column does hold
    /// nulls after all.
    ///
    /// ```
    /// # use quiver::{ColumnDesc, Utf8};
    /// const NAME: ColumnDesc<Option<Utf8>> = ColumnDesc::new("Person", "name");
    ///
    /// # let batch = quiver::arrow::record_batch::RecordBatch::try_from_iter([(
    /// #     "name",
    /// #     std::sync::Arc::new(quiver::arrow::array::StringArray::from(vec!["Alice"]))
    /// #         as quiver::arrow::array::ArrayRef,
    /// # )])?;
    /// let names: quiver::Column<Utf8> = NAME.required().extract(&batch)?;
    /// assert_eq!(names.value(0), "Alice"); // not `Some("Alice")`
    /// assert!(!NAME.required().arrow_field().is_nullable());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    #[doc(alias = "non_nullable")]
    pub const fn required(self) -> ColumnDesc<L::Required> {
        let Self {
            record_type,
            name,
            metadata,
            _marker,
        } = self;
        ColumnDesc::new_with_metadata(record_type, name, metadata)
    }

    /// Extracts and validates this single column of a record batch.
    ///
    /// # Errors
    /// Errors if the column is missing, has the wrong data type, or unexpected nulls.
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
    /// Errors on data type mismatch, or on nulls at any non-`Option` nesting level.
    pub fn typed_array(&self, array: ArrayRef) -> Result<TypedArray<L>, Error> {
        let Self {
            record_type, name, ..
        } = *self;

        TypedArray::try_new(array)
            .map_err(|err| Error::new(record_type, err.for_column(name.to_owned())))
    }

    /// The name of the column in the record batch.
    ///
    /// The same as the [`name`](Self::name) field, but as a method.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// The name of the column in the record batch.
    ///
    /// The same as the [`name`](Self::name) field and method,
    /// but returning a `String`, which can be more ergonomic in some situations.
    #[must_use]
    pub fn name_owned(&self) -> String {
        self.name.to_owned()
    }

    /// The declared [`metadata`](ColumnDesc::metadata), owned, in the shape
    /// arrow wants it — for [`arrow_field`](ColumnDesc::arrow_field), and for
    /// stamping it on a field you build yourself.
    #[must_use]
    pub fn arrow_metadata(&self) -> std::collections::HashMap<String, String> {
        self.metadata
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }
}

impl<L: crate::ConcreteType> ColumnDesc<L> {
    /// The exact arrow data type of this column.
    ///
    /// The same as [`Column::data_type`] and
    /// [`TypedArray::data_type`](crate::TypedArray::data_type), reachable from the
    /// descriptor so you don't have to name the logical type at the call site.
    ///
    /// ```
    /// # use quiver::{ColumnDesc, Utf8, arrow::datatypes::DataType};
    /// const SENSOR: ColumnDesc<Utf8> = ColumnDesc::new("Measurements", "sensor");
    /// assert_eq!(SENSOR.data_type(), DataType::Utf8);
    /// ```
    #[must_use]
    #[expect(
        clippy::unused_self,
        reason = "a method, so it can be called on a descriptor value without naming `L`"
    )]
    pub fn data_type(&self) -> arrow::datatypes::DataType {
        L::data_type()
    }

    /// The arrow field of this column, including the declared metadata.
    #[must_use]
    pub fn arrow_field(&self) -> arrow::datatypes::Field {
        arrow::datatypes::Field::new(self.name, L::data_type(), L::NULLABLE)
            .with_metadata(self.arrow_metadata())
    }

    /// The same as [`arrow_field`](ColumnDesc::arrow_field), as an `Arc`, for
    /// the arrow APIs that take a [`FieldRef`](arrow::datatypes::FieldRef).
    #[must_use]
    pub fn arrow_field_ref(&self) -> arrow::datatypes::FieldRef {
        // TODO(emilk): it would be nice if this just `Arc::clone`d an existing `FieldRef` instead of allocating a new one on each call.
        self.arrow_field().into()
    }
}

impl<L: crate::ConcreteType> ColumnDesc<L> {
    /// Builds this column from owned values; the fallible form of
    /// [`ColumnDesc::new_from_values`], needed only for fallible encodings
    /// (dictionary key overflow).
    ///
    /// The descriptor supplies the column name and the declared
    /// [`metadata`](ColumnDesc::metadata), so neither is repeated at the call
    /// site — handy for the `COLUMN_*` constants the derive generates.
    ///
    /// # Errors
    /// Errors if the encoding fails, e.g. too many distinct values
    /// for the dictionary key type.
    pub fn try_new_from_values(
        &self,
        values: impl IntoIterator<Item = impl Into<L::Owned>>,
    ) -> Result<Column<L>, crate::ColumnError> {
        Ok(Column::try_from_values(self.name, values)?.with_metadata(self.arrow_metadata()))
    }
}

impl<L: crate::InfallibleBuild> ColumnDesc<L> {
    /// Builds this column from owned values, under the descriptor's name and
    /// declared [`metadata`](ColumnDesc::metadata).
    ///
    /// ```
    /// # use quiver::{ColumnDesc, Utf8};
    /// const SENSOR: ColumnDesc<Utf8> = ColumnDesc::new("Measurements", "sensor");
    ///
    /// let sensors = SENSOR.new_from_values(["kitchen", "attic"]);
    /// assert_eq!(sensors.name(), "sensor");
    /// assert_eq!(sensors.to_vec(), ["kitchen", "attic"]);
    /// ```
    #[must_use]
    pub fn new_from_values(
        &self,
        values: impl IntoIterator<Item = impl Into<L::Owned>>,
    ) -> Column<L> {
        Column::from_values(self.name, values).with_metadata(self.arrow_metadata())
    }
}

impl<L: crate::ConcreteType> ColumnDesc<Option<L>> {
    /// A column of `len` nulls, carrying this descriptor's
    /// [`name`](ColumnDesc::name) and declared
    /// [`metadata`](ColumnDesc::metadata).
    ///
    /// Pads a record batch that is missing this column — typically reached
    /// through [`optional`](ColumnDesc::optional), so the name stays
    /// single-sourced. [`into_dyn`](Column::into_dyn) then pairs the data with
    /// the arrow field, ready to widen a record batch with:
    ///
    /// ```
    /// # use quiver::{Binary, ColumnDesc};
    /// const CHUNK_KEY: ColumnDesc<Binary> = ColumnDesc::new("Manifest", "chunk_key");
    ///
    /// let column = CHUNK_KEY.optional().new_null(3);
    /// assert_eq!(column.to_vec(), [None, None, None]);
    /// assert_eq!(column.name(), "chunk_key");
    ///
    /// let dyn_column = column.into_dyn();
    /// assert!(dyn_column.field().is_nullable());
    /// ```
    #[must_use]
    pub fn new_null(&self, len: usize) -> Column<Option<L>> {
        Column::<Option<L>>::new_null(self.name, len).with_metadata(self.arrow_metadata())
    }
}

impl<L: LogicalType> ColumnDesc<L> {
    /// Forgets the static type: the same column, described dynamically.
    ///
    /// The declared [`metadata`](ColumnDesc::metadata) is dropped, since
    /// [`DynColumnDesc`] does not carry any.
    #[must_use]
    pub const fn to_dyn(self) -> DynColumnDesc {
        DynColumnDesc::new(self.record_type, self.name)
    }
}

impl<L: LogicalType> From<&ColumnDesc<L>> for DynColumnDesc {
    fn from(desc: &ColumnDesc<L>) -> Self {
        desc.to_dyn()
    }
}

/// A descriptor is [`Copy`], so it converts by value too — the spelling
/// [`to_dyn`](ColumnDesc::to_dyn) already uses.
impl<L: LogicalType> From<ColumnDesc<L>> for DynColumnDesc {
    fn from(desc: ColumnDesc<L>) -> Self {
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
/// [`DynColumn`] (field plus array), checking nothing beyond what the record
/// batch already guarantees — no logical type is involved.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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

    /// The name of the column in the record batch.
    ///
    /// The same as the [`name`](Self::name) field, as a method.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// The name of the column in the record batch.
    ///
    /// The same as the [`name`](Self::name) field and method,
    /// but returning a `String`, which can be more ergonomic in some situations.
    #[must_use]
    pub fn name_owned(&self) -> String {
        self.name.to_owned()
    }

    /// Extracts this single column of a record batch.
    ///
    /// # Errors
    /// Errors if the column is missing.
    pub fn extract(&self, batch: &arrow::record_batch::RecordBatch) -> Result<DynColumn, Error> {
        let Self { record_type, name } = *self;

        let index = batch.schema_ref().index_of(name).map_err(|_not_found| {
            Error::new(
                record_type,
                ErrorKind::MissingColumn {
                    column: name.to_owned(),
                },
            )
        })?;

        // Unvalidated: a record batch has already checked that each column's
        // array matches the schema's field, data type and nullability both.
        Ok(DynColumn::new_unvalidated(
            std::sync::Arc::clone(&batch.schema_ref().fields()[index]),
            ArrayRef::clone(batch.column(index)),
        ))
    }
}
