//! Tests for `#[derive(Quiver)]`.

#![cfg(feature = "derive")]

use std::collections::BTreeMap;
use std::sync::Arc;

use quiver::arrow::array::{
    Array as _, ArrayRef, DictionaryArray, DurationNanosecondArray, FixedSizeBinaryArray,
    Int32Array, Int64Array, ListArray, StringArray, StructArray, TimestampNanosecondArray,
};
use quiver::arrow::datatypes::{DataType, Field, Int32Type, Schema as ArrowSchema};
use quiver::arrow::record_batch::RecordBatch;
use quiver::{DynColumn, Error, ErrorKind, List, Quiver, Utf8};

/// Important thing
#[derive(Quiver)]
struct Thing {
    /// …of the record-batch
    #[quiver(metadata)]
    metadata: BTreeMap<String, String>,

    /// Name
    name: StringArray,

    /// Date of birth
    dob: Option<TimestampNanosecondArray>,

    /// All columns not declared above
    #[quiver(extra_columns)]
    other_columns: Vec<DynColumn>,
}

/// No extra columns or metadata allowed.
#[derive(Quiver)]
struct Strict {
    name: StringArray,
}

#[derive(Quiver)]
struct Renamed {
    #[quiver(name = "special:name")]
    kind: StringArray,
}

#[derive(Quiver)]
struct Anything {
    anything: ArrayRef,
}

/// Columns whose datatype depends on runtime parameters.
#[derive(Quiver)]
struct Nested {
    list: ListArray,
    type_struct: StructArray,
    dictionary: DictionaryArray<Int32Type>,
    fixed_size_binary: FixedSizeBinaryArray,
    duration: Option<DurationNanosecondArray>,
}

/// Builds a record batch with the given columns, in order.
fn batch_of(columns: &[(&str, ArrayRef)]) -> RecordBatch {
    let fields: Vec<_> = columns
        .iter()
        .map(|(name, array)| Field::new(*name, array.data_type().clone(), true))
        .collect();
    let arrays: Vec<_> = columns.iter().map(|(_, array)| Arc::clone(array)).collect();
    RecordBatch::try_new(Arc::new(ArrowSchema::new(fields)), arrays)
        .expect("Bad columns passed to test helper")
}

#[test]
fn roundtrip_full() {
    let thing = Thing {
        metadata: BTreeMap::from([("key".to_owned(), "value".to_owned())]),
        name: StringArray::from(vec!["Alice", "Bob"]),
        dob: Some(TimestampNanosecondArray::from(vec![1, 2])),
        other_columns: vec![DynColumn {
            field: Arc::new(Field::new("age", DataType::Int64, true)),
            array: Arc::new(Int64Array::from(vec![30, 40])),
        }],
    };

    let batch = RecordBatch::try_from(thing).unwrap();
    assert_eq!(batch.num_columns(), 3);
    assert_eq!(batch.num_rows(), 2);
    assert_eq!(batch.schema_ref().metadata()["key"], "value");

    let thing = Thing::try_from(batch).unwrap();
    assert_eq!(thing.metadata["key"], "value");
    assert_eq!(thing.name, StringArray::from(vec!["Alice", "Bob"]));
    assert_eq!(thing.dob, Some(TimestampNanosecondArray::from(vec![1, 2])));
    assert_eq!(thing.other_columns.len(), 1);
    assert_eq!(thing.other_columns[0].field.name(), "age");
}

#[test]
fn roundtrip_without_optional_column() {
    let thing = Thing {
        metadata: BTreeMap::default(),
        name: StringArray::from(vec!["Alice"]),
        dob: None,
        other_columns: vec![],
    };

    let batch = RecordBatch::try_from(thing).unwrap();
    assert_eq!(batch.num_columns(), 1);

    let thing = Thing::try_from(batch).unwrap();
    assert_eq!(thing.dob, None);
    assert!(thing.other_columns.is_empty());
}

#[test]
fn missing_required_column() {
    let batch = batch_of(&[(
        "dob",
        Arc::new(TimestampNanosecondArray::from(vec![1])) as ArrayRef,
    )]);
    let result = Thing::try_from(batch);
    assert!(matches!(
        result,
        Err(Error {
            record_type: "Thing",
            kind: ErrorKind::MissingColumn { column },
        }) if column == "name"
    ));
}

#[test]
fn wrong_datatype() {
    let batch = batch_of(&[("name", Arc::new(Int64Array::from(vec![1])) as ArrayRef)]);
    let result = Strict::try_from(batch);
    assert!(matches!(
        result,
        Err(Error {
            record_type: "Strict",
            kind: ErrorKind::WrongDatatype {
                column,
                expected,
                actual: DataType::Int64,
            },
        }) if column == "name" && expected == "Utf8"
    ));
}

#[test]
fn raw_arrow_columns_are_dynamic_about_nulls() {
    // Raw arrow array fields make no nullability guarantees;
    // use `quiver::Column<…>` for compile-time guarantees.
    let batch = batch_of(&[(
        "name",
        Arc::new(StringArray::from(vec![Some("Alice"), None])) as ArrayRef,
    )]);
    let strict = Strict::try_from(batch).unwrap();
    assert_eq!(strict.name.null_count(), 1);
}

#[test]
fn unexpected_column() {
    let batch = batch_of(&[
        (
            "name",
            Arc::new(StringArray::from(vec!["Alice"])) as ArrayRef,
        ),
        ("age", Arc::new(Int64Array::from(vec![30])) as ArrayRef),
    ]);
    let result = Strict::try_from(batch);
    assert!(matches!(
        result,
        Err(Error {
            record_type: "Strict",
            kind: ErrorKind::UnexpectedColumn { column },
        }) if column == "age"
    ));
}

#[test]
fn extra_columns_are_collected() {
    let batch = batch_of(&[
        (
            "name",
            Arc::new(StringArray::from(vec!["Alice"])) as ArrayRef,
        ),
        ("age", Arc::new(Int64Array::from(vec![30])) as ArrayRef),
    ]);
    let thing = Thing::try_from(batch).unwrap();
    assert_eq!(thing.other_columns.len(), 1);
    assert_eq!(thing.other_columns[0].field.name(), "age");
    assert_eq!(thing.other_columns[0].field.data_type(), &DataType::Int64);
}

#[test]
fn renamed_column() {
    let renamed = Renamed {
        kind: StringArray::from(vec!["point"]),
    };

    let batch = RecordBatch::try_from(renamed).unwrap();
    assert_eq!(batch.schema_ref().field(0).name(), "special:name");

    let renamed = Renamed::try_from(batch).unwrap();
    assert_eq!(renamed.kind, StringArray::from(vec!["point"]));
}

#[test]
fn any_datatype() {
    let anything = Anything {
        anything: Arc::new(Int64Array::from(vec![1, 2, 3])),
    };

    let batch = RecordBatch::try_from(anything).unwrap();
    assert_eq!(
        batch.schema_ref().field(0).data_type(),
        &DataType::Int64,
        "The datatype should be taken from the array"
    );

    let anything = Anything::try_from(batch).unwrap();
    assert_eq!(anything.anything.len(), 3);
}

#[test]
fn roundtrip_nested_datatypes() {
    let list = ListArray::from_iter_primitive::<Int32Type, _, _>(vec![
        Some(vec![Some(1), Some(2)]),
        Some(vec![Some(3)]),
    ]);
    let type_struct = StructArray::from(vec![(
        Arc::new(Field::new("x", DataType::Int32, false)),
        Arc::new(Int32Array::from(vec![1, 2])) as ArrayRef,
    )]);
    let dictionary: DictionaryArray<Int32Type> = vec!["foo", "bar"]
        .into_iter()
        .collect::<DictionaryArray<_>>();

    let fixed_size_binary =
        FixedSizeBinaryArray::try_from_iter(vec![vec![1_u8, 2], vec![3, 4]].into_iter()).unwrap();

    let nested = Nested {
        list: list.clone(),
        type_struct: type_struct.clone(),
        dictionary: dictionary.clone(),
        fixed_size_binary: fixed_size_binary.clone(),
        duration: Some(DurationNanosecondArray::from(vec![10, 20])),
    };

    let batch = RecordBatch::try_from(nested).unwrap();
    assert_eq!(batch.num_columns(), 5);

    let nested = Nested::try_from(batch).unwrap();
    assert_eq!(nested.list, list);
    assert_eq!(nested.type_struct, type_struct);
    assert_eq!(nested.dictionary, dictionary);
    assert_eq!(nested.fixed_size_binary, fixed_size_binary);
    assert_eq!(
        nested.duration,
        Some(DurationNanosecondArray::from(vec![10, 20]))
    );
}

#[test]
fn wrong_array_type() {
    let batch = batch_of(&[
        ("list", Arc::new(Int64Array::from(vec![1])) as ArrayRef),
        (
            "type_struct",
            Arc::new(Int64Array::from(vec![1])) as ArrayRef,
        ),
        (
            "dictionary",
            Arc::new(Int64Array::from(vec![1])) as ArrayRef,
        ),
        (
            "fixed_size_binary",
            Arc::new(Int64Array::from(vec![1])) as ArrayRef,
        ),
    ]);
    let result = Nested::try_from(batch);
    assert!(matches!(
        result,
        Err(Error {
            record_type: "Nested",
            kind: ErrorKind::WrongArrayType {
                column,
                expected,
                actual: DataType::Int64,
            },
        }) if column == "list" && expected == "ListArray"
    ));
}

#[test]
fn column_length_mismatch() {
    let thing = Thing {
        metadata: BTreeMap::default(),
        name: StringArray::from(vec!["Alice", "Bob"]),
        dob: Some(TimestampNanosecondArray::from(vec![1, 2, 3])),
        other_columns: vec![],
    };
    let result = RecordBatch::try_from(thing);
    assert!(matches!(
        result,
        Err(Error {
            record_type: "Thing",
            kind: ErrorKind::BuildRecordBatch(_),
        })
    ));
}

#[test]
fn typed_column_nullability_is_emitted() {
    let typed = Typed {
        name: quiver::Column::try_new(Arc::new(StringArray::from(vec!["Alice"]))).unwrap(),
        maybe_age: quiver::Column::try_new(Arc::new(Int64Array::from(vec![30]))).unwrap(),
        tags: quiver::Column::try_new(string_list_array_of_one()).unwrap(),
        scores: None,
    };
    let batch = RecordBatch::try_from(typed).unwrap();
    let schema = batch.schema_ref();
    assert!(!schema.field_with_name("name").unwrap().is_nullable());
    assert!(schema.field_with_name("maybe_age").unwrap().is_nullable());
}

/// A `List<Utf8>` array with non-nullable items: `[["a"]]`
fn string_list_array_of_one() -> ArrayRef {
    let values = StringArray::from(vec!["a"]);
    let offsets = quiver::arrow::buffer::OffsetBuffer::new(vec![0, 1].into());
    let field = Arc::new(Field::new("item", DataType::Utf8, false));
    Arc::new(ListArray::new(field, offsets, Arc::new(values), None))
}

#[test]
fn error_messages() {
    let err = Thing::try_from(batch_of(&[(
        "age",
        Arc::new(Int64Array::from(vec![30])) as ArrayRef,
    )]))
    .err()
    .unwrap();
    assert_eq!(
        err.to_string(),
        "Thing: Missing required column \"name\". \
         If the column is allowed to be missing, declare the field as `Option<…>`"
    );

    let err = Thing::try_from(batch_of(&[(
        "name",
        Arc::new(Int64Array::from(vec![30])) as ArrayRef,
    )]))
    .err()
    .unwrap();
    assert_eq!(
        err.to_string(),
        "Thing: Column \"name\": expected Utf8, found Int64"
    );

    let err = Strict::try_from(batch_of(&[
        (
            "name",
            Arc::new(StringArray::from(vec!["Alice"])) as ArrayRef,
        ),
        ("age", Arc::new(Int64Array::from(vec![30])) as ArrayRef),
    ]))
    .err()
    .unwrap();
    assert_eq!(
        err.to_string(),
        "Strict: Unexpected column \"age\". Either add it to the struct, \
         or accept unknown columns with a `#[quiver(extra_columns)]` field"
    );

    let err = Typed::try_from(batch_of(&[
        (
            "name",
            Arc::new(StringArray::from(vec![Some("Alice"), None])) as ArrayRef,
        ),
        (
            "maybe_age",
            Arc::new(Int64Array::from(vec![1, 2])) as ArrayRef,
        ),
        ("tags", string_list_array()),
    ]))
    .err()
    .unwrap();
    assert_eq!(
        err.to_string(),
        "Typed: Column \"name\" has 1 null(s) at a non-nullable level. \
         Use `Option<…>` in the logical type to allow nulls"
    );
}

/// Strongly-typed wrapper columns.
#[derive(Quiver)]
struct Typed {
    name: quiver::Column<Utf8>,
    maybe_age: quiver::Column<Option<i64>>,
    tags: quiver::Column<List<Utf8>>,
    scores: Option<quiver::Column<List<Option<f64>>>>,
}

#[test]
fn roundtrip_typed_columns() {
    let list = ListArray::from_iter_primitive::<quiver::arrow::datatypes::Float64Type, _, _>(vec![
        Some(vec![Some(1.0), None]),
        Some(vec![Some(3.0)]),
    ]);
    // `from_iter_primitive` marks the item field nullable, matching `List<Option<f64>>`.

    let typed = Typed {
        name: quiver::Column::try_new(Arc::new(StringArray::from(vec!["Alice", "Bob"]))).unwrap(),
        maybe_age: quiver::Column::try_new(Arc::new(Int64Array::from(vec![Some(30), None])))
            .unwrap(),
        tags: quiver::Column::try_new(string_list_array()).unwrap(),
        scores: Some(quiver::Column::try_new(Arc::new(list)).unwrap()),
    };

    let batch = RecordBatch::try_from(typed).unwrap();
    assert_eq!(batch.num_columns(), 4);

    let typed = Typed::try_from(batch).unwrap();

    let names: Vec<&str> = typed.name.iter().collect();
    assert_eq!(names, ["Alice", "Bob"]);

    let ages: Vec<Option<i64>> = typed.maybe_age.iter().collect();
    assert_eq!(ages, [Some(30), None]);

    let tags: Vec<Vec<&str>> = typed.tags.iter().map(Iterator::collect).collect();
    assert_eq!(tags, [vec!["a", "b"], vec!["c"]]);

    let scores: Vec<Vec<Option<f64>>> = typed
        .scores
        .unwrap()
        .iter()
        .map(Iterator::collect)
        .collect();
    assert_eq!(scores, [vec![Some(1.0), None], vec![Some(3.0)]]);
}

/// A `List<Utf8>` array with non-nullable items: `[["a", "b"], ["c"]]`
fn string_list_array() -> ArrayRef {
    let values = StringArray::from(vec!["a", "b", "c"]);
    let offsets = quiver::arrow::buffer::OffsetBuffer::new(vec![0, 2, 3].into());
    let field = Arc::new(Field::new("item", DataType::Utf8, false));
    Arc::new(ListArray::new(field, offsets, Arc::new(values), None))
}

#[test]
fn typed_column_rejects_nulls() {
    let batch = batch_of(&[
        (
            "name",
            Arc::new(StringArray::from(vec![Some("Alice"), None])) as ArrayRef,
        ),
        (
            "maybe_age",
            Arc::new(Int64Array::from(vec![1, 2])) as ArrayRef,
        ),
        ("tags", string_list_array()),
    ]);
    let result = Typed::try_from(batch);
    assert!(matches!(
        result,
        Err(Error {
            record_type: "Typed",
            kind: ErrorKind::UnexpectedNulls {
                column,
                null_count: 1,
            },
        }) if column == "name"
    ));
}

#[test]
fn typed_column_validates_inner_list_type() {
    // A List<Int64> where List<Utf8> is expected:
    let list =
        ListArray::from_iter_primitive::<quiver::arrow::datatypes::Int64Type, _, _>(vec![Some(
            vec![Some(1)],
        )]);
    let batch = batch_of(&[
        (
            "name",
            Arc::new(StringArray::from(vec!["Alice"])) as ArrayRef,
        ),
        ("maybe_age", Arc::new(Int64Array::from(vec![1])) as ArrayRef),
        ("tags", Arc::new(list) as ArrayRef),
    ]);
    let result = Typed::try_from(batch);
    assert!(matches!(
        result,
        Err(Error {
            record_type: "Typed",
            kind: ErrorKind::WrongDatatype { column, .. },
        }) if column == "tags"
    ));
}

#[derive(Quiver)]
struct Uuids {
    uuid: quiver::Column<quiver::FixedSizeBinary<16>>,
}

#[test]
fn roundtrip_fixed_size_binary() {
    let array = quiver::arrow::array::FixedSizeBinaryArray::try_from_iter(
        vec![[7_u8; 16], [8; 16]].into_iter(),
    )
    .unwrap();

    let uuids = Uuids {
        uuid: quiver::Column::try_new(Arc::new(array)).unwrap(),
    };

    let batch = RecordBatch::try_from(uuids).unwrap();
    assert_eq!(
        batch.schema_ref().field(0).data_type(),
        &DataType::FixedSizeBinary(16)
    );

    let uuids = Uuids::try_from(batch).unwrap();
    let values: Vec<&[u8; 16]> = uuids.uuid.iter().collect();
    assert_eq!(values, [&[7_u8; 16], &[8; 16]]);
}

#[derive(Quiver)]
struct Times {
    at: quiver::Column<quiver::Timestamp<quiver::Nanosecond, quiver::Utc>>,
}

#[test]
fn roundtrip_timestamp() {
    let array = TimestampNanosecondArray::from(vec![1, 2]).with_timezone("UTC");
    let times = Times {
        at: quiver::Column::try_new(Arc::new(array)).unwrap(),
    };

    let batch = RecordBatch::try_from(times).unwrap();
    assert_eq!(
        batch.schema_ref().field(0).data_type(),
        &DataType::Timestamp(
            quiver::arrow::datatypes::TimeUnit::Nanosecond,
            Some("UTC".into())
        )
    );

    let times = Times::try_from(batch).unwrap();
    let values: Vec<i64> = times.at.iter().collect();
    assert_eq!(values, [1, 2]);
}

#[test]
fn column_metadata_roundtrip() {
    let array = TimestampNanosecondArray::from(vec![1]).with_timezone("UTC");
    let times = Times {
        at: quiver::Column::try_new(Arc::new(array))
            .unwrap()
            .with_metadata(BTreeMap::from([("unit".to_owned(), "ns".to_owned())])),
    };

    let batch = RecordBatch::try_from(times).unwrap();
    assert_eq!(batch.schema_ref().field(0).metadata()["unit"], "ns");

    let times = Times::try_from(batch).unwrap();
    assert_eq!(times.at.metadata()["unit"], "ns");
}

#[test]
fn error_converts_to_arrow_error() {
    use quiver::arrow::error::ArrowError;

    // Most errors wrap as ExternalError, preserving the source chain:
    let err = Strict::try_from(batch_of(&[(
        "age",
        Arc::new(Int64Array::from(vec![1])) as ArrayRef,
    )]))
    .err()
    .unwrap();
    let arrow_err = ArrowError::from(err);
    assert!(matches!(arrow_err, ArrowError::ExternalError(_)));

    // …except BuildRecordBatch, which returns the original ArrowError:
    let thing = Thing {
        metadata: BTreeMap::default(),
        name: StringArray::from(vec!["Alice", "Bob"]),
        dob: Some(TimestampNanosecondArray::from(vec![1, 2, 3])),
        other_columns: vec![],
    };
    let err = RecordBatch::try_from(thing).err().unwrap();
    let arrow_err = ArrowError::from(err);
    assert!(matches!(arrow_err, ArrowError::InvalidArgumentError(_)));
}

#[test]
fn static_schema() {
    // All-static struct (quiver columns):
    let schema = Typed::max_schema();
    let name = schema.field_with_name("name").unwrap();
    assert_eq!(name.data_type(), &DataType::Utf8);
    assert!(!name.is_nullable());

    let maybe_age = schema.field_with_name("maybe_age").unwrap();
    assert_eq!(maybe_age.data_type(), &DataType::Int64);
    assert!(maybe_age.is_nullable());

    // Optional columns are included in the max schema, but not the min:
    assert!(schema.field_with_name("scores").is_ok());
    let min = Typed::min_schema();
    assert!(min.field_with_name("scores").is_err());
    assert!(min.field_with_name("name").is_ok());

    // Raw arrow arrays with an exact datatype are included, as nullable
    // (their nullability is not statically known):
    let schema = Strict::max_schema();
    let name = schema.field_with_name("name").unwrap();
    assert_eq!(name.data_type(), &DataType::Utf8);
    assert!(name.is_nullable());

    // Structs with dynamically-typed columns (ArrayRef, ListArray, …)
    // get no schema functions at all.
}

#[test]
fn try_from_record_batch_reference() {
    let strict = Strict {
        name: StringArray::from(vec!["Alice"]),
    };
    let batch = RecordBatch::try_from(strict).unwrap();

    // By reference — the batch stays usable:
    let strict = Strict::try_from(&batch).unwrap();
    assert_eq!(strict.name, StringArray::from(vec!["Alice"]));
    assert_eq!(batch.num_rows(), 1);
}

#[test]
fn into_record_batch() {
    let strict = Strict {
        name: StringArray::from(vec!["Alice"]),
    };
    let batch = strict.into_record_batch().unwrap();
    assert_eq!(batch.num_rows(), 1);
}

/// Documents a roundtrip asymmetry: extra columns are re-encoded *after*
/// the declared columns, even if they originally appeared before them.
#[test]
fn extra_columns_are_reordered_on_roundtrip() {
    let batch = batch_of(&[
        ("age", Arc::new(Int64Array::from(vec![30])) as ArrayRef), // extra, first
        (
            "name",
            Arc::new(StringArray::from(vec!["Alice"])) as ArrayRef,
        ),
    ]);

    let thing = Thing::try_from(batch).unwrap();
    let batch = thing.into_record_batch().unwrap();

    let names: Vec<&String> = batch
        .schema_ref()
        .fields()
        .iter()
        .map(|field| field.name())
        .collect();
    assert_eq!(names, ["name", "age"], "Declared columns come first");
}

#[test]
fn from_record_batch() {
    let batch = batch_of(&[(
        "name",
        Arc::new(StringArray::from(vec!["Alice"])) as ArrayRef,
    )]);
    let strict = Strict::from_record_batch(batch).unwrap();
    assert_eq!(strict.name, StringArray::from(vec!["Alice"]));
}

#[test]
fn column_descriptors() {
    // Names without hard-coding, honoring #[quiver(name = …)]:
    assert_eq!(Typed::COLUMN_NAME.name, "name");
    assert_eq!(Renamed::COLUMN_KIND.name, "special:name");

    let typed = Typed {
        name: quiver::Column::from_values(["Alice", "Bob"]),
        maybe_age: quiver::Column::from_values([Some(30_i64), None]),
        tags: quiver::Column::try_new(string_list_array()).unwrap(),
        scores: None,
    };
    let batch = typed.into_record_batch().unwrap();

    // Extract a single strongly-typed column:
    let ages = Typed::COLUMN_MAYBE_AGE.extract(&batch).unwrap();
    assert_eq!(ages.to_vec(), [Some(30), None]);

    // Missing column:
    let err = Typed::COLUMN_SCORES.extract(&batch).err().unwrap();
    assert!(matches!(
        err,
        Error {
            record_type: "Typed",
            kind: ErrorKind::MissingColumn { .. },
        }
    ));

    // Dynamically-typed columns get a DynColumnDesc:
    let strict = Strict {
        name: StringArray::from(vec!["Alice"]),
    };
    let batch = strict.into_record_batch().unwrap();
    let column = Strict::COLUMN_NAME.extract(&batch).unwrap();
    assert_eq!(column.field.name(), "name");
}

/// All columns required: unlike `Typed`, this gets `empty_record_batch`.
#[derive(Quiver)]
struct AllRequired {
    name: quiver::Column<Utf8>,
    maybe_age: quiver::Column<Option<i64>>,
    tags: quiver::Column<List<Utf8>>,
}

#[test]
fn empty_record_batch() {
    // `empty_record_batch` is only generated when all columns are required
    // (`min_schema() == max_schema()`) — `Typed` has an optional column,
    // so it does NOT get the fn (only `AllRequired` does).
    let batch = AllRequired::empty_record_batch();
    assert_eq!(batch.num_rows(), 0);
    assert_eq!(batch.num_columns(), 3);

    // The empty batch parses back:
    let parsed = AllRequired::try_from(batch).unwrap();
    assert!(parsed.name.is_empty());
    assert!(parsed.tags.is_empty());
}

/// Column matching is by name: the input column order never matters,
/// and encoding always emits struct declaration order.
#[test]
fn column_order_is_ignored() {
    let batch = batch_of(&[
        // Reverse of the declaration order:
        (
            "dob",
            Arc::new(TimestampNanosecondArray::from(vec![1])) as ArrayRef,
        ),
        (
            "name",
            Arc::new(StringArray::from(vec!["Alice"])) as ArrayRef,
        ),
    ]);

    let thing = Thing::try_from(batch).unwrap();
    assert_eq!(thing.name, StringArray::from(vec!["Alice"]));

    let batch = thing.into_record_batch().unwrap();
    let names: Vec<&String> = batch
        .schema_ref()
        .fields()
        .iter()
        .map(|field| field.name())
        .collect();
    assert_eq!(names, ["name", "dob"], "Declaration order on encode");
}

/// Unknown columns are silently ignored.
#[derive(Quiver)]
#[quiver(nonexhaustive)]
struct Lenient {
    name: StringArray,
}

/// Unknown columns are an error (explicit form of the default).
#[derive(Quiver)]
#[quiver(exhaustive)]
struct Exhaustive {
    name: StringArray,
}

#[test]
fn nonexhaustive_ignores_unknown_columns() {
    let batch = batch_of(&[
        (
            "name",
            Arc::new(StringArray::from(vec!["Alice"])) as ArrayRef,
        ),
        ("age", Arc::new(Int64Array::from(vec![30])) as ArrayRef),
    ]);

    let lenient = Lenient::try_from(&batch).unwrap();
    assert_eq!(lenient.name, StringArray::from(vec!["Alice"]));

    // The unknown column is dropped on the roundtrip:
    let batch = lenient.into_record_batch().unwrap();
    assert_eq!(batch.num_columns(), 1);
}

#[test]
fn exhaustive_rejects_unknown_columns() {
    let batch = batch_of(&[
        (
            "name",
            Arc::new(StringArray::from(vec!["Alice"])) as ArrayRef,
        ),
        ("age", Arc::new(Int64Array::from(vec![30])) as ArrayRef),
    ]);

    let result = Exhaustive::try_from(batch);
    assert!(matches!(
        result,
        Err(Error {
            record_type: "Exhaustive",
            kind: ErrorKind::UnexpectedColumn { column },
        }) if column == "age"
    ));
}

/// Columns with declared (`#[quiver(metadata(…))]`) field metadata.
#[derive(Quiver)]
struct Annotated {
    #[quiver(metadata("meta:kind" = "control"))]
    chunk_id: quiver::Column<quiver::FixedSizeBinary<16>>,

    #[quiver(
        metadata("meta:kind" = "index", "meta:index_marker" = "start"),
        name = "frame_nr"
    )]
    frame_start: quiver::Column<Option<i64>>,

    /// Declared metadata also works on raw arrow array fields:
    #[quiver(metadata("raw" = "yes"))]
    comment: StringArray,
}

fn annotated() -> Annotated {
    Annotated {
        chunk_id: quiver::Column::from_values([[1_u8; 16]]),
        frame_start: quiver::Column::from_values([Some(7_i64)]),
        comment: StringArray::from(vec!["hi"]),
    }
}

#[test]
fn declared_metadata_is_encoded() {
    let batch = annotated().into_record_batch().unwrap();
    let schema = batch.schema_ref();
    assert_eq!(
        schema.field_with_name("chunk_id").unwrap().metadata()["meta:kind"],
        "control"
    );
    let frame_nr = schema.field_with_name("frame_nr").unwrap().metadata();
    assert_eq!(frame_nr["meta:kind"], "index");
    assert_eq!(frame_nr["meta:index_marker"], "start");
    assert_eq!(
        schema.field_with_name("comment").unwrap().metadata()["raw"],
        "yes"
    );
}

#[test]
fn declared_metadata_is_not_validated_when_parsing() {
    // A batch without any field metadata parses fine…
    let batch = batch_of(&[
        (
            "chunk_id",
            Arc::new(FixedSizeBinaryArray::try_from_iter(vec![[1_u8; 16]].into_iter()).unwrap())
                as ArrayRef,
        ),
        ("frame_nr", Arc::new(Int64Array::from(vec![7])) as ArrayRef),
        (
            "comment",
            Arc::new(StringArray::from(vec!["hi"])) as ArrayRef,
        ),
    ]);
    let annotated = Annotated::try_from(batch).unwrap();
    assert!(annotated.chunk_id.metadata().is_empty());

    // …and re-encoding re-stamps the declared metadata (normalization):
    let batch = annotated.into_record_batch().unwrap();
    assert_eq!(
        batch
            .schema_ref()
            .field_with_name("chunk_id")
            .unwrap()
            .metadata()["meta:kind"],
        "control"
    );
}

#[test]
fn declared_metadata_merges_with_instance_metadata() {
    let mut annotated = annotated();
    annotated.chunk_id.metadata_mut().extend([
        ("meta:kind".to_owned(), "override".to_owned()), // conflicts: instance wins
        ("unit".to_owned(), "ids".to_owned()),           // disjoint: union
    ]);

    let batch = annotated.into_record_batch().unwrap();
    let metadata = batch
        .schema_ref()
        .field_with_name("chunk_id")
        .unwrap()
        .metadata()
        .clone();
    assert_eq!(metadata["meta:kind"], "override");
    assert_eq!(metadata["unit"], "ids");
}

#[test]
fn declared_metadata_in_static_schema() {
    let schema = Annotated::max_schema();
    let expected = Field::new("chunk_id", DataType::FixedSizeBinary(16), false)
        .with_metadata(std::iter::once(("meta:kind".to_owned(), "control".to_owned())).collect());
    assert_eq!(schema.field_with_name("chunk_id").unwrap(), &expected);

    // The COLUMN_* descriptor exposes it too:
    assert_eq!(
        Annotated::COLUMN_CHUNK_ID.metadata,
        [("meta:kind", "control")]
    );
    assert_eq!(Annotated::COLUMN_CHUNK_ID.arrow_field(), expected);
}

/// The generated code refers to the crate via `#[quiver(crate = "…")]`,
/// for renamed dependencies and re-exports
/// (proc-macros have no `$crate` equivalent).
mod crate_path_override {
    use quiver as renamed_quiver;

    #[derive(renamed_quiver::Quiver)]
    #[quiver(crate = "renamed_quiver")]
    struct Thing {
        x: renamed_quiver::Column<i64>,
    }

    #[test]
    fn crate_path_override() {
        let thing = Thing {
            x: renamed_quiver::Column::from_values([1, 2]),
        };
        let batch = thing.into_record_batch().unwrap();
        let thing = Thing::try_from(batch).unwrap();
        assert_eq!(thing.x.to_vec(), [1, 2]);
        assert_eq!(Thing::COLUMN_X.name, "x");
    }
}

#[test]
fn column_name_constants_in_patterns() {
    let batch = annotated().into_record_batch().unwrap();

    let mut kinds = Vec::new();
    for field in batch.schema_ref().fields() {
        // Plain consts are valid match patterns (COLUMN_*.name would not be):
        kinds.push(match field.name().as_str() {
            Annotated::COLUMN_CHUNK_ID_NAME => "control",
            Annotated::COLUMN_FRAME_START_NAME => "index",
            _ => "other",
        });
    }
    assert_eq!(kinds, ["control", "index", "other"]);

    // The descriptor's name is the same constant:
    assert_eq!(
        Annotated::COLUMN_CHUNK_ID.name,
        Annotated::COLUMN_CHUNK_ID_NAME
    );
    // Renames are honored:
    assert_eq!(Annotated::COLUMN_FRAME_START_NAME, "frame_nr");
}
