//! Support for domain types: making `Column<MyType>` work.
//!
//! * For types you own: [`newtype_data_type!`](crate::newtype_data_type)
//!   (must be invoked in the crate declaring the type, per the orphan rule).
//! * For foreign types: the [`As`](crate::As) adapter, e.g. `Column<As<Ipv4Addr, u32>>`.
//! * For a type you want only as a *tag*, with no conversion and no up-front
//!   validation: the [`Transparent`](crate::Transparent) adapter, e.g. `Column<Transparent<Even, i64>>`.

use crate::data_type::PrimitiveType;

/// Implements [`LogicalType`](crate::LogicalType) for a domain newtype,
/// making `Column<MyType>` work — including nesting (`List<MyType>`),
/// the convenience constructors, and the derive.
///
/// The newtype must convert to and from the representation's *owned value*
/// ([`LogicalType::Owned`](crate::LogicalType::Owned), e.g. `String` for `Utf8`):
/// `impl From<MyType> for Owned` and `impl From<Owned> for MyType`.
///
/// Reading stays zero-copy and yields the *representation's* borrowed value
/// (e.g. `&str` for a `Utf8`-backed newtype);
/// owned values ([`Column::to_vec`](crate::Column::to_vec) etc.) are the newtype.
///
/// `column[index]` works too, borrowing through the representation (the
/// `primitive` arm below borrows the newtype instead).
/// That requires the representation to implement
/// [`RefType`](crate::RefType) — most do, but not e.g. `bool` or
/// `List<…>`; for those, add a trailing `noref` to skip the `Index` support.
///
/// For representations that implement [`PrimitiveType`]
/// (primitives, [`FixedSizeBinary<N>`](crate::FixedSizeBinary)), add a trailing
/// `primitive` to also enable the bulk zero-copy
/// [`Column::as_slice`](crate::Column::as_slice), which yields the *newtype*
/// (e.g. `&[Uuid]` for a `FixedSizeBinary<16>`-backed `Uuid`) — and then
/// `column[index]` borrows the newtype as well, so the two agree.
///
/// That reinterprets the representation's buffer as the newtype, so the newtype
/// must be layout-compatible with the representation's native type and accept
/// every bit pattern — spelled as [`bytemuck::Pod`].
/// The size and alignment are checked at compile time.
///
/// Quiver re-exports `bytemuck`, derive included, so you do not have to depend
/// on it yourself; the `crate` attribute points the derive back at the
/// re-export:
///
/// ```
/// use quiver::FixedSizeBinary;
///
/// #[derive(Debug, PartialEq, Clone, Copy, quiver::bytemuck::Pod, quiver::bytemuck::Zeroable)]
/// #[bytemuck(crate = "::quiver::bytemuck")]
/// #[repr(transparent)]
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
/// quiver::newtype_data_type!(Uuid, FixedSizeBinary<16>, primitive);
///
/// let array = quiver::TypedArray::<Uuid>::from_values([Uuid([7; 16])]);
/// assert_eq!(array.as_slice(), &[Uuid([7; 16])]); // bulk, zero-copy
/// assert_eq!(array[0], Uuid([7; 16])); // and element-wise, the same type
/// ```
///
/// A newtype that cannot be `Pod` — because it is not layout-compatible, or
/// because its type has a *niche*, a bit pattern the compiler treats as
/// impossible (`NonZero*`, [`char`]) — has no bulk read here. Where handing back the *representation's* values is
/// still useful, write the three-line [`PrimitiveType`] impl by hand:
///
/// ```
/// # struct Even(i64);
/// # impl From<i64> for Even { fn from(value: i64) -> Self { Self(value) } }
/// # impl From<Even> for i64 { fn from(even: Even) -> Self { even.0 } }
/// # quiver::newtype_data_type!(Even, i64);
/// impl quiver::PrimitiveType for Even {
///     type Native = i64;
///
///     fn values(typed: &Self::Typed) -> &[i64] {
///         <i64 as quiver::PrimitiveType>::values(typed)
///     }
/// }
///
/// let array = quiver::TypedArray::<Even>::from_values([Even(2), Even(4)]);
/// assert_eq!(array.as_slice(), &[2_i64, 4]);
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
/// quiver::newtype_data_type!(SensorName, quiver::Utf8);
///
/// let array = quiver::TypedArray::<SensorName>::from_values([
///     SensorName("kitchen".to_owned()),
/// ]);
/// assert_eq!(array.value(0), "kitchen"); // borrowed: the repr's value
/// assert_eq!(&array[0], "kitchen"); // indexing, also borrowed
/// assert_eq!(array.to_vec(), [SensorName("kitchen".to_owned())]); // owned: the newtype
/// ```
#[macro_export]
macro_rules! newtype_data_type {
    ($newtype:ty, $repr:ty) => {
        $crate::newtype_data_type!($newtype, $repr, noref);

        impl $crate::RefType for $newtype {
            type Ref = <$repr as $crate::RefType>::Ref;

            fn value_ref(typed: &Self::Typed, index: usize) -> &Self::Ref {
                <$repr as $crate::RefType>::value_ref(typed, index)
            }
        }
    };

    ($newtype:ty, $repr:ty, primitive) => {
        $crate::newtype_data_type!($newtype, $repr, noref);

        impl $crate::RefType for $newtype {
            // The newtype itself, not the representation's `Ref`: the arm
            // already requires the two to be layout-compatible, so `column[i]`
            // can hand back the newtype, agreeing with `as_slice`.
            type Ref = Self;

            fn value_ref(typed: &Self::Typed, index: usize) -> &Self {
                // Checked at compile time: `Self` has the same size and
                // alignment as the representation's native type.
                $crate::bytemuck::must_cast_ref(<$repr as $crate::RefType>::value_ref(typed, index))
            }
        }

        impl $crate::PrimitiveType for $newtype {
            type Native = Self;

            fn values(typed: &Self::Typed) -> &[Self] {
                // Checked at compile time: `Self` has the same size and
                // alignment as the representation's native type.
                $crate::bytemuck::must_cast_slice(<$repr as $crate::PrimitiveType>::values(typed))
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
            type Optional = ::core::option::Option<Self>;
            type Required = Self;

            fn downcast(
                array: &dyn $crate::arrow::array::Array,
            ) -> ::core::result::Result<Self::Typed, $crate::ColumnError> {
                <$repr as $crate::LogicalType>::downcast(array)
            }

            #[inline]
            fn is_null(typed: &Self::Typed, index: usize) -> bool {
                <$repr as $crate::LogicalType>::is_null(typed, index)
            }

            #[inline]
            unsafe fn is_null_unchecked(typed: &Self::Typed, index: usize) -> bool {
                // SAFETY: the caller guarantees `index` is in bounds.
                unsafe { <$repr as $crate::LogicalType>::is_null_unchecked(typed, index) }
            }

            #[inline]
            fn value(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
                <$repr as $crate::LogicalType>::value(typed, index)
            }

            #[inline]
            unsafe fn value_unchecked(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
                // SAFETY: the caller guarantees `index` is in bounds.
                unsafe { <$repr as $crate::LogicalType>::value_unchecked(typed, index) }
            }

            fn to_owned_value(value: Self::Value<'_>) -> Self::Owned {
                ::core::convert::From::from(<$repr as $crate::LogicalType>::to_owned_value(value))
            }

            fn slice_typed(
                typed: &Self::Typed,
                offset: usize,
                length: usize,
            ) -> ::core::option::Option<Self::Typed> {
                <$repr as $crate::LogicalType>::slice_typed(typed, offset, length)
            }
        }

        impl $crate::ConcreteType for $newtype
        where
            $repr: $crate::ConcreteType,
        {
            fn data_type() -> $crate::arrow::datatypes::DataType {
                <$repr as $crate::ConcreteType>::data_type()
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

/// Like [`newtype_data_type!`](crate::newtype_data_type), but for a **fallible**
/// conversion *from* the representation's owned value.
///
/// The newtype provides `impl TryFrom<Owned> for MyType` instead of
/// `impl From<Owned> for MyType` (the reverse, `impl From<MyType> for Owned`,
/// must still be infallible, for building). The `TryFrom::Error` must be
/// `std::error::Error + Send + Sync + 'static`.
///
/// The standard `NonZero*` integer types and [`char`] are wired up out of the
/// box (they are `TryFrom` a primitive quiver supports), so `Column<NonZeroU32>`
/// and `Column<char>` just work.
///
/// Consistent with [`Column`](crate::Column)'s "validate once, then read
/// infallibly" contract, the conversion of *every* value is checked eagerly at
/// construction ([`Column::try_new`](crate::Column::try_new), and the derive's
/// record-batch parsing). A rejected value stops there, boxing the `TryFrom`
/// error into [`ColumnError::Conversion`](crate::ColumnError::Conversion) — surfaced as
/// [`ErrorKind::Conversion`](crate::ErrorKind::Conversion) once the column name
/// is known. After that, element access is infallible, as usual.
///
/// The trailing `noref` / `primitive` arguments work exactly as in
/// [`newtype_data_type!`](crate::newtype_data_type). `primitive` asks for
/// [`bytemuck::Pod`], which is about the *layout*, not about the domain: the
/// validating `Even` below is `Pod` all the same — every bit pattern is a valid
/// `Even` **struct**, and evenness is checked once per element at construction,
/// so `&[Even]` is sound and correct. What rules `Pod` out is a *niche*, a bit
/// pattern the compiler treats as impossible — which is why the built-in
/// `NonZero*` and [`char`] columns use the hand-written [`PrimitiveType`] impl
/// shown there instead.
///
/// ```
/// #[derive(Debug, PartialEq, Clone, Copy, quiver::bytemuck::Pod, quiver::bytemuck::Zeroable)]
/// #[bytemuck(crate = "::quiver::bytemuck")]
/// #[repr(transparent)]
/// struct Even(i64);
///
/// #[derive(Debug)]
/// struct NotEven(i64);
/// impl std::fmt::Display for NotEven {
///     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
///         write!(f, "{} is not even", self.0)
///     }
/// }
/// impl std::error::Error for NotEven {}
///
/// impl TryFrom<i64> for Even {
///     type Error = NotEven;
///     fn try_from(value: i64) -> Result<Self, NotEven> {
///         if value % 2 == 0 { Ok(Self(value)) } else { Err(NotEven(value)) }
///     }
/// }
/// impl From<Even> for i64 {
///     fn from(even: Even) -> Self {
///         even.0
///     }
/// }
///
/// quiver::try_newtype_data_type!(Even, i64, primitive);
///
/// // Building goes through the infallible `From<Even> for i64`:
/// let array = quiver::TypedArray::<Even>::from_values([Even(2), Even(4)]);
/// assert_eq!(array.to_vec(), [Even(2), Even(4)]);
///
/// // Validation is not layout: `primitive` gives the bulk zero-copy read,
/// // and every element has already been checked.
/// assert_eq!(array.as_slice(), &[Even(2), Even(4)]);
///
/// // A array whose values don't all convert is rejected at construction:
/// use quiver::arrow::array::Int64Array;
/// let array = std::sync::Arc::new(Int64Array::from(vec![2, 3]));
/// assert!(quiver::TypedArray::<Even>::try_new(array).is_err());
/// ```
#[macro_export]
macro_rules! try_newtype_data_type {
    ($newtype:ty, $repr:ty) => {
        $crate::try_newtype_data_type!($newtype, $repr, noref);

        impl $crate::RefType for $newtype {
            type Ref = <$repr as $crate::RefType>::Ref;

            fn value_ref(typed: &Self::Typed, index: usize) -> &Self::Ref {
                <$repr as $crate::RefType>::value_ref(typed, index)
            }
        }
    };

    ($newtype:ty, $repr:ty, primitive) => {
        $crate::try_newtype_data_type!($newtype, $repr, noref);

        impl $crate::RefType for $newtype {
            // The newtype itself; see `newtype_data_type!`'s `primitive` arm.
            type Ref = Self;

            fn value_ref(typed: &Self::Typed, index: usize) -> &Self {
                // Checked at compile time: `Self` has the same size and
                // alignment as the representation's native type.
                $crate::bytemuck::must_cast_ref(<$repr as $crate::RefType>::value_ref(typed, index))
            }
        }

        impl $crate::PrimitiveType for $newtype {
            type Native = Self;

            fn values(typed: &Self::Typed) -> &[Self] {
                // Checked at compile time: `Self` has the same size and
                // alignment as the representation's native type.
                $crate::bytemuck::must_cast_slice(<$repr as $crate::PrimitiveType>::values(typed))
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
            type Optional = ::core::option::Option<Self>;
            type Required = Self;

            fn downcast(
                array: &dyn $crate::arrow::array::Array,
            ) -> ::core::result::Result<Self::Typed, $crate::ColumnError> {
                let typed = <$repr as $crate::LogicalType>::downcast(array)?;
                // Validate every value converts, once, up front — so element
                // access can stay infallible afterwards.
                for index in 0..$crate::arrow::array::Array::len(array) {
                    if !<$repr as $crate::LogicalType>::is_null(&typed, index) {
                        let owned = <$repr as $crate::LogicalType>::to_owned_value(
                            <$repr as $crate::LogicalType>::value(&typed, index),
                        );
                        if let ::core::result::Result::Err(err) =
                            <$newtype as ::core::convert::TryFrom<_>>::try_from(owned)
                        {
                            return ::core::result::Result::Err($crate::ColumnError::Conversion(
                                ::std::boxed::Box::new(err),
                            ));
                        }
                    }
                }
                ::core::result::Result::Ok(typed)
            }

            #[inline]
            fn is_null(typed: &Self::Typed, index: usize) -> bool {
                <$repr as $crate::LogicalType>::is_null(typed, index)
            }

            #[inline]
            unsafe fn is_null_unchecked(typed: &Self::Typed, index: usize) -> bool {
                // SAFETY: the caller guarantees `index` is in bounds.
                unsafe { <$repr as $crate::LogicalType>::is_null_unchecked(typed, index) }
            }

            #[inline]
            fn value(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
                <$repr as $crate::LogicalType>::value(typed, index)
            }

            #[inline]
            unsafe fn value_unchecked(typed: &Self::Typed, index: usize) -> Self::Value<'_> {
                // SAFETY: the caller guarantees `index` is in bounds.
                unsafe { <$repr as $crate::LogicalType>::value_unchecked(typed, index) }
            }

            fn to_owned_value(value: Self::Value<'_>) -> Self::Owned {
                let owned = <$repr as $crate::LogicalType>::to_owned_value(value);
                match <$newtype as ::core::convert::TryFrom<_>>::try_from(owned) {
                    ::core::result::Result::Ok(value) => value,
                    // The column was validated at construction, so this cannot
                    // happen for a well-behaved (deterministic) `TryFrom`.
                    ::core::result::Result::Err(_) => ::core::panic!(::core::concat!(
                        "`",
                        ::core::stringify!($newtype),
                        "` conversion failed despite being validated at column \
                         construction; this indicates a non-deterministic `TryFrom` impl"
                    )),
                }
            }

            fn slice_typed(
                typed: &Self::Typed,
                offset: usize,
                length: usize,
            ) -> ::core::option::Option<Self::Typed> {
                <$repr as $crate::LogicalType>::slice_typed(typed, offset, length)
            }
        }

        impl $crate::ConcreteType for $newtype
        where
            $repr: $crate::ConcreteType,
        {
            fn data_type() -> $crate::arrow::datatypes::DataType {
                <$repr as $crate::ConcreteType>::data_type()
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

// Standard library types that are `TryFrom` a primitive quiver already
// supports, wired up with [`try_newtype_data_type!`]. Each is stored as (and
// read back as) that primitive; its invariant (non-zero, valid scalar value) is
// checked once at column construction.

/// Wires up every `NonZero*` integer over its plain integer representation.
///
/// The bulk read yields the plain integers, not the `NonZero*` themselves: a
/// zero is not a valid `NonZeroU32`, so the buffer cannot be reinterpreted the
/// way the `primitive` arm of [`try_newtype_data_type!`] does.
macro_rules! nonzero_data_type {
    ($($nonzero:ty => $int:ty),* $(,)?) => {
        $(
            crate::try_newtype_data_type!($nonzero, $int);

            impl PrimitiveType for $nonzero {
                type Native = $int;

                fn values(typed: &Self::Typed) -> &[$int] {
                    <$int as PrimitiveType>::values(typed)
                }
            }
        )*
    };
}

nonzero_data_type! {
    ::core::num::NonZeroI8   => i8,
    ::core::num::NonZeroI16  => i16,
    ::core::num::NonZeroI32  => i32,
    ::core::num::NonZeroI64  => i64,
    ::core::num::NonZeroU8   => u8,
    ::core::num::NonZeroU16  => u16,
    ::core::num::NonZeroU32  => u32,
    ::core::num::NonZeroU64  => u64,
}

// `char` is `TryFrom<u32>` (rejecting surrogates and out-of-range values),
// and `u32: From<char>`; stored as `UInt32`. The bulk read yields the `u32`s,
// for the same reason as the `NonZero*` above.
crate::try_newtype_data_type!(char, u32);

impl PrimitiveType for char {
    type Native = u32;

    fn values(typed: &Self::Typed) -> &[u32] {
        <u32 as PrimitiveType>::values(typed)
    }
}
