//! Tests for standalone use of [`arrow_quiver::Column`] — no derive macro involved.

use std::sync::Arc;

use arrow_quiver::arrow::array::{
    ArrayRef, FixedSizeBinaryArray, Int64Array, ListArray, StringArray,
};
use arrow_quiver::arrow::datatypes::{DataType, Field, Int64Type};
use arrow_quiver::{Column, ColumnError, List};

#[test]
fn standalone_flat_column() {
    let dynamic_array: ArrayRef = Arc::new(StringArray::from(vec!["foo", "bar"]));

    let column = Column::<String>::try_from(dynamic_array).unwrap();
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

    // `from_iter_primitive` produces a nullable item field, so `List<i64>` is rejected…
    let result = Column::<List<i64>>::try_from(Arc::clone(&dynamic_array));
    assert!(matches!(result, Err(ColumnError::WrongDatatype { .. })));

    // …but `List<Option<i64>>` matches:
    let column = Column::<List<Option<i64>>>::try_from(dynamic_array).unwrap();
    let lists: Vec<Vec<Option<i64>>> = column.iter().map(Iterator::collect).collect();
    assert_eq!(lists, [vec![Some(1), Some(2)], vec![Some(3)]]);
}

#[test]
fn standalone_wrong_datatype() {
    let dynamic_array: ArrayRef = Arc::new(Int64Array::from(vec![1]));

    let result = Column::<String>::try_from(dynamic_array);
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
        arrow_quiver::arrow::buffer::OffsetBuffer::new(vec![0, 1, 3].into()),
        Arc::new(strings),
        None,
    );
    let outer_field = Arc::new(Field::new("item", DataType::List(inner_field), false));
    let outer = ListArray::new(
        outer_field,
        arrow_quiver::arrow::buffer::OffsetBuffer::new(vec![0, 2].into()),
        Arc::new(inner),
        None,
    );

    let column = Column::<List<List<String>>>::try_from(Arc::new(outer) as ArrayRef).unwrap();
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
