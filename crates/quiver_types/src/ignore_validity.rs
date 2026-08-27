//! [`IgnoreValidity<L>`]: a logical type for arrow datatypes whose validity
//! mask carries no information.
//!
//! Quiver's default is strict: a `Column<i64>` is null-free by construction,
//! and an array that carries a validity mask is rejected with
//! [`ColumnError::UnexpectedNulls`]. That is the right default, and this module
//! is the one documented way out of it — for the case where the *datatype's own
//! contract* says the mask is meaningless, so there is nothing to reject.

use std::marker::PhantomData;

use arrow::array::{Array, ArrayRef};
use arrow::datatypes::DataType;

use crate::datatype::{ColumnError, InfallibleBuild, LogicalType, PrimitiveType, RefType};

/// Reads `L` while ignoring arrow's validity mask: the values are read whether
/// or not a mask is present, and a mask is never an error.
///
/// Some arrow datatypes declare, as part of their contract, that their validity
/// mask is not meaningful — every slot holds a real value, and a producer may
/// still attach a mask. Quiver's strict default rejects such an array:
///
/// ```
/// # use std::sync::Arc;
/// # use quiver::arrow::array::{ArrayRef, Int64Array};
/// # use quiver::{Column, ColumnError, IgnoreValidity};
/// // An array whose mask says "null", but whose value buffer is fully populated:
/// let array: ArrayRef = Arc::new(Int64Array::from(vec![Some(1), None, Some(3)]));
///
/// assert!(matches!(
///     Column::<i64>::try_new(ArrayRef::clone(&array)),
///     Err(ColumnError::UnexpectedNulls { null_count: 1 }),
/// ));
///
/// // `IgnoreValidity` accepts it, and reads the values as they are:
/// let column = Column::<IgnoreValidity<i64>>::try_new(array)?;
/// assert_eq!(column.value(0), 1);
/// assert_eq!(column.as_slice(), &[1, 0, 3]); // bulk, zero-copy — the raw buffer
/// # Ok::<(), ColumnError>(())
/// ```
///
/// # What you are asserting
/// That every slot of the value buffer holds a value you are happy to read. The
/// masked-off slots are *not* skipped and *not* replaced: whatever the producer
/// left there is what you get (arrow zero-fills in the common case, as above,
/// but that is not guaranteed).
///
/// If the mask is genuinely meaningful, you want [`Option<L>`] instead —
/// `Column<Option<i64>>` reads `Option<i64>` and never hands you a value that
/// was masked off.
///
/// # What it does not change
/// * **The declared datatype.** [`datatype`](crate::ConcreteType::datatype) and
///   [`NULLABLE`](LogicalType::NULLABLE) come straight from `L`, so
///   `IgnoreValidity<L>` declares the same non-nullable arrow field `L` does.
///   That is the point: the field is not nullable, the mask is just noise.
/// * **The values.** [`Value`](LogicalType::Value),
///   [`Owned`](LogicalType::Owned), `Index`, and
///   [`as_slice`](crate::Column::as_slice) are all `L`'s, so
///   `Column<IgnoreValidity<L>>` reads exactly like `Column<L>`.
/// * **Nested levels.** It speaks only for its own level:
///   `IgnoreValidity<List<i64>>` ignores the mask on the *rows*, while the
///   items are still validated by `List`'s own rules. Ignore an inner mask by
///   putting it there instead — `List<IgnoreValidity<i64>>`.
///
/// # Reading only
/// This is a *read-side* assertion, and quiver stays strict on the way out. The
/// mask rides along on the validated array, so putting a masked column back
/// into a record batch fails — arrow will not accept a masked array in the
/// non-nullable field this type declares:
///
/// ```
/// # use std::sync::Arc;
/// # use quiver::arrow::array::{ArrayRef, Int64Array};
/// # use quiver::arrow::datatypes::Schema;
/// # use quiver::arrow::record_batch::RecordBatch;
/// # use quiver::{Column, ColumnDesc, IgnoreValidity};
/// const VALUE: ColumnDesc<IgnoreValidity<i64>> = ColumnDesc::new("Row", "value");
///
/// let array: ArrayRef = Arc::new(Int64Array::from(vec![Some(1), None]));
/// let column = Column::<IgnoreValidity<i64>>::try_new(array)?;
///
/// // The schema this descriptor declares is non-nullable, and arrow enforces
/// // that against the mask the array still carries:
/// let schema = Arc::new(Schema::new(vec![VALUE.arrow_field()]));
/// let err = RecordBatch::try_new(schema, vec![column.into_arrow()]).unwrap_err();
/// assert!(err.to_string().contains("declared as non-nullable"));
/// # Ok::<(), quiver::ColumnError>(())
/// ```
///
/// That is deliberate: an array whose non-nullable field carries a mask is not
/// valid arrow, which is why it needed an escape hatch to read in the first
/// place. To emit one, either declare the column nullable on the write side, or
/// rebuild it from the values.
///
/// # Through a newtype
/// This composes with [`newtype_datatype!`](crate::newtype_datatype), which
/// forwards [`REJECTS_NULLS`](LogicalType::REJECTS_NULLS) from the
/// representation — so a newtype whose datatype contract declares the mask
/// meaningless says so once, in the representation, and the decision travels
/// with the type:
///
/// ```
/// # use quiver::{Column, FixedSizeBinary, IgnoreValidity};
/// #[derive(Debug, PartialEq, Clone, Copy, quiver::bytemuck::Pod, quiver::bytemuck::Zeroable)]
/// #[bytemuck(crate = "::quiver::bytemuck")]
/// #[repr(transparent)]
/// struct Tuid([u8; 16]);
///
/// # impl From<[u8; 16]> for Tuid {
/// #     fn from(bytes: [u8; 16]) -> Self { Self(bytes) }
/// # }
/// # impl From<Tuid> for [u8; 16] {
/// #     fn from(tuid: Tuid) -> Self { tuid.0 }
/// # }
/// quiver::newtype_datatype!(Tuid, IgnoreValidity<FixedSizeBinary<16>>, primitive);
///
/// // `Column<Tuid>` at every call site — no wrapper to name, and still the
/// // bulk zero-copy read:
/// let column = Column::<Tuid>::from_values([Tuid([7; 16])]);
/// let tuids: &[Tuid] = column.as_slice();
/// assert_eq!(tuids, &[Tuid([7; 16])]);
/// assert!(!Column::<Tuid>::NULLABLE); // still a non-nullable field
/// ```
///
/// This type is never instantiated — it only appears as a type parameter.
#[doc(alias = "nulls")]
#[doc(alias = "validity")]
#[doc(alias = "AnyValidity")]
pub struct IgnoreValidity<L> {
    _marker: PhantomData<fn() -> L>,
}

impl<L: LogicalType> LogicalType for IgnoreValidity<L> {
    // Read as `L` does — only the *rejection* of a mask changes:
    const NULLABLE: bool = L::NULLABLE;
    const REJECTS_NULLS: bool = false;

    type Typed = L::Typed;
    type Value<'a>
        = L::Value<'a>
    where
        Self: 'a;
    type Owned = L::Owned;
    type Optional = Option<Self>;
    type Required = Self;

    fn downcast(array: &dyn Array) -> Result<Self::Typed, ColumnError> {
        L::downcast(array)
    }

    /// Always `false`: this type asserts that every slot holds a value, so no
    /// element reads as null even where the mask says otherwise.
    #[inline]
    fn is_null(_typed: &Self::Typed, _index: usize) -> bool {
        false
    }

    #[inline]
    unsafe fn is_null_unchecked(_typed: &Self::Typed, _index: usize) -> bool {
        false
    }

    #[inline]
    fn value(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
        L::value(typed, index)
    }

    #[inline]
    unsafe fn value_unchecked(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
        // SAFETY: the caller guarantees `index` is in bounds.
        unsafe { L::value_unchecked(typed, index) }
    }

    fn to_owned_value(value: Self::Value<'_>) -> Self::Owned {
        L::to_owned_value(value)
    }
}

impl<L: crate::ConcreteType> crate::ConcreteType for IgnoreValidity<L> {
    fn datatype() -> DataType {
        L::datatype()
    }

    fn build(values: impl Iterator<Item = Option<Self::Owned>>) -> Result<ArrayRef, ColumnError> {
        L::build(values)
    }
}

impl<L: InfallibleBuild> InfallibleBuild for IgnoreValidity<L> {}

/// Bulk zero-copy reads, as for `L` — the whole reason to reach for this type.
impl<L: PrimitiveType> PrimitiveType for IgnoreValidity<L> {
    type Native = L::Native;

    #[inline]
    fn values(typed: &Self::Typed) -> &[Self::Native] {
        L::values(typed)
    }
}

impl<L: RefType> RefType for IgnoreValidity<L> {
    type Ref = L::Ref;

    #[inline]
    fn value_ref(typed: &Self::Typed, index: usize) -> &Self::Ref {
        L::value_ref(typed, index)
    }
}
