//! Support for domain types: making `Column<MyType>` work.
//!
//! * For types you own: [`newtype_datatype!`](crate::newtype_datatype)
//!   (must be invoked in the crate declaring the type, per the orphan rule).
//! * For foreign types: the [`As`] adapter, e.g. `Column<As<Ipv4Addr, u32>>`.

/// Implements [`LogicalType`] for a domain newtype,
/// making `Column<MyType>` work — including nesting (`List<MyType>`),
/// the convenience constructors, and the derive.
///
/// The newtype must convert to and from the representation's *owned value*
/// ([`LogicalType::Owned`], e.g. `String` for `Utf8`):
/// `impl From<MyType> for Owned` and `impl From<Owned> for MyType`.
///
/// Reading stays zero-copy and yields the *representation's* borrowed value
/// (e.g. `&str` for a `Utf8`-backed newtype);
/// owned values ([`Column::to_vec`](crate::Column::to_vec) etc.) are the newtype.
///
/// `column[index]` works too, borrowing through the representation.
/// That requires the representation to implement
/// [`RefType`] — most do, but not e.g. `bool` or
/// `List<…>`; for those, add a trailing `noref` to skip the `Index` support.
///
/// For representations that implement [`PrimitiveType`]
/// (primitives, [`FixedSizeBinary<N>`](crate::FixedSizeBinary)), add a trailing
/// `primitive` to also enable the bulk zero-copy
/// [`Column::as_slice`](crate::Column::as_slice) — which, like
/// reading, yields the *representation's* values
/// (e.g. `&[[u8; 16]]` for a `FixedSizeBinary<16>`-backed newtype):
///
/// ```
/// use quiver::FixedSizeBinary;
///
/// #[derive(Debug, PartialEq)]
/// struct Uuid([u8; 16]);
///
/// impl From<[u8; 16]> for Uuid {
///     fn from(bytes: [u8; 16]) -> Self {
///         Self(bytes)
///     }
/// }
/// impl From<Uuid> for [u8; 16] {
///     fn from(uuid: Uuid) -> Self {
///         uuid.0
///     }
/// }
///
/// quiver::newtype_datatype!(Uuid, FixedSizeBinary<16>, primitive);
///
/// let column = quiver::Column::<Uuid>::from_values([Uuid([7; 16])]);
/// assert_eq!(column.as_slice(), &[[7_u8; 16]]); // bulk, zero-copy
/// ```
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
/// quiver::newtype_datatype!(SensorName, quiver::Utf8);
///
/// let column = quiver::Column::<SensorName>::from_values([
///     SensorName("kitchen".to_owned()),
/// ]);
/// assert_eq!(column.value(0), "kitchen"); // borrowed: the repr's value
/// assert_eq!(&column[0], "kitchen"); // indexing, also borrowed
/// assert_eq!(column.to_vec(), [SensorName("kitchen".to_owned())]); // owned: the newtype
/// ```
#[macro_export]
macro_rules! newtype_datatype {
    ($newtype:ty, $repr:ty) => {
        $crate::newtype_datatype!($newtype, $repr, noref);

        impl $crate::RefType for $newtype {
            type Ref = <$repr as $crate::RefType>::Ref;

            fn value_ref(typed: &Self::Typed, index: usize) -> &Self::Ref {
                <$repr as $crate::RefType>::value_ref(typed, index)
            }
        }
    };

    ($newtype:ty, $repr:ty, primitive) => {
        $crate::newtype_datatype!($newtype, $repr);

        impl $crate::PrimitiveType for $newtype {
            type Native = <$repr as $crate::PrimitiveType>::Native;

            fn values(typed: &Self::Typed) -> &[Self::Native] {
                <$repr as $crate::PrimitiveType>::values(typed)
            }
        }
    };

    ($newtype:ty, $repr:ty, noref) => {
        impl $crate::LogicalType for $newtype {
            const NULLABLE: bool = <$repr as $crate::LogicalType>::NULLABLE;
            type Typed = <$repr as $crate::LogicalType>::Typed;
            type Value<'a>
                = <$repr as $crate::LogicalType>::Value<'a>
            where
                Self: 'a;
            type Owned = $newtype;

            fn matches(actual: &$crate::arrow::datatypes::DataType) -> bool {
                <$repr as $crate::LogicalType>::matches(actual)
            }

            fn supported_datatypes() -> ::std::vec::Vec<$crate::arrow::datatypes::DataType> {
                <$repr as $crate::LogicalType>::supported_datatypes()
            }

            fn downcast(
                array: &dyn $crate::arrow::array::Array,
            ) -> ::core::result::Result<Self::Typed, $crate::ColumnError> {
                <$repr as $crate::LogicalType>::downcast(array)
            }

            fn is_null(typed: &Self::Typed, index: usize) -> bool {
                <$repr as $crate::LogicalType>::is_null(typed, index)
            }

            fn value(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
                <$repr as $crate::LogicalType>::value(typed, index)
            }

            fn to_owned_value(value: Self::Value<'_>) -> Self::Owned {
                ::core::convert::From::from(<$repr as $crate::LogicalType>::to_owned_value(value))
            }
        }

        impl $crate::ConcreteType for $newtype
        where
            $repr: $crate::ConcreteType,
        {
            fn datatype() -> $crate::arrow::datatypes::DataType {
                <$repr as $crate::ConcreteType>::datatype()
            }

            fn build(
                values: impl ::core::iter::Iterator<Item = ::core::option::Option<Self::Owned>>,
            ) -> ::core::result::Result<$crate::arrow::array::ArrayRef, $crate::ColumnError> {
                <$repr as $crate::ConcreteType>::build(
                    values.map(|value| value.map(::core::convert::Into::into)),
                )
            }
        }

        impl $crate::InfallibleBuild for $newtype where $repr: $crate::InfallibleBuild {}
    };
}

use std::marker::PhantomData;

use crate::datatype::{ColumnError, InfallibleBuild, LogicalType, PrimitiveType, RefType};

/// Adapter for using a *foreign* type (one you don't own, so
/// [`newtype_datatype!`](crate::newtype_datatype) is off-limits by the orphan rule)
/// as a logical column type, stored as `Repr`:
///
/// ```
/// use std::net::Ipv4Addr;
///
/// use quiver::{As, Column};
///
/// type IpColumn = Column<As<Ipv4Addr, u32>>; // u32: the arrow representation
///
/// let column = IpColumn::from_values([Ipv4Addr::LOCALHOST]);
/// assert_eq!(column.value(0), u32::from(Ipv4Addr::LOCALHOST)); // borrowed: the repr's value
/// assert_eq!(column.to_vec(), [Ipv4Addr::LOCALHOST]); // owned: the foreign type
/// ```
///
/// Requires `From` conversions between the foreign type and the representation's
/// owned value, in both directions.
/// Like [`newtype_datatype!`](crate::newtype_datatype), reading stays zero-copy and
/// yields the *representation's* borrowed value; owned values are the foreign type.
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct As<T, Repr> {
    _marker: PhantomData<fn() -> (T, Repr)>,
}

impl<T, Repr> LogicalType for As<T, Repr>
where
    T: 'static,
    Repr: LogicalType + 'static,
    T: From<Repr::Owned>,
    Repr::Owned: From<T>,
{
    const NULLABLE: bool = Repr::NULLABLE;
    type Typed = Repr::Typed;
    type Value<'a>
        = Repr::Value<'a>
    where
        Self: 'a;
    type Owned = T;

    fn matches(actual: &arrow::datatypes::DataType) -> bool {
        Repr::matches(actual)
    }

    fn supported_datatypes() -> Vec<arrow::datatypes::DataType> {
        Repr::supported_datatypes()
    }

    fn downcast(array: &dyn arrow::array::Array) -> Result<Self::Typed, ColumnError> {
        Repr::downcast(array)
    }

    fn is_null(typed: &Self::Typed, index: usize) -> bool {
        Repr::is_null(typed, index)
    }

    fn value(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
        Repr::value(typed, index)
    }

    fn to_owned_value(value: Self::Value<'_>) -> Self::Owned {
        T::from(Repr::to_owned_value(value))
    }
}

impl<T, Repr> crate::ConcreteType for As<T, Repr>
where
    T: 'static,
    Repr: crate::ConcreteType + 'static,
    T: From<Repr::Owned>,
    Repr::Owned: From<T>,
{
    fn datatype() -> arrow::datatypes::DataType {
        Repr::datatype()
    }

    fn build(
        values: impl Iterator<Item = Option<Self::Owned>>,
    ) -> Result<arrow::array::ArrayRef, ColumnError> {
        Repr::build(values.map(|value| value.map(Repr::Owned::from)))
    }
}

impl<T, Repr> InfallibleBuild for As<T, Repr>
where
    T: 'static,
    Repr: LogicalType + InfallibleBuild + 'static,
    T: From<Repr::Owned>,
    Repr::Owned: From<T>,
{
}

/// Like reading, `column[index]` yields the *representation's* reference.
impl<T, Repr> RefType for As<T, Repr>
where
    T: 'static,
    Repr: RefType + 'static,
    T: From<Repr::Owned>,
    Repr::Owned: From<T>,
{
    type Ref = Repr::Ref;

    fn value_ref(typed: &Self::Typed, index: usize) -> &Self::Ref {
        Repr::value_ref(typed, index)
    }
}

/// Like reading, [`Column::as_slice`](crate::Column::as_slice) yields
/// the *representation's* values.
impl<T, Repr> PrimitiveType for As<T, Repr>
where
    T: 'static,
    Repr: PrimitiveType + 'static,
    T: From<Repr::Owned>,
    Repr::Owned: From<T>,
{
    type Native = Repr::Native;

    fn values(typed: &Self::Typed) -> &[Self::Native] {
        Repr::values(typed)
    }
}
