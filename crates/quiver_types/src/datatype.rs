//! The [`LogicalType`] trait: the bridge between quiver's logical column types
//! and the arrow arrays they are stored in.

use arrow::array::{Array, ArrayRef};
use arrow::datatypes::DataType;

use crate::ErrorKind;

/// A logical column type, e.g. `Utf8`, `Option<i64>`, or `List<Utf8>`.
///
/// `Option<L>` means the values at this nesting level may be null.
///
/// The primitive Rust types implement it directly, and `Option<L>` adds
/// nullability at any nesting level:
///
/// ```
/// use quiver::Column;
///
/// let numbers = Column::<i64>::from_values([1, 2, 3]);
/// assert_eq!(numbers.value(0), 1);
/// assert_eq!(numbers.as_slice(), &[1, 2, 3]); // bulk, zero-copy (not for `bool`)
///
/// let maybe = Column::<Option<i64>>::from_values([Some(1), None]);
/// assert_eq!(maybe.value(1), None);
/// ```
pub trait LogicalType {
    /// May the values at this level be null? (`true` only for `Option<…>`)
    const NULLABLE: bool = false;

    /// The fully-downcast, validated representation of one column of this datatype.
    /// Cheap to clone (arrow arrays are reference-counted).
    type Typed: Clone;

    /// Zero-copy element view: `&'a str` for `Utf8`, `i64` for `i64`,
    /// an iterator for `List<T>`, `Option<…>` for `Option<T>`.
    type Value<'a>
    where
        Self: 'a;

    /// The owned value of one element, used by the convenience constructors:
    /// `String` for `Utf8`, `Option<i64>` for `Option<i64>`, `Vec<…>` for `List<…>`, etc.
    type Owned;

    /// Validates that `array` has an acceptable datatype, then recursively
    /// downcasts it — checking the nulls of all *children* along the way.
    ///
    /// This is the single validation+downcast hook, called once per column at the
    /// boundary ([`Column::try_new`](crate::Column::try_new)). It must reject any
    /// `array` whose datatype this logical type does not accept (returning
    /// [`ColumnError::WrongDatatype`]), *including* datatype parameters not
    /// encoded in the concrete arrow array's Rust type — a
    /// [`FixedSizeBinary`](crate::FixedSizeBinary) / [`FixedSizeList`](crate::FixedSizeList)
    /// size, a [`Timestamp`](crate::Timestamp) timezone, etc. The leaf type check
    /// comes from the concrete-array downcast; nested element types are
    /// validated by recursing into the children's `downcast`.
    ///
    /// Multi-encoding types like [`AnyList`](crate::AnyList) inspect
    /// `array.data_type()` and dispatch to the matching encoding.
    ///
    /// Nulls at the level of `array` itself are the responsibility of the caller
    /// (the parent datatype, or [`Column::try_new`](crate::Column::try_new) at the
    /// top level), because only the caller knows if this level is wrapped in an
    /// `Option`.
    ///
    /// # Errors
    /// Errors on a datatype mismatch, or on unexpected nulls in children.
    fn downcast(array: &dyn Array) -> Result<Self::Typed, ColumnError>;

    /// Is the value at `index` null?
    fn is_null(typed: &Self::Typed, index: usize) -> bool;

    /// The value at `index`.
    ///
    /// Contract: `index` is in bounds, and the value is non-null unless `Self` is an `Option`.
    fn value(typed: &Self::Typed, index: usize) -> Self::Value<'_>;

    /// The value at `index`, **without** the bounds check [`value`](LogicalType::value)
    /// performs.
    ///
    /// Reading a validated [`Column`](crate::Column) or
    /// [`ListValue`](crate::ListValue) is the hot path: the bounds are known
    /// once (the column length, or a list element's offset range), so this lets
    /// bulk iteration skip arrow's per-element bounds check. The default just
    /// forwards to [`value`](LogicalType::value); encodings override it to call
    /// arrow's unchecked accessor.
    ///
    /// # Safety
    /// `index` must be in bounds (`< length`) — the same precondition as
    /// [`value`](LogicalType::value), but here it is the caller's responsibility,
    /// not checked.
    unsafe fn value_unchecked(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
        Self::value(typed, index)
    }

    /// Converts a borrowed element value into an owned one,
    /// e.g. `&str` → `String`.
    fn to_owned_value(value: Self::Value<'_>) -> Self::Owned;
}

/// A [`LogicalType`] that corresponds to a *single* concrete arrow datatype,
/// and can therefore be built and used to generate schemas.
///
/// Implemented by every logical type except multi-encoding ones like
/// [`AnyList`](crate::AnyList), which accept several arrow datatypes and so have
/// no single [`datatype`](ConcreteType::datatype) to report or build — those are
/// parse-only (read via [`LogicalType`], but no `from_values`/`Default`/schema).
pub trait ConcreteType: LogicalType {
    /// The exact arrow datatype, built recursively
    /// (including the nullability of inner fields).
    fn datatype() -> DataType;

    /// Builds an arrow array of this datatype from owned values.
    ///
    /// `None` items only ever occur at `Option<…>` levels.
    ///
    /// # Errors
    /// Building can only fail for fallible encodings —
    /// today that is dictionary key overflow (see [`crate::Dictionary`]).
    /// Implementations of [`InfallibleBuild`] never error.
    fn build(values: impl Iterator<Item = Option<Self::Owned>>) -> Result<ArrayRef, ColumnError>;
}

/// Logical types whose values are stored in one contiguous buffer of
/// primitive values, enabling the zero-copy
/// [`Column::as_slice`](crate::Column::as_slice).
///
/// Implemented by the primitive types except `bool` (arrow bit-packs it),
/// by the primitive-backed marker types
/// ([`Date32`](crate::Date32), [`Timestamp`](crate::Timestamp), …),
/// and by [`FixedSizeBinary<N>`](crate::FixedSizeBinary) (stored contiguously).
pub trait PrimitiveType: LogicalType {
    /// The in-memory element type: `f32` for `f32`, `i64` for `Timestamp<…>`,
    /// `[u8; N]` for `[u8; N]`, etc.
    type Native;

    /// All values as one contiguous slice.
    fn values(typed: &Self::Typed) -> &[Self::Native];
}

/// Logical types whose element values can be borrowed as plain references
/// (`&str`, `&i64`, …), enabling `column[index]`
/// (see [`Column`](crate::Column)'s `Index` impl).
///
/// Implemented by strings, binaries, primitives, and the primitive-backed
/// marker types — but not `bool` (arrow bit-packs it, so there is no `&bool`
/// to hand out), and not nullable (`Option<…>`) or nested (`List<…>`) types,
/// whose values are built on the fly.
pub trait RefType: LogicalType {
    /// The borrow target: `str` for `Utf8`, `i64` for `i64`, etc.
    type Ref: ?Sized;

    /// A reference to the value at `index`.
    ///
    /// Contract: `index` is in bounds, and the value is non-null.
    fn value_ref(typed: &Self::Typed, index: usize) -> &Self::Ref;
}

/// Marker for logical types whose [`ConcreteType::build`] can never fail,
/// making the convenient [`Column::from_values`](crate::Column::from_values)
/// (and `From<Vec<T>>`, `FromIterator`) available.
///
/// Implemented by every concrete logical type except [`Dictionary`](crate::Dictionary)
/// and [`Run`](crate::Run), whose encodings can fail (key / run-end overflow) —
/// use [`Column::try_from_values`](crate::Column::try_from_values) there.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be built infallibly",
    note = "dictionary/run encoding can fail (overflow): use `Column::try_from_values` instead of `from_values`"
)]
pub trait InfallibleBuild: ConcreteType {}

/// What can go wrong when constructing a [`Column`](crate::Column).
///
/// Does not know which column it concerns — see [`ColumnError::for_column`].
#[derive(Debug, thiserror::Error)]
pub enum ColumnError {
    #[error("Expected {expected}, found {actual:?}")]
    WrongDatatype {
        /// A description of the datatype the logical type expected, produced by
        /// the failing [`LogicalType::downcast`] (e.g. `"Utf8"`, `"List(…)"`).
        expected: String,
        actual: DataType,
    },

    #[error(
        "Found {null_count} null(s) at a non-nullable level. Use `Option<…>` in the logical type to allow nulls"
    )]
    UnexpectedNulls { null_count: usize },

    #[error("Failed to build the array: {0}")]
    Build(arrow::error::ArrowError),
}

impl ColumnError {
    /// Attach the column name, producing an [`ErrorKind`].
    pub fn for_column(self, column: String) -> ErrorKind {
        match self {
            Self::WrongDatatype { expected, actual } => ErrorKind::WrongDatatype {
                column,
                expected,
                actual,
            },
            Self::UnexpectedNulls { null_count } => {
                ErrorKind::UnexpectedNulls { column, null_count }
            }
            Self::Build(err) => ErrorKind::BuildRecordBatch(err),
        }
    }
}

/// Lets `?` convert column errors in functions returning arrow results.
///
/// The error is preserved (including its source chain),
/// wrapped as an [`arrow::error::ArrowError::ExternalError`].
impl From<ColumnError> for arrow::error::ArrowError {
    fn from(err: ColumnError) -> Self {
        Self::ExternalError(Box::new(err))
    }
}

/// Downcasts and clones (cheaply) a typed arrow array, validating the array's
/// concrete type in the process.
///
/// This is the leaf datatype check: a wrong array type yields
/// [`ColumnError::WrongDatatype`]. Datatype *parameters* not encoded in the Rust
/// type (a fixed size, a timestamp timezone) must be checked separately by the
/// caller — see [`LogicalType::downcast`].
pub(crate) fn downcast_array<A: Array + Clone + 'static>(
    array: &dyn Array,
    expected: impl FnOnce() -> String,
) -> Result<A, ColumnError> {
    array
        .as_any()
        .downcast_ref::<A>()
        .cloned()
        .ok_or_else(|| ColumnError::WrongDatatype {
            expected: expected(),
            actual: array.data_type().clone(),
        })
}

macro_rules! impl_flat_datatype {
    ($rust:ty, $array:ty, $value:ty, $datatype:expr) => {
        impl LogicalType for $rust {
            type Typed = $array;
            type Value<'a> = $value;
            type Owned = $rust;

            fn downcast(array: &dyn Array) -> Result<Self::Typed, ColumnError> {
                downcast_array::<$array>(array, || format!("{:?}", $datatype))
            }

            fn is_null(typed: &Self::Typed, index: usize) -> bool {
                typed.is_null(index)
            }

            fn value(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
                typed.value(index)
            }

            unsafe fn value_unchecked(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
                // SAFETY: the caller guarantees `index` is in bounds.
                unsafe { typed.value_unchecked(index) }
            }

            fn to_owned_value(value: Self::Value<'_>) -> Self::Owned {
                value.into()
            }
        }

        impl crate::datatype::ConcreteType for $rust {
            fn datatype() -> DataType {
                $datatype
            }

            fn build(
                values: impl Iterator<Item = Option<Self::Owned>>,
            ) -> Result<ArrayRef, ColumnError> {
                Ok(std::sync::Arc::new(<$array>::from_iter(values)))
            }
        }

        impl crate::datatype::InfallibleBuild for $rust {}
    };
}

pub(crate) use impl_flat_datatype;

/// Implements [`PrimitiveType`] and [`RefType`] for a logical type
/// whose `Typed` array is an [`arrow::array::PrimitiveArray`].
macro_rules! impl_primitive_datatype {
    ($logical:ty, $native:ty) => {
        impl crate::datatype::PrimitiveType for $logical {
            type Native = $native;

            fn values(typed: &Self::Typed) -> &[$native] {
                typed.values()
            }
        }

        impl crate::datatype::RefType for $logical {
            type Ref = $native;

            fn value_ref(typed: &Self::Typed, index: usize) -> &$native {
                &typed.values()[index]
            }
        }
    };
}

pub(crate) use impl_primitive_datatype;

/// Implements [`LogicalType`] for a marker type whose owned value differs from the
/// marker itself (e.g. the marker `Date32` has `i32` values).
macro_rules! impl_marker_datatype {
    ($marker:ty, $array:ty, $value:ty, $owned:ty, $datatype:expr) => {
        impl LogicalType for $marker {
            type Typed = $array;
            type Value<'a> = $value;
            type Owned = $owned;

            fn downcast(array: &dyn Array) -> Result<Self::Typed, ColumnError> {
                crate::datatype::downcast_array::<$array>(array, || format!("{:?}", $datatype))
            }

            fn is_null(typed: &Self::Typed, index: usize) -> bool {
                typed.is_null(index)
            }

            fn value(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
                typed.value(index)
            }

            unsafe fn value_unchecked(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
                // SAFETY: the caller guarantees `index` is in bounds.
                unsafe { typed.value_unchecked(index) }
            }

            fn to_owned_value(value: Self::Value<'_>) -> Self::Owned {
                value.into()
            }
        }

        impl crate::datatype::ConcreteType for $marker {
            fn datatype() -> DataType {
                $datatype
            }

            fn build(
                values: impl Iterator<Item = Option<Self::Owned>>,
            ) -> Result<ArrayRef, ColumnError> {
                Ok(std::sync::Arc::new(<$array>::from_iter(values)))
            }
        }

        impl crate::datatype::InfallibleBuild for $marker {}
    };
}

pub(crate) use impl_marker_datatype;
