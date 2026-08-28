//! Tests for standalone use of [`quiver::Column`] — no derive macro involved.
//!
//! A column is a named record batch column: these tests cover the name, the
//! metadata, and the record batch boundary. Everything a column shares with a
//! bare array — element access, iteration, slicing, encodings — is tested on
//! [`quiver::TypedArray`], in `typed_array.rs`.

use std::collections::BTreeMap;
use std::sync::Arc;

use quiver::arrow::array::Array as _;
use quiver::arrow::array::{ArrayRef, Int64Array, StringArray};
use quiver::arrow::datatypes::{DataType, Field, Schema};
use quiver::arrow::record_batch::RecordBatch;
use quiver::{
    Column, ColumnDesc, DynColumn, DynColumnDesc, ErrorKind, FixedSizeBinary, List, TypedArray,
    Utf8,
};

#[test]
fn column_metadata() {
    let column = Column::<i64>::try_new("elapsed", Arc::new(Int64Array::from(vec![1])) as ArrayRef)
        .unwrap()
        .with_metadata(std::collections::BTreeMap::from([(
            "unit".to_owned(),
            "seconds".to_owned(),
        )]));
    assert_eq!(column.name(), "elapsed");
    assert_eq!(column.metadata()["unit"], "seconds");

    let mut column = column;
    column
        .metadata_mut()
        .insert("source".to_owned(), "sensor".to_owned());
    assert_eq!(column.metadata().len(), 2);
}

#[test]
fn new_null() {
    use quiver::{Binary, ColumnDesc, Dictionary, TypedArray};

    const CHUNK_KEY: ColumnDesc<Binary> =
        ColumnDesc::new_with_metadata("Manifest", "chunk_key", &[("sorted", "true")]);

    let array = TypedArray::<Option<Binary>>::new_null(3);
    assert_eq!(array.len(), 3);
    assert_eq!(array.to_vec(), [None, None, None]);
    assert_eq!(array.as_arrow().null_count(), 3);
    assert_eq!(TypedArray::<Option<Binary>>::data_type(), DataType::Binary);

    // Zero-length is the empty array, not a special case:
    assert_eq!(
        TypedArray::<Option<i64>>::new_null(0),
        TypedArray::default()
    );

    // Nesting: the nulls are at the outer level, the items are untouched:
    let array = TypedArray::<Option<List<i64>>>::new_null(2);
    assert_eq!(array.to_vec(), [None, None]);

    // …and at an inner level, through the item type:
    let array = TypedArray::<List<Option<i64>>>::from_values([vec![None, Some(1)]]);
    assert_eq!(array.value(0).to_vec(), [None, Some(1)]);

    // Encodings whose *values* can fail to build still have an all-null form:
    let array = TypedArray::<Option<Dictionary<i32, Utf8>>>::new_null(2);
    assert_eq!(array.to_vec(), [None, None]);

    // Fixed-size binary keeps its width:
    let array = TypedArray::<Option<FixedSizeBinary<4>>>::new_null(1);
    assert_eq!(
        TypedArray::<Option<FixedSizeBinary<4>>>::data_type(),
        DataType::FixedSizeBinary(4)
    );
    assert_eq!(array.to_vec(), [None]);

    // The `Column` form agrees, and is named:
    let column = Column::<Option<Binary>>::new_null("chunk_key", 3);
    assert_eq!(column.name(), "chunk_key");
    assert_eq!(
        column.into_typed_array(),
        TypedArray::<Option<Binary>>::new_null(3)
    );

    // Through a descriptor: the declared metadata comes along, and `into_dyn`
    // pairs the data with the nullable field, under the descriptor's name.
    let column = CHUNK_KEY.optional().new_null(2);
    assert_eq!(column.to_vec(), [None, None]);
    assert_eq!(column.metadata()["sorted"], "true");

    assert_eq!(CHUNK_KEY.arrow_metadata()["sorted"], "true");
    assert_eq!(
        CHUNK_KEY.arrow_metadata(),
        CHUNK_KEY.arrow_field().metadata().clone()
    );

    let dyn_column = column.into_dyn();
    assert_eq!(dyn_column.field().name(), "chunk_key");
    assert!(dyn_column.field().is_nullable());
    assert_eq!(dyn_column.field().metadata()["sorted"], "true");
    assert_eq!(dyn_column.array().len(), 2);
    let (_field, dyn_array) = dyn_column.into_parts();

    // It round-trips back through the same descriptor:
    let batch = RecordBatch::try_from_iter_with_nullable([("chunk_key", dyn_array, true)]).unwrap();
    assert_eq!(
        CHUNK_KEY.optional().extract(&batch).unwrap().to_vec(),
        [None, None]
    );
    // …but not through the strict one: nulls where none were declared.
    assert!(matches!(
        *CHUNK_KEY.extract(&batch).unwrap_err().kind,
        ErrorKind::UnexpectedNulls { .. }
    ));
}

#[test]
fn from_record_batch_and_name() {
    let names: ArrayRef = Arc::new(StringArray::from(vec!["alice", "bob"]));
    let ages: ArrayRef = Arc::new(Int64Array::from(vec![30, 40]));
    let schema = Schema::new(vec![
        Field::new("name", DataType::Utf8, false).with_metadata(std::collections::HashMap::from([
            ("pii".to_owned(), "true".to_owned()),
        ])),
        Field::new("age", DataType::Int64, false),
    ]);
    let batch = RecordBatch::try_new(Arc::new(schema), vec![names, ages]).unwrap();

    // Happy path: looks up by name, validates, and carries over the name and
    // the field metadata.
    let column = Column::<Utf8>::from_record_batch_and_name(&batch, "name").unwrap();
    assert_eq!(column.to_vec(), ["alice", "bob"]);
    assert_eq!(column.name(), "name");
    assert_eq!(column.metadata()["pii"], "true");

    // Missing column → a helpful `MissingColumn` error.
    let err = Column::<Utf8>::from_record_batch_and_name(&batch, "nope")
        .err()
        .unwrap();
    assert_eq!(err.record_type, "Column");
    assert!(matches!(*err.kind, ErrorKind::MissingColumn { column } if column == "nope"));

    // Present but wrong data type → a `WrongDataType` error, naming the column.
    let err = Column::<Utf8>::from_record_batch_and_name(&batch, "age")
        .err()
        .unwrap();
    assert_eq!(err.record_type, "Column");
    assert!(
        matches!(*err.kind, ErrorKind::WrongDataType { column, actual: DataType::Int64, .. } if column == "age")
    );
}

/// `TypedArray` is the one with `Default`; an empty column still needs a name.
#[test]
fn empty_column() {
    let column = Column::<List<Option<Utf8>>>::empty("tags");
    assert!(column.is_empty());
    assert_eq!(column.name(), "tags");
    assert!(column.metadata().is_empty());
    assert_eq!(column.into_typed_array(), TypedArray::default());
}

#[test]
fn static_data_type() {
    assert_eq!(Column::<i64>::data_type(), DataType::Int64);
    assert_eq!(Column::<Option<i64>>::data_type(), DataType::Int64); // Nullability is not part of the data type
    assert_eq!(
        Column::<List<Option<Utf8>>>::data_type(),
        DataType::List(Arc::new(Field::new("item", DataType::Utf8, true)))
    );
    assert_eq!(
        Column::<List<Utf8>>::data_type(),
        DataType::List(Arc::new(Field::new("item", DataType::Utf8, false)))
    );
    const {
        assert!(Column::<Option<i64>>::NULLABLE);
        assert!(!Column::<i64>::NULLABLE);
    }

    // A descriptor reports the same data type, without naming the logical type:
    let names: ColumnDesc<List<Option<Utf8>>> = ColumnDesc::new("Record", "names");
    assert_eq!(names.data_type(), Column::<List<Option<Utf8>>>::data_type());
    assert_eq!(names.data_type(), names.arrow_field().data_type().clone());

    // Nullability lives on the field, not in the data type, so `optional`
    // leaves the data type alone:
    assert_eq!(names.optional().data_type(), names.data_type());
    assert!(names.optional().arrow_field().is_nullable());
}

#[test]
fn deref_to_slice() {
    fn takes_slice(values: &[u64]) -> u64 {
        values.iter().sum()
    }
    fn takes_as_ref(values: impl AsRef<[u64]>) -> usize {
        values.as_ref().len()
    }

    let column = Column::<u64>::from_values("chunk_byte_size", [1_u64, 2, 3]);
    assert_eq!(takes_slice(&column), 6);
    assert_eq!(takes_as_ref(&column), 3);

    // Slice methods that `Column` does not have come along…
    assert_eq!(column.first(), Some(&1));
    assert_eq!(column.chunks(2).count(), 2);

    // …though `column[1..]` does not: `Index<usize>` is already implemented,
    // so indexing never reaches the slice. Deref explicitly for that:
    assert_eq!(&(*column)[1..], &[2, 3]);

    // …but never displace `Column`'s own, which read the logical values:
    assert_eq!(column.get(0), Some(1_u64)); // not `Some(&1)`
    assert_eq!(column.len(), 3);
    assert_eq!(column.iter().collect::<Vec<u64>>(), [1, 2, 3]);
    assert_eq!(column.to_vec(), [1_u64, 2, 3]);
    assert_eq!(column[1], 2); // `Index<usize>`, not the slice's

    // The deref follows the logical window, like `as_slice`:
    assert_eq!(takes_slice(&column.slice(1, 2)), 5);

    // Non-numeric natives deref too:
    let hashes = Column::<FixedSizeBinary<2>>::from_values("hash", [[1_u8, 2], [3, 4]]);
    let raw: &[[u8; 2]] = &hashes;
    assert_eq!(raw, &[[1, 2], [3, 4]]);

    // Same on the data half:
    let array = column.into_typed_array();
    assert_eq!(takes_slice(&array), 6);
    assert_eq!(array.get(0), Some(1_u64));
}

#[test]
fn column_partial_eq() {
    let a = Column::<Utf8>::from_values("sensor", ["x", "y"]);
    let b = Column::<Utf8>::from_values("sensor", ["x", "y"]);
    let c = Column::<Utf8>::from_values("sensor", ["x", "z"]);
    assert_eq!(a, b);
    assert_ne!(a, c);

    // The name participates, so the same values under two names differ:
    assert_ne!(a, b.clone().with_name("other"));

    // …and so does the metadata:
    let annotated = b.with_metadata(std::collections::BTreeMap::from([(
        "k".to_owned(),
        "v".to_owned(),
    )]));
    assert_ne!(a, annotated);

    // The data half compares on the values alone:
    assert_eq!(
        a.clone().into_typed_array(),
        a.with_name("other").into_typed_array()
    );
}

#[test]
fn column_slice() {
    let column = Column::<i64>::from_values("frame", [1, 2, 3, 4]).with_metadata(
        std::collections::BTreeMap::from([("k".to_owned(), "v".to_owned())]),
    );

    // The name and the metadata survive the slice:
    let sliced = column.slice(1, 2);
    assert_eq!(sliced.to_vec(), [2, 3]);
    assert_eq!(sliced.name(), "frame");
    assert_eq!(sliced.metadata()["k"], "v");

    // Lists slice too (the offsets shift):
    let column = Column::<List<i64>>::from_values("runs", [vec![1], vec![2, 3], vec![4]]);
    let sliced = column.slice(1, 2);
    assert_eq!(sliced.to_vec(), [vec![2, 3], vec![4]]);
}

#[test]
fn names() {
    const SENSOR: ColumnDesc<Utf8> = ColumnDesc::new("Measurements", "sensor");
    const RAW: DynColumnDesc = DynColumnDesc::new("Measurements", "raw");

    // `const`, and the same as the field:
    const NAME: &str = SENSOR.name();
    assert_eq!(NAME, SENSOR.name);
    assert_eq!(RAW.name(), RAW.name);

    // Changing the nullability keeps the name:
    assert_eq!(SENSOR.optional().name(), "sensor");
    assert_eq!(SENSOR.optional().required().name(), "sensor");
    assert_eq!(SENSOR.to_dyn().name(), "sensor");

    // A `Column` carries its own name, and hands it to the `DynColumn`:
    let column = SENSOR.new_from_values(["kitchen"]);
    assert_eq!(column.name(), "sensor");
    assert_eq!(column.clone().with_name("other").name(), "other");

    let column = column.into_dyn();
    assert_eq!(column.name(), "sensor");
    assert_eq!(column.name(), column.field().name());

    let batch =
        RecordBatch::try_from_iter([("raw", Arc::new(Int64Array::from(vec![1])) as ArrayRef)])
            .unwrap();
    assert_eq!(RAW.extract(&batch).unwrap().name(), "raw");
}

/// Every way of making a `Column` names it, and every transformation keeps
/// that name — the data half, `TypedArray`, has none to keep.
#[test]
fn column_names_survive_every_route() {
    let named = |column: Column<i64>| column.name().to_owned();

    assert_eq!(
        named(Column::try_new("a", Arc::new(Int64Array::from(vec![1])) as ArrayRef).unwrap()),
        "a"
    );
    assert_eq!(named(Column::from_values("a", [1_i64])), "a");
    assert_eq!(named(Column::try_from_values("a", [1_i64]).unwrap()), "a");
    assert_eq!(named(Column::empty("a")), "a");
    assert_eq!(
        named(Column::new("a", TypedArray::from_values([1_i64]))),
        "a"
    );
    assert_eq!(Column::<Option<i64>>::new_null("a", 1).name(), "a");

    let column = Column::from_values("a", [1_i64, 2]);
    assert_eq!(named(column.clone()), "a");
    assert_eq!(named(column.slice(0, 1)), "a");
    assert_eq!(column.clone().optional().name(), "a");
    assert_eq!(
        named(column.clone().optional().try_required().unwrap()),
        "a"
    );
    assert_eq!(named(column.clone().with_name("b").with_name("a")), "a");

    // Out to a `DynColumn` and back, by both routes:
    assert_eq!(column.clone().into_dyn().name(), "a");
    assert_eq!(
        named(column.clone().into_dyn().try_into_column().unwrap()),
        "a"
    );

    let batch = RecordBatch::try_from_iter([("a", column.into_arrow())]).unwrap();
    assert_eq!(
        named(Column::from_record_batch_and_name(&batch, "a").unwrap()),
        "a"
    );

    let desc: ColumnDesc<i64> = ColumnDesc::new("Record", "a");
    assert_eq!(named(desc.extract(&batch).unwrap()), "a");
    assert_eq!(named(desc.new_from_values([1_i64])), "a");
    assert_eq!(named(desc.try_new_from_values([1_i64]).unwrap()), "a");
    assert_eq!(desc.optional().new_null(1).name(), "a");
}

#[test]
fn column_to_dyn_and_back() {
    let column = Column::<Utf8>::from_values("name", ["alice", "bob"])
        .with_metadata(BTreeMap::from([("pii".to_owned(), "true".to_owned())]));

    // The field takes the column's name and metadata, the rest from `L`:
    let dynamic = column.into_dyn();
    assert_eq!(dynamic.field().name(), "name");
    assert_eq!(dynamic.field().data_type(), &DataType::Utf8);
    assert!(!dynamic.field().is_nullable());
    assert_eq!(dynamic.field().metadata()["pii"], "true");

    // …and back, name and metadata intact:
    let column: Column<Utf8> = dynamic.try_into_column().unwrap();
    assert_eq!(column.to_vec(), ["alice", "bob"]);
    assert_eq!(column.name(), "name");
    assert_eq!(column.metadata()["pii"], "true");

    // Renaming on the way out goes through `with_name`:
    let dynamic = column.with_name("who").into_dyn();
    assert_eq!(dynamic.field().name(), "who");

    // `Option<…>` is the only thing that makes the field nullable:
    let dynamic = Column::<Option<i64>>::from_values("age", [Some(1), None]).into_dyn();
    assert!(dynamic.field().is_nullable());
    assert_eq!(dynamic.field().data_type(), &DataType::Int64);

    // Nested types keep their inner field nullability through the round trip:
    let dynamic =
        Column::<List<Utf8>>::from_values("tags", [vec!["a".to_owned()], vec![]]).into_dyn();
    assert_eq!(
        dynamic.field().data_type(),
        &Column::<List<Utf8>>::data_type()
    );
    let column: Column<List<Utf8>> = dynamic.try_into_column().unwrap();
    assert_eq!(column.to_vec(), [vec!["a"], vec![]]);
}

#[test]
fn dyn_column_try_new_validation() {
    let ints: ArrayRef = Arc::new(Int64Array::from(vec![1, 2]));
    let with_null: ArrayRef = Arc::new(Int64Array::from(vec![Some(1), None]));

    // The array must have *exactly* the field's data type:
    let err = DynColumn::try_new(
        Arc::new(Field::new("age", DataType::Int32, false)),
        ArrayRef::clone(&ints),
    )
    .err()
    .unwrap();
    assert!(
        matches!(*err.kind, ErrorKind::WrongDataType { column, actual: DataType::Int64, .. } if column == "age")
    );

    // Inner-field nullability is part of the data type, so it is checked too:
    let list = TypedArray::<List<Utf8>>::from_values([vec!["a".to_owned()]]).into_arrow();
    let err = DynColumn::try_new(
        Arc::new(Field::new(
            "tags",
            TypedArray::<List<Option<Utf8>>>::data_type(),
            false,
        )),
        list,
    )
    .err()
    .unwrap();
    assert!(
        matches!(*err.kind, ErrorKind::WrongDataType { .. }),
        "{err}"
    );

    // A non-nullable field may not hold nulls…
    let err = DynColumn::try_new(
        Arc::new(Field::new("age", DataType::Int64, false)),
        ArrayRef::clone(&with_null),
    )
    .err()
    .unwrap();
    assert!(
        matches!(*err.kind, ErrorKind::UnexpectedNulls { column, null_count: 1 } if column == "age")
    );

    // …but a nullable one may, and may equally hold none:
    assert!(
        DynColumn::try_new(
            Arc::new(Field::new("age", DataType::Int64, true)),
            with_null,
        )
        .is_ok()
    );
    let column =
        DynColumn::try_new(Arc::new(Field::new("age", DataType::Int64, true)), ints).unwrap();

    // Field and array agree, so the pair always fits a record batch:
    let (field, array) = column.into_parts();
    let schema = Schema::new(vec![field]);
    assert!(RecordBatch::try_new(Arc::new(schema), vec![array]).is_ok());
}

#[test]
fn dyn_column_validation_names_the_field() {
    // Wrong data type → `WrongDataType`, naming the field.
    let dynamic = DynColumn::try_new(
        Arc::new(Field::new("age", DataType::Int64, false)),
        Arc::new(Int64Array::from(vec![1, 2])),
    )
    .unwrap();
    let err = dynamic.try_into_column::<Utf8>().err().unwrap();
    assert_eq!(err.record_type, "DynColumn");
    assert!(
        matches!(*err.kind, ErrorKind::WrongDataType { column, actual: DataType::Int64, .. } if column == "age")
    );

    // Nulls at a non-`Option` level → `UnexpectedNulls`, naming the field.
    let dynamic = DynColumn::try_new(
        Arc::new(Field::new("age", DataType::Int64, true)),
        Arc::new(Int64Array::from(vec![Some(1), None])),
    )
    .unwrap();
    let err = dynamic.try_into_column::<i64>().err().unwrap();
    assert_eq!(err.record_type, "DynColumn");
    assert!(
        matches!(*err.kind, ErrorKind::UnexpectedNulls { column, null_count: 1 } if column == "age")
    );

    // The *array* decides, not the field flag: a nullable field with no nulls
    // is a perfectly good `Column<i64>`.
    let dynamic = DynColumn::try_new(
        Arc::new(Field::new("age", DataType::Int64, true)),
        Arc::new(Int64Array::from(vec![1, 2])),
    )
    .unwrap();
    let column: Column<i64> = dynamic.try_into_column().unwrap();
    assert_eq!(column.to_vec(), [1, 2]);
}
