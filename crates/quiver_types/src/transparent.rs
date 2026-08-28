//! The [`Transparent`] adapter: a domain-type tag that converts nothing.

use std::marker::PhantomData;

use crate::data_type::{ColumnError, InfallibleBuild, LogicalType, PrimitiveType, RefType};

/// Adapter for *tagging* a column with a domain type, paying nothing for it.
///
/// `Transparent<T, Repr>` behaves exactly like `Repr` — the same arrow data
/// type, the same borrowed values, and the same *owned* values — with `T` only
/// a type-level decoration:
///
/// ```
/// use quiver::{Transparent, TypedArray};
///
/// /// An even integer.
/// #[derive(Debug, PartialEq)]
/// struct Even(i64);
///
/// impl TryFrom<i64> for Even {
///     type Error = i64;
///     fn try_from(value: i64) -> Result<Self, i64> {
///         if value % 2 == 0 { Ok(Self(value)) } else { Err(value) }
///     }
/// }
///
/// type EvenColumn = TypedArray<Transparent<Even, i64>>;
///
/// let array = EvenColumn::from_values([2, 4]); // built from the repr's values,
/// assert_eq!(array.value(0), 2); // read back as the repr's values,
/// assert_eq!(array.to_vec(), [2, 4]); // owned ones included: nothing converts
///
/// assert_eq!(Even::try_from(array.value(0)), Ok(Even(2))); // convert where you need it
/// ```
///
/// Compare with [`As`](crate::As), which *converts*: `As<T, Repr>` has `T` as its owned
/// value, so it requires an infallible `T: From<Repr::Owned>` and converts on
/// every owned read. A domain type that is only `TryFrom` the representation can
/// go through [`try_newtype_data_type!`](crate::try_newtype_data_type) instead
/// (foreign types excepted, by the orphan rule), but quiver's "validate once,
/// then read infallibly" contract makes that check *every* value at column
/// construction. `Transparent` is the way out when that validation costs too
/// much: nothing is validated, nothing is converted, and the caller converts
/// the values it actually cares about.
///
/// So a `Transparent<T, …>` column carries no promise that its values satisfy
/// `T`'s invariant — the tag says what the values are *meant* to be, and the
/// conversion stays fallible at the point of use.
///
/// The `T: TryFrom<Repr::Owned>` bound is all that is required (an infallible
/// `From` satisfies it). Quiver never calls it; it only keeps the tag honest.
///
/// This type is never instantiated — it only appears as a type parameter.
pub struct Transparent<T, Repr> {
    _marker: PhantomData<fn() -> (T, Repr)>,
}

impl<T, Repr> LogicalType for Transparent<T, Repr>
where
    T: TryFrom<Repr::Owned> + 'static,
    Repr: LogicalType + 'static,
{
    const NULLABLE: bool = Repr::NULLABLE;
    type Typed = Repr::Typed;
    type Value<'a>
        = Repr::Value<'a>
    where
        Self: 'a;
    type Owned = Repr::Owned;
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
        Repr::to_owned_value(value)
    }

    fn slice_typed(typed: &Self::Typed, offset: usize, length: usize) -> Option<Self::Typed> {
        Repr::slice_typed(typed, offset, length)
    }
}

impl<T, Repr> crate::ConcreteType for Transparent<T, Repr>
where
    T: TryFrom<Repr::Owned> + 'static,
    Repr: crate::ConcreteType + 'static,
{
    fn data_type() -> arrow::datatypes::DataType {
        Repr::data_type()
    }

    fn build(
        values: impl Iterator<Item = Option<Self::Owned>>,
    ) -> Result<arrow::array::ArrayRef, ColumnError> {
        Repr::build(values)
    }
}

impl<T, Repr> InfallibleBuild for Transparent<T, Repr>
where
    T: TryFrom<Repr::Owned> + 'static,
    Repr: InfallibleBuild + 'static,
{
}

/// Like reading, `column[index]` yields the *representation's* reference.
impl<T, Repr> RefType for Transparent<T, Repr>
where
    T: TryFrom<Repr::Owned> + 'static,
    Repr: RefType + 'static,
{
    type Ref = Repr::Ref;

    fn value_ref(typed: &Self::Typed, index: usize) -> &Self::Ref {
        Repr::value_ref(typed, index)
    }
}

/// Like reading, [`Column::as_slice`](crate::Column::as_slice) yields
/// the *representation's* values.
impl<T, Repr> PrimitiveType for Transparent<T, Repr>
where
    T: TryFrom<Repr::Owned> + 'static,
    Repr: PrimitiveType + 'static,
{
    type Native = Repr::Native;

    fn values(typed: &Self::Typed) -> &[Self::Native] {
        Repr::values(typed)
    }
}
