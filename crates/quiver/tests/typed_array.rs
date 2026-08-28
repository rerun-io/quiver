//! Tests for [`quiver::TypedArray`]: a [`quiver::Column`] without the metadata.

use std::sync::Arc;

use quiver::arrow::array::{ArrayRef, Int64Array, StringArray};
use quiver::{Column, ColumnError, List, TypedArray, Utf8};

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
