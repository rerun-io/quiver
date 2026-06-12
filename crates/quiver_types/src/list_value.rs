//! [`ListValue`]: one element of a list column, a `Column`-like view of its items.

use crate::datatype::{LogicalType, PrimitiveType, RefType};

/// One list element of a list column (`List`, [`LargeList`](crate::LargeList),
/// [`FixedSizeList`](crate::FixedSizeList), …): a zero-copy, random-access view
/// of that row's typed items.
///
/// It mirrors [`Column`](crate::Column)'s read API — the items behave like a
/// borrowed slice of a single-typed column:
///
/// - length: [`len`](ListValue::len), [`is_empty`](ListValue::is_empty)
/// - by-item access: [`get`](ListValue::get) / [`value`](ListValue::value)
///   (zero-copy views) and [`get_owned`](ListValue::get_owned) /
///   [`value_owned`](ListValue::value_owned) (owned), plus `list[i]` where the
///   item can be borrowed from the array (see the [`Index`](std::ops::Index) impl)
/// - bulk: [`to_vec`](ListValue::to_vec), and [`as_slice`](ListValue::as_slice)
///   for primitive items (one contiguous zero-copy slice)
/// - iteration: [`iter`](ListValue::iter) / [`iter_owned`](ListValue::iter_owned)
///
/// `ListValue` is itself an [`Iterator`] over the items, so `.map(…)` /
/// `.collect()` / `.sum()` work directly on it. It is [`Copy`]: consuming it as
/// an iterator advances a cursor (the random-access methods then see the
/// remaining items), while [`iter`](ListValue::iter) hands out a fresh cursor
/// without consuming the original.
///
/// The item range is validated once (against the array's offsets) at
/// construction, so element access — including iteration — never re-checks
/// bounds.
///
/// ```
/// use quiver::{Column, List};
///
/// let column = Column::<List<i64>>::from_values([vec![10, 20, 30], vec![]]);
/// let row = column.value(0);
///
/// assert_eq!(row.len(), 3);
/// assert_eq!(row.value(1), 20);     // by item index
/// assert_eq!(row[2], 30);           // borrowed (primitive items)
/// assert_eq!(row.as_slice(), &[10, 20, 30]); // contiguous, zero-copy
/// assert_eq!(row.to_vec(), vec![10, 20, 30]);
///
/// let sum: i64 = row.iter().sum();  // `iter` does not consume `row`
/// assert_eq!(sum, 60);
///
/// assert!(column.value(1).is_empty());
/// ```
pub struct ListValue<'a, L: LogicalType> {
    values: &'a L::Typed,
    index: usize,
    end: usize,
}

impl<L: LogicalType> Clone for ListValue<'_, L> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<L: LogicalType> Copy for ListValue<'_, L> {}

impl<'a, L: LogicalType + 'a> ListValue<'a, L> {
    /// `index..end` into `values`.
    ///
    /// The caller must uphold `index <= end <= values`' length — the invariant
    /// arrow's list offsets give us. This is the single bounds check: every
    /// later item access skips re-checking (see
    /// [`value_unchecked`](LogicalType::value_unchecked)), so a bad range here
    /// would make even safe iteration unsound. The `debug_assert` catches an
    /// inverted range (in tests and under Miri); `end <= length` rests on
    /// arrow's offsets and can't be cheaply checked generically.
    pub(crate) fn new(values: &'a L::Typed, index: usize, end: usize) -> Self {
        debug_assert!(index <= end, "ListValue range {index}..{end} is inverted");
        Self { values, index, end }
    }

    /// The value of the item at `index`, without bounds checking.
    ///
    /// # Safety
    /// `self.index + index < self.end`.
    #[inline]
    unsafe fn value_unchecked(&self, index: usize) -> L::Value<'a> {
        // SAFETY: by the caller's contract `self.index + index < self.end <=
        // values' length`, so the absolute item index is in bounds.
        unsafe { L::value_unchecked(self.values, self.index + index) }
    }

    /// The number of items in this list element.
    #[must_use]
    #[inline]
    pub fn len(&self) -> usize {
        self.end - self.index
    }

    /// Is this list element empty?
    #[must_use]
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.index == self.end
    }

    /// The item at `index`, or `None` if out of bounds.
    ///
    /// See [`ListValue::value`] for the returned view;
    /// [`ListValue::get_owned`] returns the owned value instead.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<L::Value<'a>> {
        // SAFETY: bounds checked here, once.
        (index < self.len()).then(|| unsafe { self.value_unchecked(index) })
    }

    /// The owned item at `index`, or `None` if out of bounds —
    /// e.g. `String` where [`ListValue::get`] returns `&str`.
    #[must_use]
    pub fn get_owned(&self, index: usize) -> Option<L::Owned> {
        self.get(index).map(L::to_owned_value)
    }

    /// The item at `index`, returning the zero-copy view
    /// ([`LogicalType::Value`]); for the owned value see [`ListValue::value_owned`].
    ///
    /// Where the item can be borrowed from the array, `list[index]` works too
    /// (see the [`Index`](std::ops::Index) impl).
    ///
    /// Panics if out of bounds.
    #[must_use]
    pub fn value(&self, index: usize) -> L::Value<'a> {
        assert!(
            index < self.len(),
            "ListValue index {index} out of bounds for length {}",
            self.len()
        );
        // SAFETY: bounds checked just above.
        unsafe { self.value_unchecked(index) }
    }

    /// The owned item at `index` — e.g. `String` where [`ListValue::value`]
    /// returns `&str`.
    ///
    /// Panics if out of bounds.
    #[must_use]
    pub fn value_owned(&self, index: usize) -> L::Owned {
        L::to_owned_value(self.value(index))
    }

    /// Iterates over the zero-copy views ([`LogicalType::Value`]) of the
    /// remaining items, without consuming `self`.
    ///
    /// For owned values, see [`ListValue::iter_owned`].
    #[must_use]
    pub fn iter(&self) -> Self {
        *self
    }

    /// Iterates over the owned values of the remaining items —
    /// e.g. `String` where [`ListValue::iter`] yields `&str`.
    pub fn iter_owned(&self) -> impl Iterator<Item = L::Owned> + 'a {
        self.iter().map(L::to_owned_value)
    }

    /// Copies the items into a `Vec` of owned values,
    /// e.g. `Vec<String>` for a `List<Utf8>` element.
    #[must_use]
    pub fn to_vec(self) -> Vec<L::Owned> {
        self.iter_owned().collect()
    }
}

/// `for item in &list` — iterates the items without consuming the view.
impl<'a, L: LogicalType + 'a> IntoIterator for &ListValue<'a, L> {
    type Item = L::Value<'a>;
    type IntoIter = ListValue<'a, L>;

    fn into_iter(self) -> Self::IntoIter {
        *self
    }
}

/// `list[index]`: like [`ListValue::value`], but borrows from the array —
/// `&list[i]` is `&str` for a `List<Utf8>` element, `&i64` for `List<i64>`.
///
/// Available for items that can be borrowed from the array: strings, binaries,
/// and primitives — but not `bool`, `Option<…>`, or nested `List<…>` items.
///
/// Panics if out of bounds (like [`ListValue::value`]).
impl<L: RefType> std::ops::Index<usize> for ListValue<'_, L> {
    type Output = L::Ref;

    fn index(&self, index: usize) -> &Self::Output {
        assert!(
            index < self.len(),
            "ListValue index {index} out of bounds for length {}",
            self.len()
        );
        L::value_ref(self.values, self.index + index)
    }
}

impl<'a, L: PrimitiveType> ListValue<'a, L> {
    /// The items as a contiguous zero-copy slice,
    /// e.g. `&[f32]` for a `List<f32>` element.
    ///
    /// Only available for primitive and fixed-size binary items
    /// (`bool` is excluded: arrow bit-packs it).
    #[must_use]
    pub fn as_slice(&self) -> &'a [L::Native] {
        &L::values(self.values)[self.index..self.end]
    }
}

// Iteration mirrors a slice's: the items sit in `self.index..self.end`, a range
// validated once at construction, so every step reads with
// [`value_unchecked`](LogicalType::value_unchecked) — no per-element bounds
// check. The combinators are overridden to also skip the `Option` plumbing of
// the default `next`-based implementations. (Primitive items have an even
// faster path: [`ListValue::as_slice`].)
impl<'a, L: LogicalType + 'a> Iterator for ListValue<'a, L> {
    type Item = L::Value<'a>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.end {
            // SAFETY: index < end <= values' length.
            let value = unsafe { L::value_unchecked(self.values, self.index) };
            self.index += 1;
            Some(value)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.end - self.index;
        (remaining, Some(remaining))
    }

    fn count(self) -> usize {
        self.end - self.index
    }

    fn last(self) -> Option<Self::Item> {
        // SAFETY: when non-empty, `end - 1` is in `index..end`.
        (self.index < self.end).then(|| unsafe { L::value_unchecked(self.values, self.end - 1) })
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        match self.index.checked_add(n) {
            Some(target) if target < self.end => {
                self.index = target + 1;
                // SAFETY: target < end <= values' length.
                Some(unsafe { L::value_unchecked(self.values, target) })
            }
            _ => {
                self.index = self.end;
                None
            }
        }
    }

    fn fold<B, F>(self, init: B, mut f: F) -> B
    where
        F: FnMut(B, Self::Item) -> B,
    {
        let Self { values, index, end } = self;
        let mut acc = init;
        for i in index..end {
            // SAFETY: i < end <= values' length.
            acc = f(acc, unsafe { L::value_unchecked(values, i) });
        }
        acc
    }
}

impl<'a, L: LogicalType + 'a> DoubleEndedIterator for ListValue<'a, L> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.index < self.end {
            self.end -= 1;
            // SAFETY: the new `end` is in `index..old end`, hence in bounds.
            Some(unsafe { L::value_unchecked(self.values, self.end) })
        } else {
            None
        }
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        match self.end.checked_sub(n + 1) {
            Some(target) if self.index <= target => {
                self.end = target;
                // SAFETY: index <= target < old end <= values' length.
                Some(unsafe { L::value_unchecked(self.values, target) })
            }
            _ => {
                self.end = self.index;
                None
            }
        }
    }

    fn rfold<B, F>(self, init: B, mut f: F) -> B
    where
        F: FnMut(B, Self::Item) -> B,
    {
        let Self { values, index, end } = self;
        let mut acc = init;
        for i in (index..end).rev() {
            // SAFETY: i < end <= values' length.
            acc = f(acc, unsafe { L::value_unchecked(values, i) });
        }
        acc
    }
}

impl<'a, L: LogicalType + 'a> ExactSizeIterator for ListValue<'a, L> {}

impl<'a, L: LogicalType + 'a> std::iter::FusedIterator for ListValue<'a, L> {}
