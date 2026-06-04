//! Tests for `#[derive(Quiver)]`.

#![cfg(feature = "derive")]

use std::collections::BTreeMap;
use std::sync::Arc;

use arrow_quiver::arrow::array::{
    Array as _, ArrayRef, DictionaryArray, DurationNanosecondArray, FixedSizeBinaryArray,
    Int32Array, Int64Array, ListArray, StringArray, StructArray, TimestampNanosecondArray,
};
use arrow_quiver::arrow::datatypes::{DataType, Field, Int32Type, Schema as ArrowSchema};
use arrow_quiver::arrow::record_batch::RecordBatch;
use arrow_quiver::{DynColumn, Error, ErrorKind, List, Quiver};

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
                expected: DataType::Utf8,
                actual: DataType::Int64,
            },
        }) if column == "name"
    ));
}

#[test]
fn raw_arrow_columns_are_dynamic_about_nulls() {
    // Raw arrow array fields make no nullability guarantees;
    // use `arrow_quiver::Column<…>` for compile-time guarantees.
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
        name: arrow_quiver::Column::try_new(Arc::new(StringArray::from(vec!["Alice"]))).unwrap(),
        maybe_age: arrow_quiver::Column::try_new(Arc::new(Int64Array::from(vec![30]))).unwrap(),
        tags: arrow_quiver::Column::try_new(string_list_array_of_one()).unwrap(),
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
    let offsets = arrow_quiver::arrow::buffer::OffsetBuffer::new(vec![0, 1].into());
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
        "Thing: Column \"name\": expected datatype Utf8, found Int64"
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
    name: arrow_quiver::Column<String>,
    maybe_age: arrow_quiver::Column<Option<i64>>,
    tags: arrow_quiver::Column<List<String>>,
    scores: Option<arrow_quiver::Column<List<Option<f64>>>>,
}

#[test]
fn roundtrip_typed_columns() {
    let list =
        ListArray::from_iter_primitive::<arrow_quiver::arrow::datatypes::Float64Type, _, _>(vec![
            Some(vec![Some(1.0), None]),
            Some(vec![Some(3.0)]),
        ]);
    // `from_iter_primitive` marks the item field nullable, matching `List<Option<f64>>`.

    let typed = Typed {
        name: arrow_quiver::Column::try_new(Arc::new(StringArray::from(vec!["Alice", "Bob"])))
            .unwrap(),
        maybe_age: arrow_quiver::Column::try_new(Arc::new(Int64Array::from(vec![Some(30), None])))
            .unwrap(),
        tags: arrow_quiver::Column::try_new(string_list_array()).unwrap(),
        scores: Some(arrow_quiver::Column::try_new(Arc::new(list)).unwrap()),
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
    let offsets = arrow_quiver::arrow::buffer::OffsetBuffer::new(vec![0, 2, 3].into());
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
        ListArray::from_iter_primitive::<arrow_quiver::arrow::datatypes::Int64Type, _, _>(vec![
            Some(vec![Some(1)]),
        ]);
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
    uuid: arrow_quiver::Column<[u8; 16]>,
}

#[test]
fn roundtrip_fixed_size_binary() {
    let array = arrow_quiver::arrow::array::FixedSizeBinaryArray::try_from_iter(
        vec![[7_u8; 16], [8; 16]].into_iter(),
    )
    .unwrap();

    let uuids = Uuids {
        uuid: arrow_quiver::Column::try_new(Arc::new(array)).unwrap(),
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
