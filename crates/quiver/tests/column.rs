//! Tests for standalone use of [`quiver::Column`] — no derive macro involved.

use std::sync::Arc;

use quiver::arrow::array::Array as _;
use quiver::arrow::array::{
    ArrayRef, DurationMillisecondArray, FixedSizeBinaryArray, Int64Array, ListArray, StringArray,
    TimestampNanosecondArray, TimestampSecondArray,
};
use quiver::arrow::datatypes::{DataType, Field, Int32Type, Int64Type};
use quiver::arrow::error::ArrowError;
use quiver::{
    Column, ColumnError, Duration, List, Millisecond, Nanosecond, Second, Timestamp, Utc, Utf8,
};

#[test]
fn standalone_flat_column() {
    let dynamic_array: ArrayRef = Arc::new(StringArray::from(vec!["foo", "bar"]));

    let column = Column::<Utf8>::try_from(dynamic_array).unwrap();
    assert_eq!(column.len(), 2);
    assert_eq!(column.value(0), "foo");
    assert_eq!(column.get(2), None);

    let strings: Vec<&str> = column.iter().collect();
    assert_eq!(strings, ["foo", "bar"]);
}

#[test]
fn standalone_nullable_column() {
    let dynamic_array: ArrayRef = Arc::new(Int64Array::from(vec![Some(1), None]));

    // Non-nullable logical type rejects the nulls:
    let result = Column::<i64>::try_from(Arc::clone(&dynamic_array));
    assert!(matches!(
        result,
        Err(ColumnError::UnexpectedNulls { null_count: 1 })
    ));

    // Nullable logical type accepts them:
    let column = Column::<Option<i64>>::try_from(dynamic_array).unwrap();
    let values: Vec<Option<i64>> = column.iter().collect();
    assert_eq!(values, [Some(1), None]);
}

#[test]
fn standalone_list_column() {
    let dynamic_array: ArrayRef =
        Arc::new(ListArray::from_iter_primitive::<Int64Type, _, _>(vec![
            Some(vec![Some(1), Some(2)]),
            Some(vec![Some(3)]),
        ]));

    // The item field is declared nullable but contains no nulls,
    // so both `List<i64>` and `List<Option<i64>>` accept it
    // (inner field nullability flags are not compared — actual nulls are what matters):
    let column = Column::<List<i64>>::try_from(Arc::clone(&dynamic_array)).unwrap();
    let lists: Vec<Vec<i64>> = column.iter().map(Iterator::collect).collect();
    assert_eq!(lists, [vec![1, 2], vec![3]]);

    let column = Column::<List<Option<i64>>>::try_from(dynamic_array).unwrap();
    let lists: Vec<Vec<Option<i64>>> = column.iter().map(Iterator::collect).collect();
    assert_eq!(lists, [vec![Some(1), Some(2)], vec![Some(3)]]);
}

#[test]
fn standalone_wrong_datatype() {
    let dynamic_array: ArrayRef = Arc::new(Int64Array::from(vec![1]));

    let result = Column::<Utf8>::try_from(dynamic_array);
    assert!(matches!(
        result,
        Err(ColumnError::WrongDatatype {
            expected: DataType::Utf8,
            actual: DataType::Int64,
        })
    ));
}

#[test]
fn standalone_nested_list() {
    // List<List<Utf8>>: [[["a"], ["b", "c"]]]
    let strings = StringArray::from(vec!["a", "b", "c"]);
    let inner_field = Arc::new(Field::new("item", DataType::Utf8, false));
    let inner = ListArray::new(
        Arc::clone(&inner_field),
        quiver::arrow::buffer::OffsetBuffer::new(vec![0, 1, 3].into()),
        Arc::new(strings),
        None,
    );
    let outer_field = Arc::new(Field::new("item", DataType::List(inner_field), false));
    let outer = ListArray::new(
        outer_field,
        quiver::arrow::buffer::OffsetBuffer::new(vec![0, 2].into()),
        Arc::new(inner),
        None,
    );

    let column = Column::<List<List<Utf8>>>::try_from(Arc::new(outer) as ArrayRef).unwrap();
    let nested: Vec<Vec<Vec<&str>>> = column
        .iter()
        .map(|outer| outer.map(Iterator::collect).collect())
        .collect();
    assert_eq!(nested, [vec![vec!["a"], vec!["b", "c"]]]);
}

#[test]
fn standalone_fixed_size_binary_column() {
    let dynamic_array: ArrayRef = Arc::new(
        FixedSizeBinaryArray::try_from_iter(vec![[1_u8; 16], [2; 16]].into_iter()).unwrap(),
    );

    // Wrong size is rejected:
    let result = Column::<[u8; 8]>::try_from(Arc::clone(&dynamic_array));
    assert!(matches!(
        result,
        Err(ColumnError::WrongDatatype {
            expected: DataType::FixedSizeBinary(8),
            actual: DataType::FixedSizeBinary(16),
        })
    ));

    // Matching size:
    let column = Column::<[u8; 16]>::try_from(dynamic_array).unwrap();
    assert_eq!(column.value(0), &[1_u8; 16]);
    let values: Vec<&[u8; 16]> = column.iter().collect();
    assert_eq!(values, [&[1_u8; 16], &[2; 16]]);
}

#[test]
fn standalone_nullable_fixed_size_binary_column() {
    let dynamic_array: ArrayRef = Arc::new(
        FixedSizeBinaryArray::try_from_sparse_iter_with_size(
            vec![Some([1_u8; 4]), None].into_iter(),
            4,
        )
        .unwrap(),
    );

    // Non-nullable logical type rejects the nulls:
    let result = Column::<[u8; 4]>::try_from(Arc::clone(&dynamic_array));
    assert!(matches!(
        result,
        Err(ColumnError::UnexpectedNulls { null_count: 1 })
    ));

    // Nullable logical type accepts them:
    let column = Column::<Option<[u8; 4]>>::try_from(dynamic_array).unwrap();
    let values: Vec<Option<&[u8; 4]>> = column.iter().collect();
    assert_eq!(values, [Some(&[1_u8; 4]), None]);
}

#[test]
fn standalone_timestamp_column() {
    let naive: ArrayRef = Arc::new(TimestampNanosecondArray::from(vec![1, 2]));
    let utc: ArrayRef = Arc::new(TimestampNanosecondArray::from(vec![1, 2]).with_timezone("UTC"));

    // Timezone-naive:
    let column = Column::<Timestamp<Nanosecond>>::try_from(Arc::clone(&naive)).unwrap();
    let values: Vec<i64> = column.iter().collect();
    assert_eq!(values, [1, 2]);

    // Timezones are matched exactly, in both directions:
    assert!(matches!(
        Column::<Timestamp<Nanosecond>>::try_from(Arc::clone(&utc)),
        Err(ColumnError::WrongDatatype { .. })
    ));
    assert!(matches!(
        Column::<Timestamp<Nanosecond, Utc>>::try_from(naive),
        Err(ColumnError::WrongDatatype { .. })
    ));

    let column = Column::<Timestamp<Nanosecond, Utc>>::try_from(utc).unwrap();
    assert_eq!(column.value(1), 2);

    // The unit must match, too:
    let seconds: ArrayRef = Arc::new(TimestampSecondArray::from(vec![1]));
    assert!(matches!(
        Column::<Timestamp<Nanosecond>>::try_from(Arc::clone(&seconds)),
        Err(ColumnError::WrongDatatype { .. })
    ));
    let column = Column::<Timestamp<Second>>::try_from(seconds).unwrap();
    assert_eq!(column.value(0), 1);
}

#[test]
fn standalone_nullable_timestamp_column() {
    let array: ArrayRef = Arc::new(TimestampNanosecondArray::from(vec![Some(1), None]));

    assert!(matches!(
        Column::<Timestamp<Nanosecond>>::try_from(Arc::clone(&array)),
        Err(ColumnError::UnexpectedNulls { null_count: 1 })
    ));

    let column = Column::<Option<Timestamp<Nanosecond>>>::try_from(array).unwrap();
    let values: Vec<Option<i64>> = column.iter().collect();
    assert_eq!(values, [Some(1), None]);
}

#[test]
fn column_metadata() {
    let column = Column::<i64>::try_from(Arc::new(Int64Array::from(vec![1])) as ArrayRef)
        .unwrap()
        .with_metadata(std::collections::BTreeMap::from([(
            "unit".to_owned(),
            "seconds".to_owned(),
        )]));
    assert_eq!(column.metadata()["unit"], "seconds");

    let mut column = column;
    column
        .metadata_mut()
        .insert("source".to_owned(), "sensor".to_owned());
    assert_eq!(column.metadata().len(), 2);
}

#[test]
fn standalone_duration_column() {
    let array: ArrayRef = Arc::new(DurationMillisecondArray::from(vec![100, 200]));

    // The unit must match:
    assert!(matches!(
        Column::<Duration<Nanosecond>>::try_from(Arc::clone(&array)),
        Err(ColumnError::WrongDatatype { .. })
    ));

    let column = Column::<Duration<Millisecond>>::try_from(array).unwrap();
    let values: Vec<i64> = column.iter().collect();
    assert_eq!(values, [100, 200]);

    // Nullable:
    let array: ArrayRef = Arc::new(DurationMillisecondArray::from(vec![Some(1), None]));
    assert!(matches!(
        Column::<Duration<Millisecond>>::try_from(Arc::clone(&array)),
        Err(ColumnError::UnexpectedNulls { null_count: 1 })
    ));
    let column = Column::<Option<Duration<Millisecond>>>::try_from(array).unwrap();
    let values: Vec<Option<i64>> = column.iter().collect();
    assert_eq!(values, [Some(1), None]);
}

#[test]
fn default_column_is_empty() {
    let column = Column::<i64>::default();
    assert!(column.is_empty());

    let column = Column::<List<Option<Utf8>>>::default();
    assert!(column.is_empty());
    assert_eq!(column.iter().count(), 0);
    assert!(column.metadata().is_empty());

    let column = Column::<Timestamp<Nanosecond, Utc>>::default();
    assert!(column.is_empty());

    let column = Column::<[u8; 16]>::default();
    assert_eq!(
        column.as_arrow().data_type(),
        &DataType::FixedSizeBinary(16)
    );
}

#[test]
fn errors_convert_to_arrow_error() {
    // So that `?` works in functions returning arrow results:
    fn parse(array: ArrayRef) -> Result<Column<i64>, ArrowError> {
        Ok(Column::try_new(array)?)
    }

    let err = parse(Arc::new(StringArray::from(vec!["nope"])) as ArrayRef)
        .err()
        .unwrap();
    assert!(matches!(err, ArrowError::ExternalError(_)));
    assert!(err.to_string().contains("Expected datatype"));
}

#[test]
fn convenience_constructors() {
    // From anything that converts into the owned value (e.g. `&str` → `String`):
    let column = Column::<Utf8>::from_values(["a", "b"]);
    let values: Vec<&str> = column.iter().collect();
    assert_eq!(values, ["a", "b"]);

    // `From<Vec<T>>`:
    let column: Column<i64> = vec![1, 2].into();
    let values: Vec<i64> = column.iter().collect();
    assert_eq!(values, [1, 2]);

    // `FromIterator`:
    let column: Column<f64> = [1.0, 2.5].into_iter().collect();
    assert_eq!(column.value(1), 2.5);

    // Nullable values:
    let column = Column::<Option<i64>>::from_values([Some(1), None]);
    let values: Vec<Option<i64>> = column.iter().collect();
    assert_eq!(values, [Some(1), None]);

    // Lists:
    let column = Column::<List<i64>>::from_values([vec![1, 2], vec![3]]);
    let values: Vec<Vec<i64>> = column.iter().map(Iterator::collect).collect();
    assert_eq!(values, [vec![1, 2], vec![3]]);

    // Nullable lists with nullable items:
    let column =
        Column::<Option<List<Option<i64>>>>::from_values([Some(vec![Some(1), None]), None]);
    let values: Vec<Option<Vec<Option<i64>>>> = column
        .iter()
        .map(|list| list.map(Iterator::collect))
        .collect();
    assert_eq!(values, [Some(vec![Some(1), None]), None]);

    // Fixed-size binary:
    let column = Column::<[u8; 4]>::from_values([[1_u8, 2, 3, 4], [5, 6, 7, 8]]);
    assert_eq!(column.value(1), &[5, 6, 7, 8]);

    // Timestamps get the declared timezone:
    let column = Column::<Timestamp<Nanosecond, Utc>>::from_values([1_i64, 2]);
    assert_eq!(
        column.as_arrow().data_type(),
        &DataType::Timestamp(
            quiver::arrow::datatypes::TimeUnit::Nanosecond,
            Some("UTC".into())
        )
    );

    // Durations:
    let column = Column::<Duration<Millisecond>>::from_values([100_i64]);
    assert_eq!(column.value(0), 100);
}

#[test]
fn static_datatype() {
    assert_eq!(Column::<i64>::datatype(), DataType::Int64);
    assert_eq!(Column::<Option<i64>>::datatype(), DataType::Int64); // Nullability is not part of the datatype
    assert_eq!(
        Column::<List<Option<Utf8>>>::datatype(),
        DataType::List(Arc::new(Field::new("item", DataType::Utf8, true)))
    );
    assert_eq!(
        Column::<List<Utf8>>::datatype(),
        DataType::List(Arc::new(Field::new("item", DataType::Utf8, false)))
    );
    const {
        assert!(Column::<Option<i64>>::NULLABLE);
        assert!(!Column::<i64>::NULLABLE);
    }
}

#[test]
fn to_vec_and_iter_owned() {
    let column = Column::<Utf8>::from_values(["a", "b"]);
    let owned: Vec<String> = column.to_vec();
    assert_eq!(owned, ["a".to_owned(), "b".to_owned()]);

    let column = Column::<Option<Utf8>>::from_values([Some("a".to_owned()), None]);
    assert_eq!(column.to_vec(), [Some("a".to_owned()), None]);

    let column = Column::<List<i64>>::from_values([vec![1, 2], vec![3]]);
    assert_eq!(column.to_vec(), [vec![1, 2], vec![3]]);

    let column = Column::<[u8; 2]>::from_values([[1_u8, 2], [3, 4]]);
    assert_eq!(column.to_vec(), [[1_u8, 2], [3, 4]]);

    let total: i64 = Column::<i64>::from_values([1, 2, 3]).iter_owned().sum();
    assert_eq!(total, 6);
}

#[test]
fn as_slice() {
    let column = Column::<f32>::from_values([1.0, 2.0, 3.0]);
    assert_eq!(column.as_slice(), &[1.0, 2.0, 3.0]);

    let column = Column::<u8>::from_values([1_u8, 2, 3]);
    assert_eq!(column.as_slice(), &[1, 2, 3]);

    // Markers expose their native values:
    let column = Column::<Timestamp<Nanosecond, Utc>>::from_values([10_i64, 20]);
    assert_eq!(column.as_slice(), &[10_i64, 20]);

    let column = Column::<Duration<Millisecond>>::from_values([10_i64, 20]);
    assert_eq!(column.as_slice(), &[10_i64, 20]);

    // The `As` adapter exposes the representation's values:
    let column =
        Column::<quiver::As<std::net::Ipv4Addr, u32>>::from_values([std::net::Ipv4Addr::LOCALHOST]);
    assert_eq!(
        column.as_slice(),
        &[u32::from(std::net::Ipv4Addr::LOCALHOST)]
    );
}

#[test]
fn as_slice_fixed_size_binary() {
    // Bulk zero-copy read of fixed-size binary columns:
    let column = Column::<[u8; 4]>::from_values([[1_u8, 2, 3, 4], [5, 6, 7, 8]]);
    assert_eq!(column.as_slice(), &[[1_u8, 2, 3, 4], [5, 6, 7, 8]]);

    // Also when parsed from a raw arrow array:
    let array: ArrayRef = Arc::new(
        FixedSizeBinaryArray::try_from_iter(vec![[1_u8; 16], [2; 16]].into_iter()).unwrap(),
    );
    let column = Column::<[u8; 16]>::try_from(array).unwrap();
    assert_eq!(column.as_slice(), &[[1_u8; 16], [2; 16]]);

    // Empty:
    let column = Column::<[u8; 4]>::default();
    assert_eq!(column.as_slice(), &[] as &[[u8; 4]]);
}

#[test]
fn as_slice_respects_offset() {
    let column = Column::<i64>::from_values([1, 2, 3, 4, 5]);
    let sliced = column.slice(1, 3);
    assert_eq!(sliced.as_slice(), &[2, 3, 4]);

    // Fixed-size binary too — the byte window must follow the slice:
    let column = Column::<[u8; 2]>::from_values([[1_u8, 2], [3, 4], [5, 6], [7, 8]]);
    let sliced = column.slice(1, 2);
    assert_eq!(sliced.as_slice(), &[[3_u8, 4], [5, 6]]);
}

#[test]
fn index() {
    let strings = Column::<Utf8>::from_values(["a", "b"]);
    assert_eq!(&strings[0], "a");
    assert_eq!(&strings[1], "b");

    let numbers = Column::<i64>::from_values([1, 2, 3]);
    assert_eq!(numbers[2], 3);

    let binary = Column::<quiver::Binary>::from_values([vec![1_u8, 2], vec![3]]);
    assert_eq!(&binary[1], [3_u8]);

    let uuids = Column::<[u8; 4]>::from_values([[1_u8, 2, 3, 4]]);
    assert_eq!(uuids[0], [1_u8, 2, 3, 4]);

    let timestamps = Column::<Timestamp<Nanosecond, Utc>>::from_values([10_i64, 20]);
    assert_eq!(timestamps[1], 20);

    // Dictionary values are looked up through the keys:
    let tags: Column<quiver::Dictionary<i32, Utf8>> = vec!["a", "b", "a"].try_into().unwrap();
    assert_eq!(&tags[2], "a");

    // The `As` adapter yields the representation's reference:
    let ips =
        Column::<quiver::As<std::net::Ipv4Addr, u32>>::from_values([std::net::Ipv4Addr::LOCALHOST]);
    assert_eq!(ips[0], u32::from(std::net::Ipv4Addr::LOCALHOST));

    // Indexing respects slice offsets:
    let sliced = numbers.slice(1, 2);
    assert_eq!(sliced[0], 2);
    let sliced = strings.slice(1, 1);
    assert_eq!(&sliced[0], "b");
}

#[test]
#[should_panic(expected = "Index 2 out of bounds")]
fn index_out_of_bounds() {
    let strings = Column::<Utf8>::from_values(["a", "b"]);
    let _: &str = &strings[2];
}

#[test]
fn value_owned_and_get_owned() {
    let column = Column::<Utf8>::from_values(["a", "b"]);
    let owned: String = column.value_owned(1);
    assert_eq!(owned, "b");
    assert_eq!(column.get_owned(0), Some("a".to_owned()));
    assert_eq!(column.get_owned(2), None);

    // The owned value of a newtype column is the newtype:
    let column = Column::<SensorName>::from_values([SensorName("kitchen".to_owned())]);
    assert_eq!(column.value_owned(0), SensorName("kitchen".to_owned()));
    assert_eq!(column.get_owned(0), Some(SensorName("kitchen".to_owned())));

    let column = Column::<Option<i64>>::from_values([Some(1), None]);
    assert_eq!(column.value_owned(1), None);
    assert_eq!(column.get_owned(1), Some(None));
    assert_eq!(column.get_owned(2), None);
}

#[test]
#[should_panic(expected = "Index 1 out of bounds")]
fn value_owned_out_of_bounds() {
    let column = Column::<Utf8>::from_values(["a"]);
    let _value: String = column.value_owned(1);
}

#[test]
fn nullable_construction_ergonomics() {
    // Owned values work directly:
    let column: Column<Option<Utf8>> = vec![Some("a".to_owned()), None].into();
    assert_eq!(column.to_vec(), [Some("a".to_owned()), None]);

    // Borrowed values need `from_nullable_values`
    // (std has no `From<Option<&str>> for Option<String>`):
    let column = Column::<Option<Utf8>>::from_nullable_values([Some("a"), None]);
    assert_eq!(column.to_vec(), [Some("a".to_owned()), None]);

    let column = Column::<Option<List<i64>>>::from_nullable_values([Some(vec![1, 2]), None]);
    assert_eq!(column.to_vec(), [Some(vec![1, 2]), None]);
}

#[test]
fn into_iterator() {
    let column = Column::<Utf8>::from_values(["a", "b"]);

    // By reference: borrowed values.
    let mut borrowed = Vec::new();
    for value in &column {
        borrowed.push(value); // `&str`
    }
    assert_eq!(borrowed, ["a", "b"]);

    // By value: owned values, like a `Vec`.
    let mut owned = Vec::new();
    for value in column {
        owned.push(value); // `String`
    }
    assert_eq!(owned, ["a".to_owned(), "b".to_owned()]);
}

#[test]
fn timestamp_and_duration_aliases() {
    use quiver::{
        Duration, DurationMillisecond, Millisecond, Nanosecond, Timestamp, TimestampNanosecond, Utc,
    };

    // The aliases are the same types:
    assert_eq!(
        Column::<TimestampNanosecond<Utc>>::datatype(),
        Column::<Timestamp<Nanosecond, Utc>>::datatype()
    );
    assert_eq!(
        Column::<TimestampNanosecond>::datatype(), // timezone-naive default
        Column::<Timestamp<Nanosecond>>::datatype()
    );
    assert_eq!(
        Column::<DurationMillisecond>::datatype(),
        Column::<Duration<Millisecond>>::datatype()
    );
}

#[test]
fn binary_columns() {
    use quiver::{Binary, LargeBinary};

    let column = Column::<Binary>::from_values([b"abc".to_vec(), vec![0_u8, 1]]);
    assert_eq!(column.value(0), b"abc");
    assert_eq!(column.to_vec(), [b"abc".to_vec(), vec![0_u8, 1]]);
    assert_eq!(Column::<Binary>::datatype(), DataType::Binary);

    let column = Column::<LargeBinary>::from_values([b"abc".to_vec()]);
    assert_eq!(Column::<LargeBinary>::datatype(), DataType::LargeBinary);
    assert_eq!(column.value(0), b"abc");

    // Binary ≠ LargeBinary:
    let result = Column::<Binary>::try_from(column.into_arrow());
    assert!(matches!(result, Err(ColumnError::WrongDatatype { .. })));

    // Nullable:
    let column = Column::<Option<Binary>>::from_nullable_values([Some(b"abc".to_vec()), None]);
    assert_eq!(column.to_vec(), [Some(b"abc".to_vec()), None]);

    // Lists of binary:
    let column = Column::<List<Binary>>::from_values([vec![b"a".to_vec(), b"b".to_vec()]]);
    let lists: Vec<Vec<Vec<u8>>> = column.to_vec();
    assert_eq!(lists, [vec![b"a".to_vec(), b"b".to_vec()]]);
}

#[test]
fn binary_view_columns() {
    use quiver::arrow::array::BinaryViewArray;
    use quiver::{Binary, BinaryView};

    let column = Column::<BinaryView>::from_values([b"abc".to_vec(), vec![0_u8, 1]]);
    assert_eq!(column.value(0), b"abc");
    assert_eq!(&column[1], &[0_u8, 1]);
    assert_eq!(column.to_vec(), [b"abc".to_vec(), vec![0_u8, 1]]);
    assert_eq!(Column::<BinaryView>::datatype(), DataType::BinaryView);

    // BinaryView ≠ Binary:
    let result = Column::<Binary>::try_from(column.into_arrow());
    assert!(matches!(result, Err(ColumnError::WrongDatatype { .. })));

    // Nullable:
    let column = Column::<Option<BinaryView>>::from_nullable_values([Some(b"abc".to_vec()), None]);
    assert_eq!(column.to_vec(), [Some(b"abc".to_vec()), None]);

    // Parsing an externally built array:
    let array = BinaryViewArray::from_iter_values([b"x".as_slice(), b"yz"]);
    let column = Column::<BinaryView>::try_from(Arc::new(array) as ArrayRef).unwrap();
    assert_eq!(column.value(1), b"yz");

    // A null at a non-nullable level is rejected:
    let array = BinaryViewArray::from_iter([Some(b"x".as_slice()), None]);
    let result = Column::<BinaryView>::try_from(Arc::new(array) as ArrayRef);
    assert!(matches!(
        result,
        Err(ColumnError::UnexpectedNulls { null_count: 1 })
    ));
}

#[test]
fn f16_column() {
    use quiver::half::f16;

    let column = Column::<f16>::from_values([f16::from_f32(1.5), f16::from_f32(2.5)]);
    assert_eq!(Column::<f16>::datatype(), DataType::Float16);
    assert_eq!(column.value(0), f16::from_f32(1.5));
    assert_eq!(column.iter().map(f16::to_f32).sum::<f32>(), 4.0);

    let column = Column::<Option<f16>>::from_values([Some(f16::from_f32(1.5)), None]);
    assert_eq!(column.to_vec(), [Some(f16::from_f32(1.5)), None]);
}

#[test]
fn dictionary_columns() {
    use quiver::Dictionary;
    use quiver::arrow::array::DictionaryArray;

    // Building dictionary-encodes the values:
    let column = Column::<Dictionary<i32, Utf8>>::try_from_values(["a", "b", "a", "a"]).unwrap();
    assert_eq!(
        Column::<Dictionary<i32, Utf8>>::datatype(),
        DataType::Dictionary(Box::new(DataType::Int32), Box::new(DataType::Utf8))
    );

    // The dictionary is transparent: values read as if it were a plain column:
    let values: Vec<&str> = column.iter().collect();
    assert_eq!(values, ["a", "b", "a", "a"]);
    assert_eq!(column.to_vec(), ["a", "b", "a", "a"]);

    // Parsing an externally built dictionary array:
    let array: DictionaryArray<Int64Type> = vec!["x", "y", "x"].into_iter().collect();
    let column = Column::<Dictionary<i64, Utf8>>::try_from(Arc::new(array) as ArrayRef).unwrap();
    assert_eq!(column.value(2), "x");

    // The key type must match:
    let result = Column::<Dictionary<i32, Utf8>>::try_from(column.into_arrow());
    assert!(matches!(result, Err(ColumnError::WrongDatatype { .. })));

    // Null keys via the column-level Option:
    let array: DictionaryArray<Int32Type> = vec![Some("x"), None]
        .into_iter()
        .collect::<DictionaryArray<_>>();
    let array = Arc::new(array) as ArrayRef;
    assert!(matches!(
        Column::<Dictionary<i32, Utf8>>::try_from(Arc::clone(&array)),
        Err(ColumnError::UnexpectedNulls { null_count: 1 })
    ));
    let column = Column::<Option<Dictionary<i32, Utf8>>>::try_from(array).unwrap();
    let values: Vec<Option<&str>> = column.iter().collect();
    assert_eq!(values, [Some("x"), None]);
}

#[test]
fn dictionary_key_overflow_is_an_error() {
    use quiver::Dictionary;

    // 200 distinct values do not fit in an i8 key:
    let values: Vec<String> = (0..200).map(|i| i.to_string()).collect();
    let result = Column::<Dictionary<i8, Utf8>>::try_from_values(values.clone());
    assert!(matches!(result, Err(ColumnError::Build(_))));

    // …but they fit in an i16 key:
    let column = Column::<Dictionary<i16, Utf8>>::try_from_values(values).unwrap();
    assert_eq!(column.len(), 200);
}

#[test]
fn dictionary_try_into() {
    use quiver::Dictionary;

    let column: Column<Dictionary<i32, Utf8>> = vec!["a", "b", "a"].try_into().unwrap();
    assert_eq!(column.to_vec(), ["a", "b", "a"]);

    // Key overflow propagates as an error:
    let values: Vec<String> = (0..200).map(|i| i.to_string()).collect();
    let result: Result<Column<Dictionary<i8, Utf8>>, _> = values.try_into();
    assert!(matches!(result, Err(ColumnError::Build(_))));
}

/// Validation must count *logical* nulls, not physical ones (self-review bug fix).
#[test]
fn logical_null_validation() {
    use quiver::Dictionary;
    use quiver::arrow::array::{DictionaryArray, ListArray};

    // A null item that is unreachable after slicing is fine…
    let list = ListArray::from_iter_primitive::<Int64Type, _, _>(vec![
        Some(vec![None]), // null item, only in row 0
        Some(vec![Some(2)]),
    ]);
    let sliced = list.slice(1, 1);
    let column = Column::<List<i64>>::try_from(Arc::new(sliced) as ArrayRef).unwrap();
    let values: Vec<Vec<i64>> = column.to_vec();
    assert_eq!(values, [vec![2]]);

    // …but a reachable one is still rejected:
    let list = ListArray::from_iter_primitive::<Int64Type, _, _>(vec![
        Some(vec![None]),
        Some(vec![Some(2)]),
    ]);
    let result = Column::<List<i64>>::try_from(Arc::new(list) as ArrayRef);
    assert!(matches!(
        result,
        Err(ColumnError::UnexpectedNulls { null_count: 1 })
    ));

    // Null items inside the range of a NULL row don't count:
    let list = ListArray::from_iter_primitive::<Int64Type, _, _>(vec![
        None, // null row — arrow's builder gives it an empty range
        Some(vec![Some(2)]),
    ]);
    let column = Column::<Option<List<i64>>>::try_from(Arc::new(list) as ArrayRef).unwrap();
    assert_eq!(column.len(), 2);

    // An unreferenced null entry in a dictionary's value table is fine…
    let values = StringArray::from(vec![Some("a"), None]); // entry 1 is null, unreferenced
    let keys = quiver::arrow::array::Int32Array::from(vec![0, 0]);
    let dictionary = DictionaryArray::new(keys, Arc::new(values));
    let column =
        Column::<Dictionary<i32, Utf8>>::try_from(Arc::new(dictionary) as ArrayRef).unwrap();
    assert_eq!(column.to_vec(), ["a", "a"]);

    // …but a referenced one is still rejected:
    let values = StringArray::from(vec![Some("a"), None]);
    let keys = quiver::arrow::array::Int32Array::from(vec![0, 1]); // references the null
    let dictionary = DictionaryArray::new(keys, Arc::new(values));
    let result = Column::<Dictionary<i32, Utf8>>::try_from(Arc::new(dictionary) as ArrayRef);
    assert!(matches!(
        result,
        Err(ColumnError::UnexpectedNulls { null_count: 1 })
    ));
}

/// Inner field names are not compared: parquet names list items "element",
/// arrow names them "item" — both must parse.
#[test]
fn list_item_field_name_is_ignored() {
    let values = Int64Array::from(vec![1, 2, 3]);
    let field = Arc::new(Field::new("element", DataType::Int64, false)); // parquet-style
    let offsets = quiver::arrow::buffer::OffsetBuffer::new(vec![0, 2, 3].into());
    let list = ListArray::new(field, offsets, Arc::new(values), None);

    let column = Column::<List<i64>>::try_from(Arc::new(list) as ArrayRef).unwrap();
    let lists: Vec<Vec<i64>> = column.to_vec();
    assert_eq!(lists, [vec![1, 2], vec![3]]);
}

#[test]
fn date_and_time_columns() {
    use quiver::{Date32, Date64, Time32Second, Time64Nanosecond};

    let column = Column::<Date32>::from_values([19_000_i32, 19_001]);
    assert_eq!(Column::<Date32>::datatype(), DataType::Date32);
    assert_eq!(column.to_vec(), [19_000, 19_001]);

    assert_eq!(Column::<Date64>::datatype(), DataType::Date64);

    let column = Column::<Time32Second>::from_values([3600_i32]);
    assert_eq!(
        Column::<Time32Second>::datatype(),
        DataType::Time32(quiver::arrow::datatypes::TimeUnit::Second)
    );
    assert_eq!(column.value(0), 3600);

    let column = Column::<Option<Time64Nanosecond>>::from_values([Some(1_i64), None]);
    assert_eq!(column.to_vec(), [Some(1), None]);
}

#[test]
fn large_utf8_column() {
    use quiver::LargeUtf8;

    let column = Column::<LargeUtf8>::from_values(["a", "b"]);
    assert_eq!(Column::<LargeUtf8>::datatype(), DataType::LargeUtf8);
    let values: Vec<&str> = column.iter().collect();
    assert_eq!(values, ["a", "b"]);
    assert_eq!(column.to_vec(), ["a".to_owned(), "b".to_owned()]);
}

#[test]
fn column_partial_eq() {
    let a = Column::<Utf8>::from_values(["x", "y"]);
    let b = Column::<Utf8>::from_values(["x", "y"]);
    let c = Column::<Utf8>::from_values(["x", "z"]);
    assert_eq!(a, b);
    assert_ne!(a, c);

    // Metadata participates:
    let annotated = b.with_metadata(std::collections::BTreeMap::from([(
        "k".to_owned(),
        "v".to_owned(),
    )]));
    assert_ne!(a, annotated);
}

#[test]
fn column_slice() {
    let column =
        Column::<i64>::from_values([1, 2, 3, 4]).with_metadata(std::collections::BTreeMap::from([
            ("k".to_owned(), "v".to_owned()),
        ]));

    let sliced = column.slice(1, 2);
    assert_eq!(sliced.to_vec(), [2, 3]);
    assert_eq!(sliced.metadata()["k"], "v");

    // Lists slice too (the offsets shift):
    let column = Column::<List<i64>>::from_values([vec![1], vec![2, 3], vec![4]]);
    let sliced = column.slice(1, 2);
    assert_eq!(sliced.to_vec(), [vec![2, 3], vec![4]]);
}

#[test]
fn fixed_size_list_columns() {
    use quiver::FixedSizeList;

    // 3D positions:
    let column =
        Column::<FixedSizeList<f32, 3>>::from_values([[1.0_f32, 2.0, 3.0], [4.0, 5.0, 6.0]]);
    assert_eq!(
        Column::<FixedSizeList<f32, 3>>::datatype(),
        DataType::FixedSizeList(Arc::new(Field::new("item", DataType::Float32, false)), 3)
    );
    let positions: Vec<[f32; 3]> = column.to_vec();
    assert_eq!(positions, [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]);

    // Iteration is zero-copy, like List:
    let first: Vec<f32> = column.value(0).collect();
    assert_eq!(first, [1.0, 2.0, 3.0]);

    // The size is part of the type:
    let result = Column::<FixedSizeList<f32, 4>>::try_from(Arc::clone(column.as_arrow()));
    assert!(matches!(result, Err(ColumnError::WrongDatatype { .. })));

    // Nullable rows: the null row's placeholder items are masked, not errors:
    let column = Column::<Option<FixedSizeList<f32, 3>>>::from_nullable_values([
        Some([1.0_f32, 2.0, 3.0]),
        None,
    ]);
    assert_eq!(column.to_vec(), [Some([1.0, 2.0, 3.0]), None]);

    // Roundtrip through arrow:
    let roundtripped =
        Column::<Option<FixedSizeList<f32, 3>>>::try_from(column.into_arrow()).unwrap();
    assert_eq!(roundtripped.to_vec(), [Some([1.0, 2.0, 3.0]), None]);

    // Slicing:
    let column = Column::<FixedSizeList<i64, 2>>::from_values([[1_i64, 2], [3, 4], [5, 6]]);
    let sliced = column.slice(1, 2);
    assert_eq!(sliced.to_vec(), [[3, 4], [5, 6]]);
}

#[test]
fn large_list_columns() {
    use quiver::LargeList;
    use quiver::arrow::array::LargeListArray;

    let column = Column::<LargeList<i64>>::from_values([vec![1_i64, 2], vec![3]]);
    assert_eq!(
        Column::<LargeList<i64>>::datatype(),
        DataType::LargeList(Arc::new(Field::new("item", DataType::Int64, false)))
    );
    let lists: Vec<Vec<i64>> = column.to_vec();
    assert_eq!(lists, [vec![1, 2], vec![3]]);

    // Iteration is zero-copy, like List:
    let first: Vec<i64> = column.value(0).collect();
    assert_eq!(first, [1, 2]);

    // List ≠ LargeList: the offset width is part of the type:
    let result = Column::<List<i64>>::try_from(Arc::clone(column.as_arrow()));
    assert!(matches!(result, Err(ColumnError::WrongDatatype { .. })));

    // Nullable items:
    let column = Column::<LargeList<Option<i64>>>::from_values([vec![Some(1), None]]);
    let lists: Vec<Vec<Option<i64>>> = column.iter().map(Iterator::collect).collect();
    assert_eq!(lists, [vec![Some(1), None]]);

    // A reachable null item at a non-nullable level is rejected:
    let array = LargeListArray::from_iter_primitive::<Int64Type, _, _>(vec![Some(vec![None])]);
    let result = Column::<LargeList<i64>>::try_from(Arc::new(array) as ArrayRef);
    assert!(matches!(
        result,
        Err(ColumnError::UnexpectedNulls { null_count: 1 })
    ));

    // Nullable rows:
    let array =
        LargeListArray::from_iter_primitive::<Int64Type, _, _>(vec![Some(vec![Some(1)]), None]);
    let column = Column::<Option<LargeList<i64>>>::try_from(Arc::new(array) as ArrayRef).unwrap();
    let lists: Vec<Option<Vec<i64>>> = column
        .iter()
        .map(|row| row.map(Iterator::collect))
        .collect();
    assert_eq!(lists, [Some(vec![1]), None]);

    // Nested in a List:
    let column = Column::<LargeList<List<Utf8>>>::from_values([vec![vec!["a".to_owned()]]]);
    assert_eq!(column.len(), 1);
}

/// Domain newtypes via `newtype_datatype!`.
#[derive(Debug, PartialEq)]
struct SensorName(String);

impl From<String> for SensorName {
    fn from(name: String) -> Self {
        Self(name)
    }
}
impl From<SensorName> for String {
    fn from(name: SensorName) -> Self {
        name.0
    }
}

quiver::newtype_datatype!(SensorName, Utf8);

/// A `[u8; 16]`-backed newtype.
#[derive(Debug, PartialEq, Clone, Copy)]
struct ChunkId([u8; 16]);

impl From<[u8; 16]> for ChunkId {
    fn from(id: [u8; 16]) -> Self {
        Self(id)
    }
}
impl From<ChunkId> for [u8; 16] {
    fn from(id: ChunkId) -> Self {
        id.0
    }
}

quiver::newtype_datatype!(ChunkId, [u8; 16], primitive);

/// A `bool`-backed newtype: `bool` has no `RefDatatype` (bit-packed),
/// so the `Index` support must be opted out of with `noref`.
#[derive(Debug, PartialEq, Clone, Copy)]
struct IsActive(bool);

impl From<bool> for IsActive {
    fn from(active: bool) -> Self {
        Self(active)
    }
}
impl From<IsActive> for bool {
    fn from(active: IsActive) -> Self {
        active.0
    }
}

quiver::newtype_datatype!(IsActive, bool, noref);

#[test]
fn newtype_columns() {
    let column = Column::<SensorName>::from_values([
        SensorName("kitchen".to_owned()),
        SensorName("attic".to_owned()),
    ]);
    assert_eq!(Column::<SensorName>::datatype(), DataType::Utf8);

    // Reading is zero-copy, yielding the repr's borrowed value:
    let values: Vec<&str> = column.iter().collect();
    assert_eq!(values, ["kitchen", "attic"]);

    // Indexing borrows through the repr:
    assert_eq!(&column[1], "attic");

    // Owned values are the newtype:
    assert_eq!(
        column.to_vec(),
        [
            SensorName("kitchen".to_owned()),
            SensorName("attic".to_owned())
        ]
    );

    // Composes like any logical type:
    let column = Column::<Option<ChunkId>>::from_nullable_values([Some(ChunkId([7; 16])), None]);
    assert_eq!(column.to_vec(), [Some(ChunkId([7; 16])), None]);
    assert_eq!(Column::<ChunkId>::datatype(), DataType::FixedSizeBinary(16));

    // The `primitive` arm enables bulk zero-copy reads, yielding the repr's values:
    let column = Column::<ChunkId>::from_values([ChunkId([7; 16]), ChunkId([8; 16])]);
    assert_eq!(column.as_slice(), &[[7_u8; 16], [8; 16]]);

    let column = Column::<List<SensorName>>::from_values([vec![SensorName("a".to_owned())]]);
    assert_eq!(column.to_vec(), [vec![SensorName("a".to_owned())]]);

    // `noref` newtypes still read normally (just no `column[index]`):
    let column = Column::<IsActive>::from_values([IsActive(true), IsActive(false)]);
    assert!(column.value(0));
    assert_eq!(column.to_vec(), [IsActive(true), IsActive(false)]);
}

#[test]
fn as_adapter_for_foreign_types() {
    use std::net::Ipv4Addr;

    use quiver::As;

    // `Ipv4Addr` is a foreign type: no `newtype_datatype!` possible (orphan rule).
    let column = Column::<As<Ipv4Addr, u32>>::from_values([
        Ipv4Addr::LOCALHOST,
        Ipv4Addr::new(192, 168, 0, 1),
    ]);
    assert_eq!(Column::<As<Ipv4Addr, u32>>::datatype(), DataType::UInt32);

    // Reading is zero-copy, yielding the repr's value:
    assert_eq!(column.value(0), u32::from(Ipv4Addr::LOCALHOST));

    // Owned values are the foreign type:
    assert_eq!(
        column.to_vec(),
        [Ipv4Addr::LOCALHOST, Ipv4Addr::new(192, 168, 0, 1)]
    );

    // Composes like any logical type:
    let column = Column::<Option<As<Ipv4Addr, u32>>>::from_nullable_values([
        Some(Ipv4Addr::LOCALHOST),
        None,
    ]);
    assert_eq!(column.to_vec(), [Some(Ipv4Addr::LOCALHOST), None]);

    let column = Column::<List<As<Ipv4Addr, u32>>>::from_values([vec![Ipv4Addr::LOCALHOST]]);
    assert_eq!(column.to_vec(), [vec![Ipv4Addr::LOCALHOST]]);
}

/// A custom logical type that overrides [`quiver::Datatype::matches`]:
/// it accepts both `Int32` and `Int64` arrays, reading every value as `i64`.
struct AnyInt;

impl quiver::Datatype for AnyInt {
    type Typed = ArrayRef;
    type Value<'a> = i64;
    type Owned = i64;

    /// The canonical datatype: used when encoding, and in error messages.
    fn datatype() -> DataType {
        DataType::Int64
    }

    fn matches(actual: &DataType) -> bool {
        matches!(actual, DataType::Int32 | DataType::Int64)
    }

    fn downcast(
        array: &dyn quiver::arrow::array::Array,
    ) -> Result<Self::Typed, quiver::ColumnError> {
        Ok(quiver::arrow::array::make_array(array.to_data()))
    }

    fn is_null(typed: &Self::Typed, index: usize) -> bool {
        typed.is_null(index)
    }

    fn value(typed: &Self::Typed, index: usize) -> i64 {
        use quiver::arrow::array::AsArray as _;
        match typed.data_type() {
            DataType::Int32 => i64::from(typed.as_primitive::<Int32Type>().value(index)),
            DataType::Int64 => typed.as_primitive::<Int64Type>().value(index),
            _ => unreachable!("`matches` only accepts Int32 and Int64"),
        }
    }

    fn build(values: impl Iterator<Item = Option<i64>>) -> Result<ArrayRef, quiver::ColumnError> {
        Ok(Arc::new(values.collect::<Int64Array>()))
    }

    fn to_owned_value(value: i64) -> i64 {
        value
    }
}

#[test]
fn custom_matches_hook() {
    use quiver::arrow::array::Int32Array;

    // The override accepts both integer widths:
    let from_i32 = Column::<AnyInt>::try_new(Arc::new(Int32Array::from(vec![1, 2]))).unwrap();
    let from_i64 = Column::<AnyInt>::try_new(Arc::new(Int64Array::from(vec![3]))).unwrap();
    assert_eq!(from_i32.to_vec(), [1, 2]);
    assert_eq!(from_i64.to_vec(), [3]);

    // …but nothing else:
    let err = Column::<AnyInt>::try_new(Arc::new(StringArray::from(vec!["nope"]))).unwrap_err();
    assert!(matches!(err, ColumnError::WrongDatatype { .. }));

    // Containers forward to the inner `matches`, at any nesting depth:
    let int32_items =
        ListArray::from_iter_primitive::<Int32Type, _, _>(vec![Some(vec![Some(1), Some(2)])]);
    let lists = Column::<List<Option<AnyInt>>>::try_new(Arc::new(int32_items)).unwrap();
    let items: Vec<Option<i64>> = lists.value(0).collect();
    assert_eq!(items, [Some(1), Some(2)]);

    // `Option<…>` forwards too:
    let nullable =
        Column::<Option<AnyInt>>::try_new(Arc::new(Int32Array::from(vec![Some(7), None]))).unwrap();
    assert_eq!(nullable.to_vec(), [Some(7), None]);
}

#[test]
fn utf8_string_encodings() {
    use quiver::{LargeUtf8, Utf8View};

    // All three string encodings build from and yield the same values:
    let plain = Column::<Utf8>::from_values(["a", "b"]);
    let large = Column::<LargeUtf8>::from_values(["a", "b"]);
    let view = Column::<Utf8View>::from_values(["a", "b"]);

    assert_eq!(Column::<Utf8>::datatype(), DataType::Utf8);
    assert_eq!(Column::<LargeUtf8>::datatype(), DataType::LargeUtf8);
    assert_eq!(Column::<Utf8View>::datatype(), DataType::Utf8View);

    for column in [&plain.to_vec(), &large.to_vec(), &view.to_vec()] {
        assert_eq!(column, &["a".to_owned(), "b".to_owned()]);
    }

    // Zero-copy reads and indexing work for all of them:
    assert_eq!(view.value(1), "b");
    assert_eq!(&view[0], "a");

    // Nullable views too:
    let nullable = Column::<Option<Utf8View>>::from_nullable_values([Some("a"), None]);
    let values: Vec<Option<&str>> = nullable.iter().collect();
    assert_eq!(values, [Some("a"), None]);
}
