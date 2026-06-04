//! Tests for `#[derive(Quiver)]`.

#![cfg(feature = "derive")]

use std::collections::BTreeMap;
use std::sync::Arc;

use arrow_quiver::arrow::array::{
    Array as _, ArrayRef, Int64Array, StringArray, TimestampNanosecondArray,
};
use arrow_quiver::arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use arrow_quiver::arrow::record_batch::RecordBatch;
use arrow_quiver::{Column, Error, Quiver};

/// Important thing
#[derive(Quiver)]
struct Thing {
    /// …of the record-batch
    #[quiver(metadata)]
    metadata: BTreeMap<String, String>,

    /// Name
    #[quiver(non_null)]
    name: StringArray,

    /// Date of birth
    dob: Option<TimestampNanosecondArray>,

    /// All columns not declared above
    #[quiver(extra_columns)]
    other_columns: Vec<Column>,
}

/// No extra columns or metadata allowed.
#[derive(Quiver)]
struct Strict {
    #[quiver(non_null)]
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
        other_columns: vec![Column {
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
    assert!(matches!(result, Err(Error::MissingColumn { column }) if column == "name"));
}

#[test]
fn wrong_datatype() {
    let batch = batch_of(&[("name", Arc::new(Int64Array::from(vec![1])) as ArrayRef)]);
    let result = Strict::try_from(batch);
    assert!(matches!(
        result,
        Err(Error::WrongDatatype {
            column,
            expected: DataType::Utf8,
            actual: DataType::Int64,
        }) if column == "name"
    ));
}

#[test]
fn nulls_in_non_null_column() {
    let batch = batch_of(&[(
        "name",
        Arc::new(StringArray::from(vec![Some("Alice"), None])) as ArrayRef,
    )]);
    let result = Strict::try_from(batch);
    assert!(matches!(
        result,
        Err(Error::UnexpectedNulls {
            column,
            null_count: 1,
        }) if column == "name"
    ));
}

#[test]
fn nulls_allowed_unless_non_null() {
    let batch = batch_of(&[(
        "anything",
        Arc::new(StringArray::from(vec![Some("Alice"), None])) as ArrayRef,
    )]);
    let anything = Anything::try_from(batch).unwrap();
    assert_eq!(anything.anything.null_count(), 1);
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
    assert!(matches!(result, Err(Error::UnexpectedColumn { column }) if column == "age"));
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
fn column_length_mismatch() {
    let thing = Thing {
        metadata: BTreeMap::default(),
        name: StringArray::from(vec!["Alice", "Bob"]),
        dob: Some(TimestampNanosecondArray::from(vec![1, 2, 3])),
        other_columns: vec![],
    };
    let result = RecordBatch::try_from(thing);
    assert!(matches!(result, Err(Error::Arrow(_))));
}

#[test]
fn non_null_column_is_emitted_as_non_nullable() {
    let strict = Strict {
        name: StringArray::from(vec!["Alice"]),
    };
    let batch = RecordBatch::try_from(strict).unwrap();
    let field = batch.schema_ref().field(0);
    assert!(!field.is_nullable());
}
