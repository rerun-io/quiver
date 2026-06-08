//! The [`Datatype`] trait: the bridge between quiver's logical column types
//! and the arrow arrays they are stored in.

use arrow::array::{Array, ArrayRef};
use arrow::datatypes::DataType;

use crate::ErrorKind;

/// A logical column type, e.g. `Utf8`, `Option<i64>`, or `List<Utf8>`.
///
/// `Option<L>` means the values at this nesting level may be null.
pub trait Datatype {
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

    /// The exact arrow datatype, built recursively
    /// (including the nullability of inner fields).
    fn datatype() -> DataType;

    /// Does this logical type accept arrow arrays of datatype `actual`?
    ///
    /// This is the datatype-matching hook, called once per column at the
    /// validation boundary ([`Column::try_new`](crate::Column::try_new)).
    ///
    /// The default implementation compares `actual` against [`Self::datatype`]
    /// with [`datatypes_compatible`]: like equality, except that the inner
    /// [`arrow::datatypes::Field`]s of nested datatypes are compared
    /// *structurally* — their names, nullability flags, and metadata are ignored.
    ///
    /// Container types ([`List`](crate::List), [`FixedSizeList`](crate::FixedSizeList),
    /// [`Dictionary`](crate::Dictionary), `Option<…>`) forward to the `matches`
    /// of their inner types, so an override applies at any nesting depth.
    ///
    /// Custom logical types can override this to accept *several* datatypes,
    /// e.g. a timestamp of any unit, or any datatype at all.
    ///
    /// Contract: `matches` must accept only datatypes that [`Self::downcast`]
    /// can handle. Error messages still report [`Self::datatype`] as the
    /// expected datatype.
    fn matches(actual: &DataType) -> bool {
        datatypes_compatible(actual, &Self::datatype())
    }

    /// Recursively downcasts the array, checking the nulls of all *children*.
    ///
    /// Nulls at the level of `array` itself are the responsibility of the caller
    /// (the parent datatype, or [`Column::try_new`](crate::Column::try_new) at the top level),
    /// because only the caller knows if this level is wrapped in an `Option`.
    ///
    /// # Errors
    /// Errors on unexpected nulls in children.
    /// The datatype is assumed to have already been checked (see [`Column::try_new`](crate::Column::try_new)),
    /// making the downcasts themselves infallible.
    fn downcast(array: &dyn Array) -> Result<Self::Typed, ColumnError>;

    /// Is the value at `index` null?
    fn is_null(typed: &Self::Typed, index: usize) -> bool;

    /// The value at `index`.
    ///
    /// Contract: `index` is in bounds, and the value is non-null unless `Self` is an `Option`.
    fn value(typed: &Self::Typed, index: usize) -> Self::Value<'_>;

    /// Builds an arrow array of this datatype from owned values.
    ///
    /// `None` items only ever occur at `Option<…>` levels.
    ///
    /// # Errors
    /// Building can only fail for fallible encodings —
    /// today that is dictionary key overflow (see [`crate::Dictionary`]).
    /// Implementations of [`InfallibleBuild`] never error.
    fn build(values: impl Iterator<Item = Option<Self::Owned>>) -> Result<ArrayRef, ColumnError>;

    /// Converts a borrowed element value into an owned one,
    /// e.g. `&str` → `String`.
    fn to_owned_value(value: Self::Value<'_>) -> Self::Owned;
}

/// Logical types whose values are stored in one contiguous buffer of
/// primitive values, enabling the zero-copy
/// [`Column::as_slice`](crate::Column::as_slice).
///
/// Implemented by the primitive types except `bool` (arrow bit-packs it),
/// by the primitive-backed marker types
/// ([`Date32`](crate::Date32), [`Timestamp`](crate::Timestamp), …),
/// and by [`FixedSizeBinary<N>`](crate::FixedSizeBinary) (stored contiguously).
pub trait PrimitiveDatatype: Datatype {
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
pub trait RefDatatype: Datatype {
    /// The borrow target: `str` for `Utf8`, `i64` for `i64`, etc.
    type Ref: ?Sized;

    /// A reference to the value at `index`.
    ///
    /// Contract: `index` is in bounds, and the value is non-null.
    fn value_ref(typed: &Self::Typed, index: usize) -> &Self::Ref;
}

/// Marker for logical types whose [`Datatype::build`] can never fail,
/// making the convenient [`Column::from_values`](crate::Column::from_values)
/// (and `From<Vec<T>>`, `FromIterator`) available.
///
/// Implemented by every logical type except [`Dictionary`](crate::Dictionary),
/// whose encoding can fail (key overflow) — use
/// [`Column::try_from_values`](crate::Column::try_from_values) there.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be built infallibly",
    note = "dictionary encoding can fail (key overflow): use `Column::try_from_values` instead of `from_values`"
)]
pub trait InfallibleBuild: Datatype {}

/// What can go wrong when constructing a [`Column`](crate::Column).
///
/// Does not know which column it concerns — see [`ColumnError::for_column`].
#[derive(Debug, thiserror::Error)]
pub enum ColumnError {
    #[error("Expected datatype {expected:?}, found {actual:?}")]
    WrongDatatype {
        expected: DataType,
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

/// Downcasts and clones (cheaply) a typed arrow array.
///
/// The datatype has already been validated, so a failure here is a bug —
/// but we return an error instead of panicking, to be safe.
pub(crate) fn downcast_array<A: Array + Clone + 'static>(
    array: &dyn Array,
) -> Result<A, ColumnError> {
    array
        .as_any()
        .downcast_ref::<A>()
        .cloned()
        .ok_or_else(|| ColumnError::WrongDatatype {
            expected: DataType::Null, // unreachable; see docstring
            actual: array.data_type().clone(),
        })
}

macro_rules! impl_flat_datatype {
    ($rust:ty, $array:ty, $value:ty, $datatype:expr) => {
        impl Datatype for $rust {
            type Typed = $array;
            type Value<'a> = $value;
            type Owned = $rust;

            fn datatype() -> DataType {
                $datatype
            }

            fn downcast(array: &dyn Array) -> Result<Self::Typed, ColumnError> {
                downcast_array::<$array>(array)
            }

            fn is_null(typed: &Self::Typed, index: usize) -> bool {
                typed.is_null(index)
            }

            fn value(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
                typed.value(index)
            }

            fn build(
                values: impl Iterator<Item = Option<Self::Owned>>,
            ) -> Result<ArrayRef, ColumnError> {
                Ok(std::sync::Arc::new(<$array>::from_iter(values)))
            }

            fn to_owned_value(value: Self::Value<'_>) -> Self::Owned {
                value.into()
            }
        }

        impl crate::datatype::InfallibleBuild for $rust {}
    };
}

pub(crate) use impl_flat_datatype;

/// Implements [`PrimitiveDatatype`] and [`RefDatatype`] for a logical type
/// whose `Typed` array is an [`arrow::array::PrimitiveArray`].
macro_rules! impl_primitive_datatype {
    ($logical:ty, $native:ty) => {
        impl crate::datatype::PrimitiveDatatype for $logical {
            type Native = $native;

            fn values(typed: &Self::Typed) -> &[$native] {
                typed.values()
            }
        }

        impl crate::datatype::RefDatatype for $logical {
            type Ref = $native;

            fn value_ref(typed: &Self::Typed, index: usize) -> &$native {
                &typed.values()[index]
            }
        }
    };
}

pub(crate) use impl_primitive_datatype;

/// Are arrow arrays of datatype `actual` acceptable where `declared` is expected?
///
/// Like equality, except that the inner [`arrow::datatypes::Field`]s of nested
/// datatypes are compared *structurally*: their names (`"item"` vs `"element"`),
/// nullability flags, and metadata are ignored.
/// Actual nullability is enforced separately, by counting logical nulls.
///
/// This is the default implementation of [`Datatype::matches`];
/// it is public so that custom `matches` overrides can fall back on it.
pub fn datatypes_compatible(actual: &DataType, declared: &DataType) -> bool {
    match (actual, declared) {
        (DataType::List(actual), DataType::List(declared))
        | (DataType::LargeList(actual), DataType::LargeList(declared)) => {
            datatypes_compatible(actual.data_type(), declared.data_type())
        }
        (
            DataType::FixedSizeList(actual, actual_size),
            DataType::FixedSizeList(declared, declared_size),
        ) => {
            actual_size == declared_size
                && datatypes_compatible(actual.data_type(), declared.data_type())
        }
        (
            DataType::Dictionary(actual_key, actual_value),
            DataType::Dictionary(declared_key, declared_value),
        ) => {
            datatypes_compatible(actual_key, declared_key)
                && datatypes_compatible(actual_value, declared_value)
        }
        _ => actual == declared,
    }
}

/// Implements [`Datatype`] for a marker type whose owned value differs from the
/// marker itself (e.g. the marker `Date32` has `i32` values).
macro_rules! impl_marker_datatype {
    ($marker:ty, $array:ty, $value:ty, $owned:ty, $datatype:expr) => {
        impl Datatype for $marker {
            type Typed = $array;
            type Value<'a> = $value;
            type Owned = $owned;

            fn datatype() -> DataType {
                $datatype
            }

            fn downcast(array: &dyn Array) -> Result<Self::Typed, ColumnError> {
                crate::datatype::downcast_array::<$array>(array)
            }

            fn is_null(typed: &Self::Typed, index: usize) -> bool {
                typed.is_null(index)
            }

            fn value(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
                typed.value(index)
            }

            fn build(
                values: impl Iterator<Item = Option<Self::Owned>>,
            ) -> Result<ArrayRef, ColumnError> {
                Ok(std::sync::Arc::new(<$array>::from_iter(values)))
            }

            fn to_owned_value(value: Self::Value<'_>) -> Self::Owned {
                value.into()
            }
        }

        impl crate::datatype::InfallibleBuild for $marker {}
    };
}

pub(crate) use impl_marker_datatype;
