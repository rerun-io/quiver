//! Support for domain newtypes: making `Column<MyType>` work for
//! `struct MyType(String)` and friends, via [`newtype_datatype!`](crate::newtype_datatype).

/// Implements [`Datatype`](crate::Datatype) for a domain newtype,
/// making `Column<MyType>` work — including nesting (`List<MyType>`),
/// the convenience constructors, and the derive.
///
/// The newtype must convert to and from its representation:
/// `impl From<MyType> for Repr` and `impl From<Repr> for MyType`.
///
/// Reading stays zero-copy and yields the *representation's* borrowed value
/// (e.g. `&str` for a `String`-backed newtype);
/// owned values ([`Column::to_vec`](crate::Column::to_vec) etc.) are the newtype.
///
/// ```
/// #[derive(Debug, PartialEq)]
/// struct SensorName(String);
///
/// impl From<String> for SensorName {
///     fn from(name: String) -> Self {
///         Self(name)
///     }
/// }
/// impl From<SensorName> for String {
///     fn from(name: SensorName) -> Self {
///         name.0
///     }
/// }
///
/// arrow_quiver::newtype_datatype!(SensorName, String);
///
/// let column = arrow_quiver::Column::<SensorName>::from_values([
///     SensorName("kitchen".to_owned()),
/// ]);
/// assert_eq!(column.value(0), "kitchen"); // borrowed: the repr's value
/// assert_eq!(column.to_vec(), [SensorName("kitchen".to_owned())]); // owned: the newtype
/// ```
#[macro_export]
macro_rules! newtype_datatype {
    ($newtype:ty, $repr:ty) => {
        impl $crate::Datatype for $newtype {
            const NULLABLE: bool = <$repr as $crate::Datatype>::NULLABLE;
            type Typed = <$repr as $crate::Datatype>::Typed;
            type Value<'a>
                = <$repr as $crate::Datatype>::Value<'a>
            where
                Self: 'a;
            type Owned = $newtype;

            fn datatype() -> $crate::arrow::datatypes::DataType {
                <$repr as $crate::Datatype>::datatype()
            }

            fn downcast(
                array: &dyn $crate::arrow::array::Array,
            ) -> ::core::result::Result<Self::Typed, $crate::ColumnError> {
                <$repr as $crate::Datatype>::downcast(array)
            }

            fn is_null(typed: &Self::Typed, index: usize) -> bool {
                <$repr as $crate::Datatype>::is_null(typed, index)
            }

            fn value(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
                <$repr as $crate::Datatype>::value(typed, index)
            }

            fn build(
                values: impl ::core::iter::Iterator<Item = ::core::option::Option<Self::Owned>>,
            ) -> ::core::result::Result<$crate::arrow::array::ArrayRef, $crate::ColumnError> {
                <$repr as $crate::Datatype>::build(
                    values.map(|value| value.map(::core::convert::Into::into)),
                )
            }

            fn to_owned_value(value: Self::Value<'_>) -> Self::Owned {
                ::core::convert::From::from(<$repr as $crate::Datatype>::to_owned_value(value))
            }
        }

        impl $crate::InfallibleBuild for $newtype where $repr: $crate::InfallibleBuild {}
    };
}
