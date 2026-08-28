//! The [`As`] adapter: a foreign type stored as, and read as, a representation.

use std::marker::PhantomData;

use crate::data_type::{ColumnError, InfallibleBuild, LogicalType, PrimitiveType, RefType};

/// Adapter for using a *foreign* type (one you don't own, so
/// [`newtype_data_type!`](crate::newtype_data_type) is off-limits by the orphan rule)
/// as a logical column type, stored as `Repr`:
///
/// ```
/// use std::net::Ipv4Addr;
///
/// use quiver::{As, TypedArray};
///
/// type IpColumn = TypedArray<As<Ipv4Addr, u32>>; // u32: the arrow representation
///
/// let array = IpColumn::from_values([Ipv4Addr::LOCALHOST]);
/// assert_eq!(array.value(0), u32::from(Ipv4Addr::LOCALHOST)); // borrowed: the repr's value
/// assert_eq!(array.to_vec(), [Ipv4Addr::LOCALHOST]); // owned: the foreign type
/// ```
///
/// Requires `From` conversions between the foreign type and the representation's
/// owned value, in both directions.
/// Like [`newtype_data_type!`](crate::newtype_data_type), reading stays zero-copy and
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
    type Optional = ::core::option::Option<Self>;
    type Required = Self;

    fn downcast(array: &dyn arrow::array::Array) -> Result<Self::Typed, ColumnError> {
        Repr::downcast(array)
    }

    #[inline]
    fn is_null(typed: &Self::Typed, index: usize) -> bool {
        Repr::is_null(typed, index)
    }

    #[inline]
    unsafe fn is_null_unchecked(typed: &Self::Typed, index: usize) -> bool {
        // SAFETY: the caller guarantees `index` is in bounds.
        unsafe { Repr::is_null_unchecked(typed, index) }
    }

    #[inline]
    fn value(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
        Repr::value(typed, index)
    }

    #[inline]
    unsafe fn value_unchecked(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
        // SAFETY: the caller guarantees `index` is in bounds.
        unsafe { Repr::value_unchecked(typed, index) }
    }

    fn to_owned_value(value: Self::Value<'_>) -> Self::Owned {
        T::from(Repr::to_owned_value(value))
    }

    fn slice_typed(typed: &Self::Typed, offset: usize, length: usize) -> Option<Self::Typed> {
        Repr::slice_typed(typed, offset, length)
    }
}

impl<T, Repr> crate::ConcreteType for As<T, Repr>
where
    T: 'static,
    Repr: crate::ConcreteType + 'static,
    T: From<Repr::Owned>,
    Repr::Owned: From<T>,
{
    fn data_type() -> arrow::datatypes::DataType {
        Repr::data_type()
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
