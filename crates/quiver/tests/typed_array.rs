//! Tests for [`quiver::TypedArray`]: a [`quiver::Column`] without the name or
//! the metadata.
//!
//! Everything that does not depend on a column having a name lives here —
//! validation, element access, iteration, slicing, and every encoding.
//! The name, the metadata, and the record batch boundary are tested on
//! `Column`, in `column.rs`.

use std::sync::Arc;

use quiver::arrow::array::Array as _;
use quiver::arrow::array::{
    ArrayRef, DurationMillisecondArray, FixedSizeBinaryArray, Int64Array, ListArray, StringArray,
    TimestampNanosecondArray, TimestampSecondArray,
};
use quiver::arrow::datatypes::{DataType, Field, Int32Type, Int64Type};
use quiver::arrow::error::ArrowError;
use quiver::arrow::record_batch::RecordBatch;
use quiver::{
    Column, ColumnError, Duration, FixedSizeBinary, List, Millisecond, Nanosecond, Second,
    Timestamp, TypedArray, Utc, Utf8,
};

#[test]
fn build_and_read() {
    let array = TypedArray::<Utf8>::from_values(["foo", "bar"]);
    assert_eq!(array.len(), 2);
    assert_eq!(array.value(1), "bar");
    assert_eq!(&array[1], "bar");
    assert_eq!(array.get(2), None);
    assert_eq!(array.get_owned(0).as_deref(), Some("foo"));
    assert_eq!(array.to_vec(), ["foo", "bar"]);
    assert_eq!(array.iter().collect::<Vec<_>>(), ["foo", "bar"]);
    assert_eq!(array.iter().rev().collect::<Vec<_>>(), ["bar", "foo"]);

    // Empty, and empty edges:
    let empty = TypedArray::<Utf8>::default();
    assert!(empty.is_empty());
    assert_eq!(empty.iter().next(), None);
    assert_eq!(array.slice(2, 0).len(), 0);

    // A slice is zero-copy and still validated:
    let tail = array.slice(1, 1);
    assert_eq!(tail.to_vec(), ["bar"]);
}

#[test]
fn nulls_and_nullable() {
    let with_nulls: ArrayRef = Arc::new(Int64Array::from(vec![Some(1), None]));

    // The non-nullable logical type rejects the nulls:
    assert!(matches!(
        TypedArray::<i64>::try_new(Arc::clone(&with_nulls)),
        Err(ColumnError::UnexpectedNulls { null_count: 1 })
    ));

    let array = TypedArray::<Option<i64>>::try_new(with_nulls).unwrap();
    assert_eq!(array.to_vec(), [Some(1), None]);
    assert_eq!(array, TypedArray::from_nullable_values([Some(1_i64), None]));

    // A wrong data type is rejected too:
    assert!(matches!(
        TypedArray::<Option<i64>>::try_new(Arc::new(StringArray::from(vec!["1"]))),
        Err(ColumnError::WrongDataType { .. })
    ));
}

#[test]
fn empty_arrays() {
    let array = TypedArray::<i64>::default();
    assert!(array.is_empty());

    let array = TypedArray::<List<Option<Utf8>>>::default();
    assert!(array.is_empty());
    assert_eq!(array.iter().count(), 0);

    let array = TypedArray::<Timestamp<Nanosecond, Utc>>::default();
    assert!(array.is_empty());

    let array = TypedArray::<FixedSizeBinary<16>>::default();
    assert_eq!(array.as_arrow().data_type(), &DataType::FixedSizeBinary(16));
}

#[test]
fn nested_and_slices() {
    let lists = TypedArray::<List<Utf8>>::from_values([vec!["a".to_owned(), "b".to_owned()]]);
    assert_eq!(lists.value(0).collect::<Vec<_>>(), ["a", "b"]);

    let numbers: TypedArray<i64> = [1_i64, 2, 3].into_iter().collect();
    assert_eq!(numbers.as_slice(), &[1, 2, 3]);
    assert_eq!(numbers.into_iter_owned().sum::<i64>(), 6);
}

#[test]
fn converts_to_and_from_column() {
    let mut column = Column::<Utf8>::from_values("sensor", ["foo"]);
    column
        .metadata_mut()
        .insert("unit".to_owned(), "name".to_owned());

    // Dropping to the data half loses the name and the metadata;
    // naming it again gets neither back.
    let array: TypedArray<Utf8> = column.clone().into_typed_array();
    assert_eq!(column.as_typed_array(), &array);
    let renamed = Column::new("other", array);
    assert_eq!(renamed.name(), "other");
    assert!(renamed.metadata().is_empty());

    // The arrow array survives the round trip:
    assert_eq!(
        &TypedArray::<Utf8>::try_new(column.into_arrow())
            .unwrap()
            .into_arrow()
            .as_ref(),
        &(&StringArray::from(vec!["foo"]) as &dyn quiver::arrow::array::Array)
    );
}

/// The pre-rename iterator names still resolve, for one deprecation cycle.
#[test]
#[expect(deprecated)]
fn deprecated_iterator_aliases() {
    let column = Column::<Utf8>::from_values("sensor", ["foo", "bar"]);
    let borrowed: quiver::ColumnIter<'_, Utf8> = column.iter();
    assert_eq!(borrowed.count(), 2);

    let owned: quiver::ColumnIntoIter<Utf8> = column.into_iter_owned();
    assert_eq!(owned.count(), 2);
}

#[test]
fn flat_array() {
    let dynamic_array: ArrayRef = Arc::new(StringArray::from(vec!["foo", "bar"]));

    let array = TypedArray::<Utf8>::try_from(dynamic_array).unwrap();
    assert_eq!(array.len(), 2);
    assert_eq!(array.value(0), "foo");
    assert_eq!(array.get(2), None);

    let strings: Vec<&str> = array.iter().collect();
    assert_eq!(strings, ["foo", "bar"]);
}

#[test]
fn nullable_array() {
    let dynamic_array: ArrayRef = Arc::new(Int64Array::from(vec![Some(1), None]));

    // Non-nullable logical type rejects the nulls:
    let result = TypedArray::<i64>::try_from(Arc::clone(&dynamic_array));
    assert!(matches!(
        result,
        Err(ColumnError::UnexpectedNulls { null_count: 1 })
    ));

    // Nullable logical type accepts them:
    let array = TypedArray::<Option<i64>>::try_from(dynamic_array).unwrap();
    let values: Vec<Option<i64>> = array.iter().collect();
    assert_eq!(values, [Some(1), None]);
}

#[test]
fn list_array() {
    let dynamic_array: ArrayRef =
        Arc::new(ListArray::from_iter_primitive::<Int64Type, _, _>(vec![
            Some(vec![Some(1), Some(2)]),
            Some(vec![Some(3)]),
        ]));

    // The item field is declared nullable but contains no nulls,
    // so both `List<i64>` and `List<Option<i64>>` accept it
    // (inner field nullability flags are not compared — actual nulls are what matters):
    let array = TypedArray::<List<i64>>::try_from(Arc::clone(&dynamic_array)).unwrap();
    let lists: Vec<Vec<i64>> = array.iter().map(Iterator::collect).collect();
    assert_eq!(lists, [vec![1, 2], vec![3]]);

    let array = TypedArray::<List<Option<i64>>>::try_from(dynamic_array).unwrap();
    let lists: Vec<Vec<Option<i64>>> = array.iter().map(Iterator::collect).collect();
    assert_eq!(lists, [vec![Some(1), Some(2)], vec![Some(3)]]);
}

#[test]
fn wrong_data_type() {
    let dynamic_array: ArrayRef = Arc::new(Int64Array::from(vec![1]));

    let result = TypedArray::<Utf8>::try_from(dynamic_array);
    assert!(matches!(
        result,
        Err(ColumnError::WrongDataType {
            expected,
            actual: DataType::Int64,
        }) if expected == "Utf8"
    ));

    // A wrong data type that *also* has nulls reports the data type mismatch,
    // not `UnexpectedNulls` — the data type check wins.
    let nullable: ArrayRef = Arc::new(StringArray::from(vec![Some("a"), None]));
    let result = TypedArray::<i64>::try_from(nullable);
    assert!(matches!(
        result,
        Err(ColumnError::WrongDataType {
            actual: DataType::Utf8,
            ..
        })
    ));
}

#[test]
fn nested_list() {
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

    let array = TypedArray::<List<List<Utf8>>>::try_from(Arc::new(outer) as ArrayRef).unwrap();
    let nested: Vec<Vec<Vec<&str>>> = array
        .iter()
        .map(|outer| outer.map(Iterator::collect).collect())
        .collect();
    assert_eq!(nested, [vec![vec!["a"], vec!["b", "c"]]]);
}

#[test]
fn fixed_size_binary_array() {
    let dynamic_array: ArrayRef = Arc::new(
        FixedSizeBinaryArray::try_from_iter(vec![[1_u8; 16], [2; 16]].into_iter()).unwrap(),
    );

    // Wrong size is rejected:
    let result = TypedArray::<FixedSizeBinary<8>>::try_from(Arc::clone(&dynamic_array));
    assert!(matches!(
        result,
        Err(ColumnError::WrongDataType {
            expected,
            actual: DataType::FixedSizeBinary(16),
        }) if expected == "FixedSizeBinary(8)"
    ));

    // Matching size:
    let array = TypedArray::<FixedSizeBinary<16>>::try_from(dynamic_array).unwrap();
    assert_eq!(array.value(0), &[1_u8; 16]);
    let values: Vec<&[u8; 16]> = array.iter().collect();
    assert_eq!(values, [&[1_u8; 16], &[2; 16]]);
}

#[test]
fn nullable_fixed_size_binary_array() {
    let dynamic_array: ArrayRef = Arc::new(
        FixedSizeBinaryArray::try_from_sparse_iter_with_size(
            vec![Some([1_u8; 4]), None].into_iter(),
            4,
        )
        .unwrap(),
    );

    // Non-nullable logical type rejects the nulls:
    let result = TypedArray::<FixedSizeBinary<4>>::try_from(Arc::clone(&dynamic_array));
    assert!(matches!(
        result,
        Err(ColumnError::UnexpectedNulls { null_count: 1 })
    ));

    // Nullable logical type accepts them:
    let array = TypedArray::<Option<FixedSizeBinary<4>>>::try_from(dynamic_array).unwrap();
    let values: Vec<Option<&[u8; 4]>> = array.iter().collect();
    assert_eq!(values, [Some(&[1_u8; 4]), None]);
}

#[test]
fn timestamp_array() {
    let naive: ArrayRef = Arc::new(TimestampNanosecondArray::from(vec![1, 2]));
    let utc: ArrayRef = Arc::new(TimestampNanosecondArray::from(vec![1, 2]).with_timezone("UTC"));

    // Timezone-naive:
    let array = TypedArray::<Timestamp<Nanosecond>>::try_from(Arc::clone(&naive)).unwrap();
    let values: Vec<i64> = array.iter().collect();
    assert_eq!(values, [1, 2]);

    // Timezones are matched exactly, in both directions:
    assert!(matches!(
        TypedArray::<Timestamp<Nanosecond>>::try_from(Arc::clone(&utc)),
        Err(ColumnError::WrongDataType { .. })
    ));
    assert!(matches!(
        TypedArray::<Timestamp<Nanosecond, Utc>>::try_from(naive),
        Err(ColumnError::WrongDataType { .. })
    ));

    let array = TypedArray::<Timestamp<Nanosecond, Utc>>::try_from(utc).unwrap();
    assert_eq!(array.value(1), 2);

    // The unit must match, too:
    let seconds: ArrayRef = Arc::new(TimestampSecondArray::from(vec![1]));
    assert!(matches!(
        TypedArray::<Timestamp<Nanosecond>>::try_from(Arc::clone(&seconds)),
        Err(ColumnError::WrongDataType { .. })
    ));
    let array = TypedArray::<Timestamp<Second>>::try_from(seconds).unwrap();
    assert_eq!(array.value(0), 1);
}

#[test]
fn nullable_timestamp_array() {
    let arrow_array: ArrayRef = Arc::new(TimestampNanosecondArray::from(vec![Some(1), None]));

    assert!(matches!(
        TypedArray::<Timestamp<Nanosecond>>::try_from(Arc::clone(&arrow_array)),
        Err(ColumnError::UnexpectedNulls { null_count: 1 })
    ));

    let array = TypedArray::<Option<Timestamp<Nanosecond>>>::try_from(arrow_array).unwrap();
    let values: Vec<Option<i64>> = array.iter().collect();
    assert_eq!(values, [Some(1), None]);
}

/// Run-end encoding has no validity of its own: the nulls belong in the values,
/// so `Option<Run<K, V>>` is unbuildable by any route, `new_null` included.
#[test]
#[should_panic(expected = "run-end encoding")]
fn new_null_run_end_encoded() {
    let _column: TypedArray<Option<quiver::Run<i32, Utf8>>> = TypedArray::new_null(2);
}

#[test]
fn duration_array() {
    let arrow_array: ArrayRef = Arc::new(DurationMillisecondArray::from(vec![100, 200]));

    // The unit must match:
    assert!(matches!(
        TypedArray::<Duration<Nanosecond>>::try_from(Arc::clone(&arrow_array)),
        Err(ColumnError::WrongDataType { .. })
    ));

    let array = TypedArray::<Duration<Millisecond>>::try_from(arrow_array).unwrap();
    let values: Vec<i64> = array.iter().collect();
    assert_eq!(values, [100, 200]);

    // Nullable:
    let arrow_array: ArrayRef = Arc::new(DurationMillisecondArray::from(vec![Some(1), None]));
    assert!(matches!(
        TypedArray::<Duration<Millisecond>>::try_from(Arc::clone(&arrow_array)),
        Err(ColumnError::UnexpectedNulls { null_count: 1 })
    ));
    let array = TypedArray::<Option<Duration<Millisecond>>>::try_from(arrow_array).unwrap();
    let values: Vec<Option<i64>> = array.iter().collect();
    assert_eq!(values, [Some(1), None]);
}

#[test]
fn errors_convert_to_arrow_error() {
    // So that `?` works in functions returning arrow results:
    fn parse(array: ArrayRef) -> Result<TypedArray<i64>, ArrowError> {
        Ok(TypedArray::try_new(array)?)
    }

    let err = parse(Arc::new(StringArray::from(vec!["nope"])) as ArrayRef)
        .err()
        .unwrap();
    assert!(matches!(err, ArrowError::ExternalError(_)));
    assert!(err.to_string().contains("Expected Int64, found Utf8"));
}

#[test]
fn convenience_constructors() {
    // From anything that converts into the owned value (e.g. `&str` → `String`):
    let array = TypedArray::<Utf8>::from_values(["a", "b"]);
    let values: Vec<&str> = array.iter().collect();
    assert_eq!(values, ["a", "b"]);

    // `From<Vec<T>>`:
    let array: TypedArray<i64> = vec![1, 2].into();
    let values: Vec<i64> = array.iter().collect();
    assert_eq!(values, [1, 2]);

    // `FromIterator`:
    let array: TypedArray<f64> = [1.0, 2.5].into_iter().collect();
    assert_eq!(array.value(1), 2.5);

    // Nullable values:
    let array = TypedArray::<Option<i64>>::from_values([Some(1), None]);
    let values: Vec<Option<i64>> = array.iter().collect();
    assert_eq!(values, [Some(1), None]);

    // Lists:
    let array = TypedArray::<List<i64>>::from_values([vec![1, 2], vec![3]]);
    let values: Vec<Vec<i64>> = array.iter().map(Iterator::collect).collect();
    assert_eq!(values, [vec![1, 2], vec![3]]);

    // Nullable lists with nullable items:
    let array =
        TypedArray::<Option<List<Option<i64>>>>::from_values([Some(vec![Some(1), None]), None]);
    let values: Vec<Option<Vec<Option<i64>>>> = array
        .iter()
        .map(|list| list.map(Iterator::collect))
        .collect();
    assert_eq!(values, [Some(vec![Some(1), None]), None]);

    // Fixed-size binary:
    let array = TypedArray::<FixedSizeBinary<4>>::from_values([[1_u8, 2, 3, 4], [5, 6, 7, 8]]);
    assert_eq!(array.value(1), &[5, 6, 7, 8]);

    // Timestamps get the declared timezone:
    let array = TypedArray::<Timestamp<Nanosecond, Utc>>::from_values([1_i64, 2]);
    assert_eq!(
        array.as_arrow().data_type(),
        &DataType::Timestamp(
            quiver::arrow::datatypes::TimeUnit::Nanosecond,
            Some("UTC".into())
        )
    );

    // Durations:
    let array = TypedArray::<Duration<Millisecond>>::from_values([100_i64]);
    assert_eq!(array.value(0), 100);
}

#[test]
fn to_vec_and_iter_owned() {
    let array = TypedArray::<Utf8>::from_values(["a", "b"]);
    let owned: Vec<String> = array.to_vec();
    assert_eq!(owned, ["a".to_owned(), "b".to_owned()]);

    let array = TypedArray::<Option<Utf8>>::from_values([Some("a".to_owned()), None]);
    assert_eq!(array.to_vec(), [Some("a".to_owned()), None]);

    let array = TypedArray::<List<i64>>::from_values([vec![1, 2], vec![3]]);
    assert_eq!(array.to_vec(), [vec![1, 2], vec![3]]);

    let array = TypedArray::<FixedSizeBinary<2>>::from_values([[1_u8, 2], [3, 4]]);
    assert_eq!(array.to_vec(), [[1_u8, 2], [3, 4]]);

    let total: i64 = TypedArray::<i64>::from_values([1, 2, 3]).iter_owned().sum();
    assert_eq!(total, 6);
}

#[test]
fn iter_combinators_and_double_ended() {
    let array = TypedArray::<i64>::from_values([10, 20, 30, 40]);

    // Overridden forward combinators:
    assert_eq!(array.iter().count(), 4);
    assert_eq!(array.iter().last(), Some(40));
    assert_eq!(array.iter().nth(2), Some(30));
    assert_eq!(array.iter().nth(4), None);
    let sum: i64 = array.iter().sum(); // routes through `fold`
    assert_eq!(sum, 100);

    // Double-ended (borrowing iterator):
    let rev: Vec<i64> = array.iter().rev().collect();
    assert_eq!(rev, [40, 30, 20, 10]);
    let mut cursor = array.iter();
    assert_eq!(cursor.next(), Some(10));
    assert_eq!(cursor.next_back(), Some(40));
    assert_eq!(cursor.next_back(), Some(30));
    assert_eq!(cursor.next(), Some(20));
    assert_eq!(cursor.next(), None);
    assert_eq!(cursor.next_back(), None);

    // Owning iterator (`TypedArray::into_iter_owned`): nth, rev, next_back.
    assert_eq!(array.clone().into_iter_owned().nth(1), Some(20));
    let owned_rev: Vec<i64> = array.clone().into_iter_owned().rev().collect();
    assert_eq!(owned_rev, [40, 30, 20, 10]);

    // Strings exercise the borrowed-view unchecked path:
    let strings = TypedArray::<Utf8>::from_values(["a", "b", "c"]);
    let joined: String = strings.iter().rev().collect();
    assert_eq!(joined, "cba");
}

#[test]
fn nested_list_iteration() {
    // `List<List<i64>>`: unchecked reads must propagate through both levels.
    let array = TypedArray::<List<List<i64>>>::from_values([
        vec![vec![1, 2], vec![3]],
        vec![vec![4, 5, 6]],
    ]);

    let flattened: Vec<i64> = array
        .iter()
        .flat_map(|row| row.flat_map(|inner| inner.collect::<Vec<_>>()))
        .collect();
    assert_eq!(flattened, [1, 2, 3, 4, 5, 6]);

    // Random access through both levels, plus reverse iteration of the inner list:
    let first_row = array.value(0);
    assert_eq!(first_row.value(0).to_vec(), vec![1, 2]);
    let inner_rev: Vec<i64> = first_row.value(0).rev().collect();
    assert_eq!(inner_rev, [2, 1]);
}

#[test]
fn as_slice() {
    let array = TypedArray::<f32>::from_values([1.0_f32, 2.0, 3.0]);
    assert_eq!(array.as_slice(), &[1.0, 2.0, 3.0]);

    let array = TypedArray::<u8>::from_values([1_u8, 2, 3]);
    assert_eq!(array.as_slice(), &[1, 2, 3]);

    // Markers expose their native values:
    let array = TypedArray::<Timestamp<Nanosecond, Utc>>::from_values([10_i64, 20]);
    assert_eq!(array.as_slice(), &[10_i64, 20]);

    let array = TypedArray::<Duration<Millisecond>>::from_values([10_i64, 20]);
    assert_eq!(array.as_slice(), &[10_i64, 20]);

    // The `As` adapter exposes the representation's values:
    let array = TypedArray::<quiver::As<std::net::Ipv4Addr, u32>>::from_values([
        std::net::Ipv4Addr::LOCALHOST,
    ]);
    assert_eq!(
        array.as_slice(),
        &[u32::from(std::net::Ipv4Addr::LOCALHOST)]
    );
}

#[test]
fn as_slice_fixed_size_binary() {
    // Bulk zero-copy read of fixed-size binary arrays:
    let array = TypedArray::<FixedSizeBinary<4>>::from_values([[1_u8, 2, 3, 4], [5, 6, 7, 8]]);
    assert_eq!(array.as_slice(), &[[1_u8, 2, 3, 4], [5, 6, 7, 8]]);

    // Also when parsed from a raw arrow array:
    let arrow_array: ArrayRef = Arc::new(
        FixedSizeBinaryArray::try_from_iter(vec![[1_u8; 16], [2; 16]].into_iter()).unwrap(),
    );
    let array = TypedArray::<FixedSizeBinary<16>>::try_from(arrow_array).unwrap();
    assert_eq!(array.as_slice(), &[[1_u8; 16], [2; 16]]);

    // Empty:
    let array = TypedArray::<FixedSizeBinary<4>>::default();
    assert_eq!(array.as_slice(), &[] as &[[u8; 4]]);
}

#[test]
fn as_slice_respects_offset() {
    let array = TypedArray::<i64>::from_values([1, 2, 3, 4, 5]);
    let sliced = array.slice(1, 3);
    assert_eq!(sliced.as_slice(), &[2, 3, 4]);

    // Fixed-size binary too — the byte window must follow the slice:
    let array = TypedArray::<FixedSizeBinary<2>>::from_values([[1_u8, 2], [3, 4], [5, 6], [7, 8]]);
    let sliced = array.slice(1, 2);
    assert_eq!(sliced.as_slice(), &[[3_u8, 4], [5, 6]]);
}

#[test]
fn index() {
    let strings = TypedArray::<Utf8>::from_values(["a", "b"]);
    assert_eq!(&strings[0], "a");
    assert_eq!(&strings[1], "b");

    let numbers = TypedArray::<i64>::from_values([1, 2, 3]);
    assert_eq!(numbers[2], 3);

    let binary = TypedArray::<quiver::Binary>::from_values([vec![1_u8, 2], vec![3]]);
    assert_eq!(&binary[1], [3_u8]);

    let uuids = TypedArray::<FixedSizeBinary<4>>::from_values([[1_u8, 2, 3, 4]]);
    assert_eq!(uuids[0], [1_u8, 2, 3, 4]);

    let timestamps = TypedArray::<Timestamp<Nanosecond, Utc>>::from_values([10_i64, 20]);
    assert_eq!(timestamps[1], 20);

    // Dictionary values are looked up through the keys:
    let tags: TypedArray<quiver::Dictionary<i32, Utf8>> = vec!["a", "b", "a"].try_into().unwrap();
    assert_eq!(&tags[2], "a");

    // The `As` adapter yields the representation's reference:
    let ips = TypedArray::<quiver::As<std::net::Ipv4Addr, u32>>::from_values([
        std::net::Ipv4Addr::LOCALHOST,
    ]);
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
    let strings = TypedArray::<Utf8>::from_values(["a", "b"]);
    let _: &str = &strings[2];
}

#[test]
fn value_owned_and_get_owned() {
    let array = TypedArray::<Utf8>::from_values(["a", "b"]);
    let owned: String = array.value_owned(1);
    assert_eq!(owned, "b");
    assert_eq!(array.get_owned(0), Some("a".to_owned()));
    assert_eq!(array.get_owned(2), None);

    // The owned value of a newtype array is the newtype:
    let array = TypedArray::<SensorName>::from_values([SensorName("kitchen".to_owned())]);
    assert_eq!(array.value_owned(0), SensorName("kitchen".to_owned()));
    assert_eq!(array.get_owned(0), Some(SensorName("kitchen".to_owned())));

    let array = TypedArray::<Option<i64>>::from_values([Some(1), None]);
    assert_eq!(array.value_owned(1), None);
    assert_eq!(array.get_owned(1), Some(None));
    assert_eq!(array.get_owned(2), None);
}

#[test]
#[should_panic(expected = "Index 1 out of bounds")]
fn value_owned_out_of_bounds() {
    let array = TypedArray::<Utf8>::from_values(["a"]);
    let _value: String = array.value_owned(1);
}

#[test]
fn nullable_construction_ergonomics() {
    // Owned values work directly:
    let array: TypedArray<Option<Utf8>> = vec![Some("a".to_owned()), None].into();
    assert_eq!(array.to_vec(), [Some("a".to_owned()), None]);

    // Borrowed values need `from_nullable_values`
    // (std has no `From<Option<&str>> for Option<String>`):
    let array = TypedArray::<Option<Utf8>>::from_nullable_values([Some("a"), None]);
    assert_eq!(array.to_vec(), [Some("a".to_owned()), None]);

    let array = TypedArray::<Option<List<i64>>>::from_nullable_values([Some(vec![1, 2]), None]);
    assert_eq!(array.to_vec(), [Some(vec![1, 2]), None]);
}

#[test]
fn into_iterator() {
    let array = TypedArray::<Utf8>::from_values(["a", "b"]);

    // By reference: borrowed values.
    let mut borrowed = Vec::new();
    for value in &array {
        borrowed.push(value); // `&str`
    }
    assert_eq!(borrowed, ["a", "b"]);

    // By value: owned values, opt-in via `into_iter_owned`.
    let mut owned = Vec::new();
    for value in array.into_iter_owned() {
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
        TypedArray::<TimestampNanosecond<Utc>>::data_type(),
        TypedArray::<Timestamp<Nanosecond, Utc>>::data_type()
    );
    assert_eq!(
        TypedArray::<TimestampNanosecond>::data_type(), // timezone-naive default
        TypedArray::<Timestamp<Nanosecond>>::data_type()
    );
    assert_eq!(
        TypedArray::<DurationMillisecond>::data_type(),
        TypedArray::<Duration<Millisecond>>::data_type()
    );
}

#[test]
fn binary_arrays() {
    use quiver::{Binary, LargeBinary};

    let array = TypedArray::<Binary>::from_values([b"abc".to_vec(), vec![0_u8, 1]]);
    assert_eq!(array.value(0), b"abc");
    assert_eq!(array.to_vec(), [b"abc".to_vec(), vec![0_u8, 1]]);
    assert_eq!(TypedArray::<Binary>::data_type(), DataType::Binary);

    let array = TypedArray::<LargeBinary>::from_values([b"abc".to_vec()]);
    assert_eq!(
        TypedArray::<LargeBinary>::data_type(),
        DataType::LargeBinary
    );
    assert_eq!(array.value(0), b"abc");

    // Binary ≠ LargeBinary:
    let result = TypedArray::<Binary>::try_from(array.into_arrow());
    assert!(matches!(result, Err(ColumnError::WrongDataType { .. })));

    // Nullable:
    let array = TypedArray::<Option<Binary>>::from_nullable_values([Some(b"abc".to_vec()), None]);
    assert_eq!(array.to_vec(), [Some(b"abc".to_vec()), None]);

    // Lists of binary:
    let array = TypedArray::<List<Binary>>::from_values([vec![b"a".to_vec(), b"b".to_vec()]]);
    let lists: Vec<Vec<Vec<u8>>> = array.to_vec();
    assert_eq!(lists, [vec![b"a".to_vec(), b"b".to_vec()]]);
}

#[test]
fn binary_view_arrays() {
    use quiver::arrow::array::BinaryViewArray;
    use quiver::{Binary, BinaryView};

    let array = TypedArray::<BinaryView>::from_values([b"abc".to_vec(), vec![0_u8, 1]]);
    assert_eq!(array.value(0), b"abc");
    assert_eq!(&array[1], &[0_u8, 1]);
    assert_eq!(array.to_vec(), [b"abc".to_vec(), vec![0_u8, 1]]);
    assert_eq!(TypedArray::<BinaryView>::data_type(), DataType::BinaryView);

    // BinaryView ≠ Binary:
    let result = TypedArray::<Binary>::try_from(array.into_arrow());
    assert!(matches!(result, Err(ColumnError::WrongDataType { .. })));

    // Nullable:
    let array =
        TypedArray::<Option<BinaryView>>::from_nullable_values([Some(b"abc".to_vec()), None]);
    assert_eq!(array.to_vec(), [Some(b"abc".to_vec()), None]);

    // Parsing an externally built array:
    let arrow_array = BinaryViewArray::from_iter_values([b"x".as_slice(), b"yz"]);
    let array = TypedArray::<BinaryView>::try_from(Arc::new(arrow_array) as ArrayRef).unwrap();
    assert_eq!(array.value(1), b"yz");

    // A null at a non-nullable level is rejected:
    let arrow_array = BinaryViewArray::from_iter([Some(b"x".as_slice()), None]);
    let result = TypedArray::<BinaryView>::try_from(Arc::new(arrow_array) as ArrayRef);
    assert!(matches!(
        result,
        Err(ColumnError::UnexpectedNulls { null_count: 1 })
    ));

    // Values longer than 12 bytes don't fit inline in the view and spill into a
    // separate data buffer (referenced by offset) — exercise that path:
    let short = b"short".to_vec(); // <= 12 bytes: stored inline
    let long = b"a value well over twelve bytes".to_vec(); // > 12 bytes: in a buffer
    let array = TypedArray::<BinaryView>::from_values([short.clone(), long.clone()]);
    assert_eq!(array.value(0), short.as_slice());
    assert_eq!(array.value(1), long.as_slice());
    assert_eq!(array.to_vec(), [short, long]);
}

#[test]
fn any_binary_arrays() {
    use quiver::arrow::array::{BinaryViewArray, FixedSizeBinaryArray, LargeBinaryArray};
    use quiver::{AnyBinary, Binary, BinaryView, FixedSizeBinary, LargeBinary};

    // `try_from` accepts every byte-string encoding, read uniformly as `&[u8]`:
    let encodings = [
        TypedArray::<Binary>::from_values([b"ab".to_vec(), vec![3_u8, 4]]).into_arrow(),
        TypedArray::<LargeBinary>::from_values([b"ab".to_vec(), vec![3_u8, 4]]).into_arrow(),
        TypedArray::<BinaryView>::from_values([b"ab".to_vec(), vec![3_u8, 4]]).into_arrow(),
        // FixedSizeBinary too (any size) — its `&[u8; N]` reads here as `&[u8]`:
        TypedArray::<FixedSizeBinary<2>>::from_values([[b'a', b'b'], [3, 4]]).into_arrow(),
    ];
    for arrow_array in encodings {
        let array = TypedArray::<AnyBinary>::try_from(arrow_array).unwrap();
        assert_eq!(array.value(0), b"ab");
        assert_eq!(&array[1], &[3_u8, 4]); // `RefType` indexing
        assert_eq!(array.to_vec(), [b"ab".to_vec(), vec![3, 4]]);
    }

    // A non-binary array is rejected:
    let ints = TypedArray::<i64>::from_values([1, 2]).into_arrow();
    assert!(matches!(
        TypedArray::<AnyBinary>::try_from(ints),
        Err(ColumnError::WrongDataType { .. })
    ));

    // Nullable rows via the array-level `Option`:
    let arrow_array = LargeBinaryArray::from_iter([Some(b"x".as_slice()), None]);
    let array =
        TypedArray::<Option<AnyBinary>>::try_from(Arc::new(arrow_array) as ArrayRef).unwrap();
    let values: Vec<Option<&[u8]>> = array.iter().collect();
    assert_eq!(values, [Some(b"x".as_slice()), None]);

    // A null at a non-nullable level is rejected:
    let arrow_array = BinaryViewArray::from_iter([Some(b"x".as_slice()), None]);
    assert!(matches!(
        TypedArray::<AnyBinary>::try_from(Arc::new(arrow_array) as ArrayRef),
        Err(ColumnError::UnexpectedNulls { null_count: 1 })
    ));

    // A FixedSizeBinary with a null is also rejected when non-nullable:
    let arrow_array = FixedSizeBinaryArray::try_from_sparse_iter_with_size(
        [Some([1_u8, 2]), None].into_iter(),
        2,
    )
    .unwrap();
    assert!(matches!(
        TypedArray::<AnyBinary>::try_from(Arc::new(arrow_array) as ArrayRef),
        Err(ColumnError::UnexpectedNulls { null_count: 1 })
    ));
}

#[test]
fn any_utf8_arrays() {
    use quiver::arrow::array::{LargeStringArray, StringViewArray};
    use quiver::{AnyUtf8, LargeUtf8, Utf8View};

    // `try_from` accepts every string encoding, read uniformly as `&str`:
    let encodings = [
        TypedArray::<Utf8>::from_values(["alice", "bob"]).into_arrow(),
        TypedArray::<LargeUtf8>::from_values(["alice", "bob"]).into_arrow(),
        TypedArray::<Utf8View>::from_values(["alice", "bob"]).into_arrow(),
    ];
    for arrow_array in encodings {
        let array = TypedArray::<AnyUtf8>::try_from(arrow_array).unwrap();
        assert_eq!(array.value(0), "alice");
        assert_eq!(&array[1], "bob"); // `RefType` indexing
        assert_eq!(array.to_vec(), ["alice", "bob"]);
    }

    // A non-string array is rejected:
    let ints = TypedArray::<i64>::from_values([1, 2]).into_arrow();
    assert!(matches!(
        TypedArray::<AnyUtf8>::try_from(ints),
        Err(ColumnError::WrongDataType { .. })
    ));

    // Nullable rows via the array-level `Option`:
    let arrow_array = LargeStringArray::from(vec![Some("x"), None]);
    let array = TypedArray::<Option<AnyUtf8>>::try_from(Arc::new(arrow_array) as ArrayRef).unwrap();
    let values: Vec<Option<&str>> = array.iter().collect();
    assert_eq!(values, [Some("x"), None]);

    // A null at a non-nullable level is rejected:
    let arrow_array = StringViewArray::from(vec![Some("x"), None]);
    assert!(matches!(
        TypedArray::<AnyUtf8>::try_from(Arc::new(arrow_array) as ArrayRef),
        Err(ColumnError::UnexpectedNulls { null_count: 1 })
    ));
}

#[test]
fn f16_array() {
    use quiver::half::f16;

    let array = TypedArray::<f16>::from_values([f16::from_f32(1.5), f16::from_f32(2.5)]);
    assert_eq!(TypedArray::<f16>::data_type(), DataType::Float16);
    assert_eq!(array.value(0), f16::from_f32(1.5));
    assert_eq!(array.iter().map(f16::to_f32).sum::<f32>(), 4.0);

    let array = TypedArray::<Option<f16>>::from_values([Some(f16::from_f32(1.5)), None]);
    assert_eq!(array.to_vec(), [Some(f16::from_f32(1.5)), None]);
}

#[test]
fn dictionary_arrays() {
    use quiver::Dictionary;
    use quiver::arrow::array::DictionaryArray;

    // Building dictionary-encodes the values:
    let array = TypedArray::<Dictionary<i32, Utf8>>::try_from_values(["a", "b", "a", "a"]).unwrap();
    assert_eq!(
        TypedArray::<Dictionary<i32, Utf8>>::data_type(),
        DataType::Dictionary(Box::new(DataType::Int32), Box::new(DataType::Utf8))
    );

    // The dictionary is transparent: values read as if it were a plain array:
    let values: Vec<&str> = array.iter().collect();
    assert_eq!(values, ["a", "b", "a", "a"]);
    assert_eq!(array.to_vec(), ["a", "b", "a", "a"]);

    // Parsing an externally built dictionary array:
    let arrow_array: DictionaryArray<Int64Type> = vec!["x", "y", "x"].into_iter().collect();
    let array =
        TypedArray::<Dictionary<i64, Utf8>>::try_from(Arc::new(arrow_array) as ArrayRef).unwrap();
    assert_eq!(array.value(2), "x");

    // The key type must match:
    let result = TypedArray::<Dictionary<i32, Utf8>>::try_from(array.into_arrow());
    assert!(matches!(result, Err(ColumnError::WrongDataType { .. })));

    // Null keys via the array-level Option:
    let arrow_array: DictionaryArray<Int32Type> = vec![Some("x"), None]
        .into_iter()
        .collect::<DictionaryArray<_>>();
    let arrow_array = Arc::new(arrow_array) as ArrayRef;
    assert!(matches!(
        TypedArray::<Dictionary<i32, Utf8>>::try_from(Arc::clone(&arrow_array)),
        Err(ColumnError::UnexpectedNulls { null_count: 1 })
    ));
    let array = TypedArray::<Option<Dictionary<i32, Utf8>>>::try_from(arrow_array).unwrap();
    let values: Vec<Option<&str>> = array.iter().collect();
    assert_eq!(values, [Some("x"), None]);
}

#[test]
fn dictionary_key_overflow_is_an_error() {
    use quiver::Dictionary;

    // 200 distinct values do not fit in an i8 key:
    let values: Vec<String> = (0..200).map(|i| i.to_string()).collect();
    let result = TypedArray::<Dictionary<i8, Utf8>>::try_from_values(values.clone());
    assert!(matches!(result, Err(ColumnError::Build(_))));

    // …but they fit in an i16 key:
    let array = TypedArray::<Dictionary<i16, Utf8>>::try_from_values(values).unwrap();
    assert_eq!(array.len(), 200);
}

#[test]
fn dictionary_try_into() {
    use quiver::Dictionary;

    let array: TypedArray<Dictionary<i32, Utf8>> = vec!["a", "b", "a"].try_into().unwrap();
    assert_eq!(array.to_vec(), ["a", "b", "a"]);

    // Key overflow propagates as an error:
    let values: Vec<String> = (0..200).map(|i| i.to_string()).collect();
    let result: Result<TypedArray<Dictionary<i8, Utf8>>, _> = values.try_into();
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
    let array = TypedArray::<List<i64>>::try_from(Arc::new(sliced) as ArrayRef).unwrap();
    let values: Vec<Vec<i64>> = array.to_vec();
    assert_eq!(values, [vec![2]]);

    // …but a reachable one is still rejected:
    let list = ListArray::from_iter_primitive::<Int64Type, _, _>(vec![
        Some(vec![None]),
        Some(vec![Some(2)]),
    ]);
    let result = TypedArray::<List<i64>>::try_from(Arc::new(list) as ArrayRef);
    assert!(matches!(
        result,
        Err(ColumnError::UnexpectedNulls { null_count: 1 })
    ));

    // Null items inside the range of a NULL row don't count:
    let list = ListArray::from_iter_primitive::<Int64Type, _, _>(vec![
        None, // null row — arrow's builder gives it an empty range
        Some(vec![Some(2)]),
    ]);
    let array = TypedArray::<Option<List<i64>>>::try_from(Arc::new(list) as ArrayRef).unwrap();
    assert_eq!(array.len(), 2);

    // An unreferenced null entry in a dictionary's value table is fine…
    let values = StringArray::from(vec![Some("a"), None]); // entry 1 is null, unreferenced
    let keys = quiver::arrow::array::Int32Array::from(vec![0, 0]);
    let dictionary = DictionaryArray::new(keys, Arc::new(values));
    let array =
        TypedArray::<Dictionary<i32, Utf8>>::try_from(Arc::new(dictionary) as ArrayRef).unwrap();
    assert_eq!(array.to_vec(), ["a", "a"]);

    // …but a referenced one is still rejected:
    let values = StringArray::from(vec![Some("a"), None]);
    let keys = quiver::arrow::array::Int32Array::from(vec![0, 1]); // references the null
    let dictionary = DictionaryArray::new(keys, Arc::new(values));
    let result = TypedArray::<Dictionary<i32, Utf8>>::try_from(Arc::new(dictionary) as ArrayRef);
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

    let array = TypedArray::<List<i64>>::try_from(Arc::new(list) as ArrayRef).unwrap();
    let lists: Vec<Vec<i64>> = array.to_vec();
    assert_eq!(lists, [vec![1, 2], vec![3]]);
}

#[test]
fn date_and_time_arrays() {
    use quiver::{Date32, Date64, Time32Second, Time64Nanosecond};

    let array = TypedArray::<Date32>::from_values([19_000_i32, 19_001]);
    assert_eq!(TypedArray::<Date32>::data_type(), DataType::Date32);
    assert_eq!(array.to_vec(), [19_000, 19_001]);

    assert_eq!(TypedArray::<Date64>::data_type(), DataType::Date64);

    let array = TypedArray::<Time32Second>::from_values([3600_i32]);
    assert_eq!(
        TypedArray::<Time32Second>::data_type(),
        DataType::Time32(quiver::arrow::datatypes::TimeUnit::Second)
    );
    assert_eq!(array.value(0), 3600);

    let array = TypedArray::<Option<Time64Nanosecond>>::from_values([Some(1_i64), None]);
    assert_eq!(array.to_vec(), [Some(1), None]);
}

#[test]
fn large_utf8_array() {
    use quiver::LargeUtf8;

    let array = TypedArray::<LargeUtf8>::from_values(["a", "b"]);
    assert_eq!(TypedArray::<LargeUtf8>::data_type(), DataType::LargeUtf8);
    let values: Vec<&str> = array.iter().collect();
    assert_eq!(values, ["a", "b"]);
    assert_eq!(array.to_vec(), ["a".to_owned(), "b".to_owned()]);
}

#[test]
fn fixed_size_list_arrays() {
    use quiver::FixedSizeList;

    // 3D positions:
    let array =
        TypedArray::<FixedSizeList<f32, 3>>::from_values([[1.0_f32, 2.0, 3.0], [4.0, 5.0, 6.0]]);
    assert_eq!(
        TypedArray::<FixedSizeList<f32, 3>>::data_type(),
        DataType::FixedSizeList(Arc::new(Field::new("item", DataType::Float32, false)), 3)
    );
    let positions: Vec<[f32; 3]> = array.to_vec();
    assert_eq!(positions, [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]);

    // Iteration is zero-copy, like List:
    let first: Vec<f32> = array.value(0).collect();
    assert_eq!(first, [1.0, 2.0, 3.0]);

    // The size is part of the type:
    let result = TypedArray::<FixedSizeList<f32, 4>>::try_from(Arc::clone(array.as_arrow()));
    assert!(matches!(result, Err(ColumnError::WrongDataType { .. })));

    // Nullable rows: the null row's placeholder items are masked, not errors:
    let array = TypedArray::<Option<FixedSizeList<f32, 3>>>::from_nullable_values([
        Some([1.0_f32, 2.0, 3.0]),
        None,
    ]);
    assert_eq!(array.to_vec(), [Some([1.0, 2.0, 3.0]), None]);

    // Roundtrip through arrow:
    let roundtripped =
        TypedArray::<Option<FixedSizeList<f32, 3>>>::try_from(array.into_arrow()).unwrap();
    assert_eq!(roundtripped.to_vec(), [Some([1.0, 2.0, 3.0]), None]);

    // Slicing:
    let array = TypedArray::<FixedSizeList<i64, 2>>::from_values([[1_i64, 2], [3, 4], [5, 6]]);
    let sliced = array.slice(1, 2);
    assert_eq!(sliced.to_vec(), [[3, 4], [5, 6]]);
}

#[test]
fn large_list_arrays() {
    use quiver::LargeList;
    use quiver::arrow::array::LargeListArray;

    let array = TypedArray::<LargeList<i64>>::from_values([vec![1_i64, 2], vec![3]]);
    assert_eq!(
        TypedArray::<LargeList<i64>>::data_type(),
        DataType::LargeList(Arc::new(Field::new("item", DataType::Int64, false)))
    );
    let lists: Vec<Vec<i64>> = array.to_vec();
    assert_eq!(lists, [vec![1, 2], vec![3]]);

    // Iteration is zero-copy, like List:
    let first: Vec<i64> = array.value(0).collect();
    assert_eq!(first, [1, 2]);

    // List ≠ LargeList: the offset width is part of the type:
    let result = TypedArray::<List<i64>>::try_from(Arc::clone(array.as_arrow()));
    assert!(matches!(result, Err(ColumnError::WrongDataType { .. })));

    // Nullable items:
    let array = TypedArray::<LargeList<Option<i64>>>::from_values([vec![Some(1), None]]);
    let lists: Vec<Vec<Option<i64>>> = array.iter().map(Iterator::collect).collect();
    assert_eq!(lists, [vec![Some(1), None]]);

    // A reachable null item at a non-nullable level is rejected:
    let arrow_array =
        LargeListArray::from_iter_primitive::<Int64Type, _, _>(vec![Some(vec![None])]);
    let result = TypedArray::<LargeList<i64>>::try_from(Arc::new(arrow_array) as ArrayRef);
    assert!(matches!(
        result,
        Err(ColumnError::UnexpectedNulls { null_count: 1 })
    ));

    // Nullable rows:
    let arrow_array =
        LargeListArray::from_iter_primitive::<Int64Type, _, _>(vec![Some(vec![Some(1)]), None]);
    let array =
        TypedArray::<Option<LargeList<i64>>>::try_from(Arc::new(arrow_array) as ArrayRef).unwrap();
    let lists: Vec<Option<Vec<i64>>> = array.iter().map(|row| row.map(Iterator::collect)).collect();
    assert_eq!(lists, [Some(vec![1]), None]);

    // Nested in a List:
    let array = TypedArray::<LargeList<List<Utf8>>>::from_values([vec![vec!["a".to_owned()]]]);
    assert_eq!(array.len(), 1);
}

#[test]
fn list_view_arrays() {
    use quiver::arrow::array::{Int64Array, ListViewArray};
    use quiver::arrow::buffer::ScalarBuffer;
    use quiver::{LargeListView, ListView};

    let array = TypedArray::<ListView<i64>>::from_values([vec![1_i64, 2], vec![3]]);
    assert_eq!(
        TypedArray::<ListView<i64>>::data_type(),
        DataType::ListView(Arc::new(Field::new("item", DataType::Int64, false)))
    );
    let lists: Vec<Vec<i64>> = array.to_vec();
    assert_eq!(lists, [vec![1, 2], vec![3]]);
    let first: Vec<i64> = array.value(0).collect();
    assert_eq!(first, [1, 2]);

    // List ≠ ListView: the layout is part of the type:
    let result = TypedArray::<List<i64>>::try_from(Arc::clone(array.as_arrow()));
    assert!(matches!(result, Err(ColumnError::WrongDataType { .. })));

    // The distinguishing feature of list-views: ranges may overlap and appear
    // out of order. Parse such an externally built array:
    let values = Arc::new(Int64Array::from(vec![10, 20, 30]));
    let field = Arc::new(Field::new("item", DataType::Int64, false));
    let arrow_array = ListViewArray::new(
        field,
        ScalarBuffer::from(vec![1_i32, 0]), // row 1 starts *before* row 0
        ScalarBuffer::from(vec![2_i32, 2]), // both length 2, overlapping
        values,
        None,
    );
    let array = TypedArray::<ListView<i64>>::try_from(Arc::new(arrow_array) as ArrayRef).unwrap();
    let lists: Vec<Vec<i64>> = array.to_vec();
    assert_eq!(lists, [vec![20, 30], vec![10, 20]]);

    // A reachable null item at a non-nullable level is rejected:
    let values = Arc::new(Int64Array::from(vec![Some(1), None]));
    let field = Arc::new(Field::new("item", DataType::Int64, true));
    let arrow_array = ListViewArray::new(
        field,
        ScalarBuffer::from(vec![0_i32]),
        ScalarBuffer::from(vec![2_i32]),
        values,
        None,
    );
    let result = TypedArray::<ListView<i64>>::try_from(Arc::new(arrow_array) as ArrayRef);
    assert!(matches!(
        result,
        Err(ColumnError::UnexpectedNulls { null_count: 1 })
    ));

    // Nullable items and nullable rows:
    let array = TypedArray::<ListView<Option<i64>>>::from_values([vec![Some(1), None]]);
    let lists: Vec<Vec<Option<i64>>> = array.iter().map(Iterator::collect).collect();
    assert_eq!(lists, [vec![Some(1), None]]);

    let array =
        TypedArray::<Option<LargeListView<i64>>>::from_nullable_values([Some(vec![1_i64]), None]);
    let lists: Vec<Option<Vec<i64>>> = array.iter().map(|row| row.map(Iterator::collect)).collect();
    assert_eq!(lists, [Some(vec![1]), None]);

    // LargeListView round-trips too:
    let array = TypedArray::<LargeListView<i64>>::from_values([vec![1_i64, 2], vec![3]]);
    assert_eq!(
        TypedArray::<LargeListView<i64>>::data_type(),
        DataType::LargeListView(Arc::new(Field::new("item", DataType::Int64, false)))
    );
    assert_eq!(array.to_vec(), [vec![1, 2], vec![3]]);
}

#[test]
fn any_list_arrays() {
    use quiver::{AnyList, FixedSizeList, LargeList, LargeListView, ListView};

    // `AnyList` is parse-only (no single data type to build): `try_from` accepts
    // every variable-length encoding, read uniformly:
    let encodings = [
        TypedArray::<List<i64>>::from_values([vec![1_i64, 2], vec![3]]).into_arrow(),
        TypedArray::<LargeList<i64>>::from_values([vec![1_i64, 2], vec![3]]).into_arrow(),
        TypedArray::<ListView<i64>>::from_values([vec![1_i64, 2], vec![3]]).into_arrow(),
        TypedArray::<LargeListView<i64>>::from_values([vec![1_i64, 2], vec![3]]).into_arrow(),
    ];
    for arrow_array in encodings {
        let array = TypedArray::<AnyList<i64>>::try_from(arrow_array).unwrap();
        assert_eq!(array.to_vec(), [vec![1, 2], vec![3]]);
    }

    // …including `FixedSizeList` (fixed cardinality, read at runtime):
    let fixed = TypedArray::<FixedSizeList<i64, 2>>::from_values([[1_i64, 2], [3, 4]]).into_arrow();
    let array = TypedArray::<AnyList<i64>>::try_from(fixed).unwrap();
    assert_eq!(array.to_vec(), [vec![1, 2], vec![3, 4]]);

    // A non-list array is rejected:
    let ints = TypedArray::<i64>::from_values([1, 2]).into_arrow();
    assert!(matches!(
        TypedArray::<AnyList<i64>>::try_from(ints),
        Err(ColumnError::WrongDataType { .. })
    ));

    // Item nullability is enforced regardless of encoding:
    let nullable =
        TypedArray::<ListView<Option<i64>>>::from_values([vec![Some(1), None]]).into_arrow();
    assert!(matches!(
        TypedArray::<AnyList<i64>>::try_from(Arc::clone(&nullable)),
        Err(ColumnError::UnexpectedNulls { null_count: 1 })
    ));
    let array = TypedArray::<AnyList<Option<i64>>>::try_from(nullable).unwrap();
    let items: Vec<Option<i64>> = array.value(0).collect();
    assert_eq!(items, [Some(1), None]);

    // Null rows via the array-level `Option`:
    let arrow_array =
        TypedArray::<Option<List<i64>>>::from_nullable_values([Some(vec![1_i64]), None])
            .into_arrow();
    let array = TypedArray::<Option<AnyList<i64>>>::try_from(arrow_array).unwrap();
    let rows: Vec<Option<Vec<i64>>> = array.iter().map(|row| row.map(Iterator::collect)).collect();
    assert_eq!(rows, [Some(vec![1]), None]);
}

#[test]
fn map_arrays() {
    use quiver::Map;
    use quiver::arrow::array::{Int64Builder, MapBuilder, StringBuilder};

    // Build from owned (key, value) pairs:
    let array = TypedArray::<Map<Utf8, i64>>::from_values([
        vec![("a".to_owned(), 1_i64), ("b".to_owned(), 2)],
        vec![],
        vec![("c".to_owned(), 3)],
    ]);
    assert_eq!(
        TypedArray::<Map<Utf8, i64>>::data_type(),
        DataType::Map(
            Arc::new(Field::new(
                "entries",
                DataType::Struct(
                    vec![
                        Field::new("keys", DataType::Utf8, false),
                        Field::new("values", DataType::Int64, false),
                    ]
                    .into()
                ),
                false,
            )),
            false,
        )
    );

    // Each row reads back as its (key, value) pairs:
    let rows: Vec<Vec<(String, i64)>> = array.to_vec();
    assert_eq!(
        rows,
        [
            vec![("a".to_owned(), 1), ("b".to_owned(), 2)],
            vec![],
            vec![("c".to_owned(), 3)],
        ]
    );

    // Zero-copy iteration over one row's pairs:
    let first: Vec<(&str, i64)> = array.value(0).collect();
    assert_eq!(first, [("a", 1), ("b", 2)]);

    // Parsing an externally built (arrow `MapBuilder`) map array:
    let mut builder = MapBuilder::new(None, StringBuilder::new(), Int64Builder::new());
    builder.keys().append_value("x");
    builder.values().append_value(10);
    builder.append(true).unwrap();
    builder.append(true).unwrap(); // empty map
    let arrow_array = builder.finish();
    let array = TypedArray::<Map<Utf8, i64>>::try_from(Arc::new(arrow_array) as ArrayRef).unwrap();
    assert_eq!(array.value_owned(0), [("x".to_owned(), 10)]);
    assert_eq!(array.value_owned(1), []);

    // Nullable values:
    let array = TypedArray::<Map<Utf8, Option<i64>>>::from_values([vec![
        ("a".to_owned(), Some(1_i64)),
        ("b".to_owned(), None),
    ]]);
    let rows: Vec<Vec<(String, Option<i64>)>> = array.to_vec();
    assert_eq!(
        rows,
        [vec![("a".to_owned(), Some(1)), ("b".to_owned(), None)]]
    );

    // A null value at a non-nullable level is rejected:
    let mut builder = MapBuilder::new(None, StringBuilder::new(), Int64Builder::new());
    builder.keys().append_value("a");
    builder.values().append_null();
    builder.append(true).unwrap();
    let arrow_array = builder.finish();
    let result = TypedArray::<Map<Utf8, i64>>::try_from(Arc::new(arrow_array) as ArrayRef);
    assert!(matches!(
        result,
        Err(ColumnError::UnexpectedNulls { null_count: 1 })
    ));

    // Whole-row (map) nullability:
    let array = TypedArray::<Option<Map<Utf8, i64>>>::from_nullable_values([
        Some(vec![("a".to_owned(), 1_i64)]),
        None,
    ]);
    let rows: Vec<Option<Vec<(&str, i64)>>> =
        array.iter().map(|row| row.map(Iterator::collect)).collect();
    assert_eq!(rows, [Some(vec![("a", 1)]), None]);
}

#[test]
fn run_arrays() {
    use quiver::Run;
    use quiver::arrow::array::{Int32Array, RunArray, StringArray};
    use quiver::arrow::datatypes::Int32Type;

    // Building run-end-encodes the values (consecutive duplicates collapse):
    let array = TypedArray::<Run<i32, Utf8>>::try_from_values(["a", "a", "a", "b", "b"]).unwrap();
    assert_eq!(
        TypedArray::<Run<i32, Utf8>>::data_type(),
        DataType::RunEndEncoded(
            Arc::new(Field::new("run_ends", DataType::Int32, false)),
            Arc::new(Field::new("values", DataType::Utf8, false)),
        )
    );

    // The encoding is transparent: values read as if it were a plain array:
    let values: Vec<&str> = array.iter().collect();
    assert_eq!(values, ["a", "a", "a", "b", "b"]);
    assert_eq!(array.value(3), "b");
    assert_eq!(&array[0], "a"); // `RefType`, looked up through the run ends

    // Parsing an externally built run array:
    let run_ends = Int32Array::from(vec![2, 5, 6]); // runs end at logical 2, 5, 6
    let run_values = StringArray::from(vec!["x", "y", "z"]);
    let arrow_array = RunArray::<Int32Type>::try_new(&run_ends, &run_values).unwrap();
    let array = TypedArray::<Run<i32, Utf8>>::try_from(Arc::new(arrow_array) as ArrayRef).unwrap();
    assert_eq!(array.to_vec(), ["x", "x", "y", "y", "y", "z"]);

    // The run-end index type is part of the type:
    let result = TypedArray::<Run<i64, Utf8>>::try_from(Arc::clone(array.as_arrow()));
    assert!(matches!(result, Err(ColumnError::WrongDataType { .. })));

    // Nulls live in the values, so nullability is `Run<R, Option<V>>`:
    let run_ends = Int32Array::from(vec![1, 2]);
    let run_values = StringArray::from(vec![Some("x"), None]);
    let arrow_array = RunArray::<Int32Type>::try_new(&run_ends, &run_values).unwrap();
    let arrow_array = Arc::new(arrow_array) as ArrayRef;

    // …a null at a non-nullable level is rejected:
    assert!(matches!(
        TypedArray::<Run<i32, Utf8>>::try_from(Arc::clone(&arrow_array)),
        Err(ColumnError::UnexpectedNulls { null_count: 1 })
    ));

    // …but `Run<i32, Option<Utf8>>` accepts it:
    let array = TypedArray::<Run<i32, Option<Utf8>>>::try_from(arrow_array).unwrap();
    let values: Vec<Option<&str>> = array.iter().collect();
    assert_eq!(values, [Some("x"), None]);

    // Run-end overflow propagates as an error (more rows than `i16` can index):
    let many: Vec<String> = (0..40_000).map(|i| i.to_string()).collect();
    let result = TypedArray::<Run<i16, Utf8>>::try_from_values(many.clone());
    assert!(matches!(result, Err(ColumnError::Build(_))));

    // …but `i32` indices fit:
    let array = TypedArray::<Run<i32, Utf8>>::try_from_values(many).unwrap();
    assert_eq!(array.len(), 40_000);
}

/// Domain newtypes via `newtype_data_type!`.
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

quiver::newtype_data_type!(SensorName, Utf8);

/// A `[u8; 16]`-backed newtype.
///
/// `Pod` (via the re-exported `bytemuck`) is what lets the `primitive` arm hand
/// out `&[ChunkId]` from `as_slice`, rather than the raw `&[[u8; 16]]`.
#[derive(Debug, PartialEq, Clone, Copy, quiver::bytemuck::Pod, quiver::bytemuck::Zeroable)]
#[bytemuck(crate = "::quiver::bytemuck")]
#[repr(transparent)]
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

quiver::newtype_data_type!(ChunkId, FixedSizeBinary<16>, primitive);

/// A `[u8; 16]`-backed newtype that is *not* `Pod`, so the buffer cannot be
/// reinterpreted; the hand-written `PrimitiveType` impl reads back the repr.
#[derive(Debug, PartialEq, Clone, Copy)]
struct RawId([u8; 16]);

impl From<[u8; 16]> for RawId {
    fn from(id: [u8; 16]) -> Self {
        Self(id)
    }
}
impl From<RawId> for [u8; 16] {
    fn from(id: RawId) -> Self {
        id.0
    }
}

quiver::newtype_data_type!(RawId, FixedSizeBinary<16>);

impl quiver::PrimitiveType for RawId {
    type Native = [u8; 16];

    fn values(typed: &Self::Typed) -> &[[u8; 16]] {
        <FixedSizeBinary<16> as quiver::PrimitiveType>::values(typed)
    }
}

/// A `bool`-backed newtype: `bool` has no `RefType` (bit-packed),
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

quiver::newtype_data_type!(IsActive, bool, noref);

#[test]
fn newtype_arrays() {
    let array = TypedArray::<SensorName>::from_values([
        SensorName("kitchen".to_owned()),
        SensorName("attic".to_owned()),
    ]);
    assert_eq!(TypedArray::<SensorName>::data_type(), DataType::Utf8);

    // Reading is zero-copy, yielding the repr's borrowed value:
    let values: Vec<&str> = array.iter().collect();
    assert_eq!(values, ["kitchen", "attic"]);

    // Indexing borrows through the repr:
    assert_eq!(&array[1], "attic");

    // Owned values are the newtype:
    assert_eq!(
        array.to_vec(),
        [
            SensorName("kitchen".to_owned()),
            SensorName("attic".to_owned())
        ]
    );

    // Composes like any logical type:
    let array = TypedArray::<Option<ChunkId>>::from_nullable_values([Some(ChunkId([7; 16])), None]);
    assert_eq!(array.to_vec(), [Some(ChunkId([7; 16])), None]);
    assert_eq!(
        TypedArray::<ChunkId>::data_type(),
        DataType::FixedSizeBinary(16)
    );

    // The `primitive` arm enables bulk zero-copy reads, yielding the newtype:
    let array = TypedArray::<ChunkId>::from_values([ChunkId([7; 16]), ChunkId([8; 16])]);
    assert_eq!(array.as_slice(), &[ChunkId([7; 16]), ChunkId([8; 16])]);

    // …while a hand-written `PrimitiveType` impl can still yield the repr's:
    let array = TypedArray::<RawId>::from_values([RawId([7; 16]), RawId([8; 16])]);
    assert_eq!(array.as_slice(), &[[7_u8; 16], [8; 16]]);

    // Slicing still lines up with the logical window:
    assert_eq!(
        TypedArray::<ChunkId>::from_values([ChunkId([7; 16]), ChunkId([8; 16])])
            .slice(1, 1)
            .as_slice(),
        &[ChunkId([8; 16])]
    );

    let array = TypedArray::<List<SensorName>>::from_values([vec![SensorName("a".to_owned())]]);
    assert_eq!(array.to_vec(), [vec![SensorName("a".to_owned())]]);

    // `noref` newtypes still read normally (just no `array[index]`):
    let array = TypedArray::<IsActive>::from_values([IsActive(true), IsActive(false)]);
    assert!(array.value(0));
    assert_eq!(array.to_vec(), [IsActive(true), IsActive(false)]);
}

/// A fallible domain newtype via `try_newtype_data_type!`:
/// only even numbers are valid.
#[derive(Debug, PartialEq, Clone, Copy)]
struct Even(i64);

#[derive(Debug, PartialEq)]
struct NotEven(i64);

impl std::fmt::Display for NotEven {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} is not even", self.0)
    }
}
impl std::error::Error for NotEven {}

impl TryFrom<i64> for Even {
    type Error = NotEven;

    fn try_from(value: i64) -> Result<Self, NotEven> {
        if value % 2 == 0 {
            Ok(Self(value))
        } else {
            Err(NotEven(value))
        }
    }
}
impl From<Even> for i64 {
    fn from(even: Even) -> Self {
        even.0
    }
}

quiver::try_newtype_data_type!(Even, i64);

impl quiver::PrimitiveType for Even {
    type Native = i64;

    fn values(typed: &Self::Typed) -> &[i64] {
        <i64 as quiver::PrimitiveType>::values(typed)
    }
}

#[test]
fn fallible_newtype_arrays() {
    // Building goes through the infallible `From<Even> for i64`:
    let arrow_array = TypedArray::<Even>::from_values([Even(2), Even(4)]);
    assert_eq!(TypedArray::<Even>::data_type(), DataType::Int64);
    assert_eq!(arrow_array.to_vec(), [Even(2), Even(4)]);

    // Reading yields the repr's borrowed value; owned values are the newtype:
    assert_eq!(arrow_array.value(0), 2_i64);
    assert_eq!(arrow_array[1], 4_i64); // indexing borrows through the repr
    assert_eq!(arrow_array.value_owned(1), Even(4));

    // The hand-written `PrimitiveType` impl still gives bulk zero-copy reads:
    assert_eq!(arrow_array.as_slice(), &[2_i64, 4]);

    // A valid array converts fine:
    let arrow_array = Arc::new(Int64Array::from(vec![2, 4, 6]));
    assert!(TypedArray::<Even>::try_new(arrow_array).is_ok());

    // …but one bad value is rejected eagerly, at construction:
    let arrow_array = Arc::new(Int64Array::from(vec![2, 3, 4]));
    let err = TypedArray::<Even>::try_new(arrow_array).unwrap_err();
    assert!(matches!(err, quiver::ColumnError::Conversion(_)));
    assert_eq!(err.to_string(), "Failed to convert value: 3 is not even");

    // Nulls at an `Option` level are skipped by the validation:
    let arrow_array = TypedArray::<Option<Even>>::from_nullable_values([Some(Even(2)), None]);
    assert_eq!(arrow_array.to_vec(), [Some(Even(2)), None]);

    // The conversion error is boxed into `ErrorKind::Conversion` once the
    // array name is known:
    let arrow_array = Arc::new(Int64Array::from(vec![1]));
    let batch = RecordBatch::try_from_iter([("level", arrow_array as ArrayRef)]).unwrap();
    let err = Column::<Even>::from_record_batch_and_name(&batch, "level").unwrap_err();
    assert!(matches!(*err.kind, quiver::ErrorKind::Conversion { .. }));

    // …and the cause stays reachable, not just printable:
    let kind = std::error::Error::source(&err).expect("the kind is the source");
    let cause = kind
        .source()
        .expect("the conversion error is the kind's source");
    assert_eq!(cause.downcast_ref::<NotEven>(), Some(&NotEven(1)));
}

#[test]
fn nonzero_and_char_arrays() {
    use std::num::{NonZeroI64, NonZeroU32};

    // `NonZero*` are wired up out of the box, stored as their plain integer:
    let array = TypedArray::<NonZeroI64>::from_values([
        NonZeroI64::new(1).unwrap(),
        NonZeroI64::new(-3).unwrap(),
    ]);
    assert_eq!(TypedArray::<NonZeroI64>::data_type(), DataType::Int64);
    assert_eq!(array.value(0), 1_i64); // reads the repr
    assert_eq!(array.as_slice(), &[1_i64, -3]); // bulk zero-copy, as the repr
    assert_eq!(
        array.to_vec(),
        [NonZeroI64::new(1).unwrap(), NonZeroI64::new(-3).unwrap()]
    );

    // A zero is rejected at construction:
    let arrow_array = Arc::new(Int64Array::from(vec![1, 0, 2]));
    let err = TypedArray::<NonZeroI64>::try_new(arrow_array).unwrap_err();
    assert!(matches!(err, quiver::ColumnError::Conversion(_)));

    // `char` is stored as `UInt32`:
    let array = TypedArray::<char>::from_values(['q', '🦀']);
    assert_eq!(TypedArray::<char>::data_type(), DataType::UInt32);
    assert_eq!(array.to_vec(), ['q', '🦀']);
    assert_eq!(array.value(0), u32::from('q'));

    // A surrogate / out-of-range code point is rejected at construction:
    let arrow_array = Arc::new(quiver::arrow::array::UInt32Array::from(vec![
        u32::from('a'),
        0xD800, // a UTF-16 surrogate: not a valid `char`
    ]));
    assert!(TypedArray::<char>::try_new(arrow_array).is_err());

    // Composes and nullable-wraps like any other logical type:
    let array = TypedArray::<Option<NonZeroU32>>::from_nullable_values([NonZeroU32::new(5), None]);
    assert_eq!(array.to_vec(), [NonZeroU32::new(5), None]);
}

#[test]
fn as_adapter_for_foreign_types() {
    use std::net::Ipv4Addr;

    use quiver::As;

    // `Ipv4Addr` is a foreign type: no `newtype_data_type!` possible (orphan rule).
    let array = TypedArray::<As<Ipv4Addr, u32>>::from_values([
        Ipv4Addr::LOCALHOST,
        Ipv4Addr::new(192, 168, 0, 1),
    ]);
    assert_eq!(
        TypedArray::<As<Ipv4Addr, u32>>::data_type(),
        DataType::UInt32
    );

    // Reading is zero-copy, yielding the repr's value:
    assert_eq!(array.value(0), u32::from(Ipv4Addr::LOCALHOST));

    // Owned values are the foreign type:
    assert_eq!(
        array.to_vec(),
        [Ipv4Addr::LOCALHOST, Ipv4Addr::new(192, 168, 0, 1)]
    );

    // Composes like any logical type:
    let array = TypedArray::<Option<As<Ipv4Addr, u32>>>::from_nullable_values([
        Some(Ipv4Addr::LOCALHOST),
        None,
    ]);
    assert_eq!(array.to_vec(), [Some(Ipv4Addr::LOCALHOST), None]);

    let array = TypedArray::<List<As<Ipv4Addr, u32>>>::from_values([vec![Ipv4Addr::LOCALHOST]]);
    assert_eq!(array.to_vec(), [vec![Ipv4Addr::LOCALHOST]]);
}

/// A custom logical type whose `downcast` accepts *several* data types:
/// both `Int32` and `Int64` arrays, reading every value as `i64`.
struct AnyInt;

impl quiver::LogicalType for AnyInt {
    type Typed = ArrayRef;
    type Value<'a> = i64;
    type Owned = i64;
    type Optional = Option<Self>;
    type Required = Self;

    fn downcast(
        arrow_array: &dyn quiver::arrow::array::Array,
    ) -> Result<Self::Typed, quiver::ColumnError> {
        // `downcast` is the validator: accept both integer widths, reject the rest.
        if !matches!(arrow_array.data_type(), DataType::Int32 | DataType::Int64) {
            return Err(quiver::ColumnError::WrongDataType {
                expected: "Int32 or Int64".to_owned(),
                actual: arrow_array.data_type().clone(),
            });
        }
        Ok(quiver::arrow::array::make_array(arrow_array.to_data()))
    }

    fn is_null(typed: &Self::Typed, index: usize) -> bool {
        typed.is_null(index)
    }

    fn value(typed: &Self::Typed, index: usize) -> i64 {
        use quiver::arrow::array::AsArray as _;
        match typed.data_type() {
            DataType::Int32 => i64::from(typed.as_primitive::<Int32Type>().value(index)),
            DataType::Int64 => typed.as_primitive::<Int64Type>().value(index),
            _ => unreachable!("`downcast` only accepts Int32 and Int64"),
        }
    }

    fn to_owned_value(value: i64) -> i64 {
        value
    }
}

impl quiver::ConcreteType for AnyInt {
    /// The canonical data type: used when encoding, and in error messages.
    fn data_type() -> DataType {
        DataType::Int64
    }

    fn build(values: impl Iterator<Item = Option<i64>>) -> Result<ArrayRef, quiver::ColumnError> {
        Ok(Arc::new(values.collect::<Int64Array>()))
    }
}

#[test]
fn custom_multi_data_type() {
    use quiver::arrow::array::Int32Array;

    // The custom `downcast` accepts both integer widths:
    let from_i32 = TypedArray::<AnyInt>::try_new(Arc::new(Int32Array::from(vec![1, 2]))).unwrap();
    let from_i64 = TypedArray::<AnyInt>::try_new(Arc::new(Int64Array::from(vec![3]))).unwrap();
    assert_eq!(from_i32.to_vec(), [1, 2]);
    assert_eq!(from_i64.to_vec(), [3]);

    // …but nothing else:
    let err = TypedArray::<AnyInt>::try_new(Arc::new(StringArray::from(vec!["nope"]))).unwrap_err();
    assert!(matches!(err, ColumnError::WrongDataType { .. }));

    // Containers forward to the inner `matches`, at any nesting depth:
    let int32_items =
        ListArray::from_iter_primitive::<Int32Type, _, _>(vec![Some(vec![Some(1), Some(2)])]);
    let lists = TypedArray::<List<Option<AnyInt>>>::try_new(Arc::new(int32_items)).unwrap();
    let items: Vec<Option<i64>> = lists.value(0).collect();
    assert_eq!(items, [Some(1), Some(2)]);

    // `Option<…>` forwards too:
    let nullable =
        TypedArray::<Option<AnyInt>>::try_new(Arc::new(Int32Array::from(vec![Some(7), None])))
            .unwrap();
    assert_eq!(nullable.to_vec(), [Some(7), None]);
}

#[test]
fn utf8_string_encodings() {
    use quiver::{LargeUtf8, Utf8View};

    // All three string encodings build from and yield the same values:
    let plain = TypedArray::<Utf8>::from_values(["a", "b"]);
    let large = TypedArray::<LargeUtf8>::from_values(["a", "b"]);
    let view = TypedArray::<Utf8View>::from_values(["a", "b"]);

    assert_eq!(TypedArray::<Utf8>::data_type(), DataType::Utf8);
    assert_eq!(TypedArray::<LargeUtf8>::data_type(), DataType::LargeUtf8);
    assert_eq!(TypedArray::<Utf8View>::data_type(), DataType::Utf8View);

    for array in [&plain.to_vec(), &large.to_vec(), &view.to_vec()] {
        assert_eq!(array, &["a".to_owned(), "b".to_owned()]);
    }

    // Zero-copy reads and indexing work for all of them:
    assert_eq!(view.value(1), "b");
    assert_eq!(&view[0], "a");

    // Nullable views too:
    let nullable = TypedArray::<Option<Utf8View>>::from_nullable_values([Some("a"), None]);
    let values: Vec<Option<&str>> = nullable.iter().collect();
    assert_eq!(values, [Some("a"), None]);
}

#[test]
fn list_value_array_like_api() {
    // A `ListValue` (one list element) mirrors `TypedArray`'s read API.
    let array = TypedArray::<List<i64>>::from_values([vec![10, 20, 30], vec![]]);

    let first = array.value(0);
    assert_eq!(first.len(), 3);
    assert!(!first.is_empty());

    // Random access by item index:
    assert_eq!(first.value(0), 10);
    assert_eq!(first.value(2), 30);
    assert_eq!(first.get(1), Some(20));
    assert_eq!(first.get(3), None);

    // `list[i]` borrows from the array (primitive items):
    assert_eq!(first[1], 20);

    // Bulk zero-copy slice, and owned copies:
    assert_eq!(first.as_slice(), &[10, 20, 30]);
    assert_eq!(first.to_vec(), vec![10, 20, 30]);

    // `iter` does not consume the view; the struct is `Copy`:
    let sum: i64 = first.iter().sum();
    assert_eq!(sum, 60);
    let sum_again: i64 = first.iter().sum();
    assert_eq!(sum_again, 60);

    // Iterating still works directly (it is an `Iterator`):
    let collected: Vec<i64> = first.collect();
    assert_eq!(collected, [10, 20, 30]);

    // Overridden combinators behave like the defaults:
    assert_eq!(first.iter().count(), 3);
    assert_eq!(first.iter().last(), Some(30));
    assert_eq!(first.iter().nth(1), Some(20));
    assert_eq!(first.iter().nth(3), None);
    assert_eq!(
        first
            .iter()
            .fold(String::new(), |acc, x| format!("{acc}{x}")),
        "102030"
    );

    // Double-ended: `rev`, `next_back`, `nth_back`, `rfold`:
    let rev: Vec<i64> = first.iter().rev().collect();
    assert_eq!(rev, [30, 20, 10]);
    let mut cursor = first.iter();
    assert_eq!(cursor.next_back(), Some(30));
    assert_eq!(cursor.next(), Some(10));
    assert_eq!(cursor.next_back(), Some(20));
    assert_eq!(cursor.next_back(), None);
    assert_eq!(first.iter().nth_back(1), Some(20));
    assert_eq!(first.iter().nth_back(3), None);
    assert_eq!(
        first
            .iter()
            .rfold(String::new(), |acc, x| format!("{acc}{x}")),
        "302010"
    );

    // Empty element:
    let second = array.value(1);
    assert!(second.is_empty());
    assert_eq!(second.len(), 0);
    assert_eq!(second.get(0), None);
    assert_eq!(second.as_slice(), &[] as &[i64]);

    // String items: owned access and indexing.
    let strings = TypedArray::<List<Utf8>>::from_values([vec!["a".to_owned(), "b".to_owned()]]);
    let row = strings.value(0);
    assert_eq!(&row[0], "a");
    assert_eq!(row.value_owned(1), "b".to_owned());
    assert_eq!(row.get_owned(0), Some("a".to_owned()));
    assert_eq!(row.to_vec(), vec!["a".to_owned(), "b".to_owned()]);
}

#[test]
#[should_panic(expected = "out of bounds for length 2")]
fn list_value_index_out_of_bounds() {
    let array = TypedArray::<List<i64>>::from_values([vec![1, 2]]);
    let value: i64 = array.value(0).value(2);
    assert_eq!(value, 0); // unreachable: the line above panics
}
