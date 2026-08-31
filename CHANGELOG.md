# `quiver` changelog

All notable changes to the `quiver` crates will be documented in this file.

This file is updated upon each release by `./scripts/generate_changelog.py`.
Do NOT add entries here manually — they are generated from PR titles and labels.


## 0.6.1 - 2026-08-31

Full diff at https://github.com/rerun-io/quiver/compare/0.6.0..0.6.1

#### New features
* Add `ColumnDesc::name_owned()` [#61](https://github.com/rerun-io/quiver/pull/61) by [@emilk](https://github.com/emilk)


## 0.6.0 - 2026-08-28

Full diff at https://github.com/rerun-io/quiver/compare/0.5.0..0.6.0

A `Column` (and `DynColumn`) now always carries a name. A column without a name is an array.

To that end, `TypedArray<L>` has been added. (A dynamic column is just an arrow `ArrayRef`).

Breaking, in brief: `Column::from_values(values)` becomes `Column::from_values(name, values)`, `datatype` is spelled `data_type` throughout, `err.kind` is boxed, and `DynColumn`'s fields are behind accessors.

#### ⚠️ Breaking changes
* Bump MSRV to 1.95.0 [#22](https://github.com/rerun-io/quiver/pull/22) by [@emilk](https://github.com/emilk)
* `ColumnDesc<L>` instead of `ColumnDesc<Column<L>>` [#24](https://github.com/rerun-io/quiver/pull/24) by [@emilk](https://github.com/emilk)
* Box `ErrorKind` inside `Error` [#31](https://github.com/rerun-io/quiver/pull/31) by [@emilk](https://github.com/emilk)
* Add `optional()` / `required()` to `ColumnDesc` and `Column` [#37](https://github.com/rerun-io/quiver/pull/37) by [@emilk](https://github.com/emilk)
* `Column<T>::as_slice()` now yields `&[T]` for newtypes [#41](https://github.com/rerun-io/quiver/pull/41) by [@emilk](https://github.com/emilk)
* Move `DynColumn` to its own file, with private, validated fields [#49](https://github.com/rerun-io/quiver/pull/49) by [@emilk](https://github.com/emilk)
* Rename `Datatype`/`datatype` to `DataType`/`data_type` [#50](https://github.com/rerun-io/quiver/pull/50) by [@emilk](https://github.com/emilk)
* Give `Column` a name [#52](https://github.com/rerun-io/quiver/pull/52) by [@emilk](https://github.com/emilk)
* Borrow the newtype, not the representation, on `primitive` newtypes [#56](https://github.com/rerun-io/quiver/pull/56) by [@emilk](https://github.com/emilk)

#### New features
* Make `TypedArray` public [#23](https://github.com/rerun-io/quiver/pull/23) by [@emilk](https://github.com/emilk)
* Add conversions between the typed and dynamic column types [#26](https://github.com/rerun-io/quiver/pull/26) by [@emilk](https://github.com/emilk)
* Add `optional()` / `required()` to `ColumnDesc` and `Column` [#37](https://github.com/rerun-io/quiver/pull/37) by [@emilk](https://github.com/emilk)
* Add `ColumnDesc::arrow_field_ref` [#38](https://github.com/rerun-io/quiver/pull/38) by [@emilk](https://github.com/emilk)
* Add a constructor for an all-null column [#42](https://github.com/rerun-io/quiver/pull/42) by [@emilk](https://github.com/emilk)
* Let `Column<L>` be used where `&[L::Native]` is expected [#43](https://github.com/rerun-io/quiver/pull/43) by [@emilk](https://github.com/emilk)
* Add `ColumnDesc::data_type()` [#48](https://github.com/rerun-io/quiver/pull/48) by [@emilk](https://github.com/emilk)
* Add `name()` to the column descriptors and `DynColumn` [#51](https://github.com/rerun-io/quiver/pull/51) by [@emilk](https://github.com/emilk)
* Add the `Transparent<T, Repr>` adapter: a domain-type tag with no conversion [#58](https://github.com/rerun-io/quiver/pull/58) by [@emilk](https://github.com/emilk)

#### Performance
* Box `ErrorKind` inside `Error` [#31](https://github.com/rerun-io/quiver/pull/31) by [@emilk](https://github.com/emilk)
* Make `Column` cheap to clone by wrapping its name and metadata in `Arc` [#54](https://github.com/rerun-io/quiver/pull/54) by [@emilk](https://github.com/emilk)
* Re-slice the downcast view instead of re-validating it [#57](https://github.com/rerun-io/quiver/pull/57) by [@emilk](https://github.com/emilk)

#### Other improvements
* Enable the pedantic clippy lints we had opted out of [#25](https://github.com/rerun-io/quiver/pull/25) by [@emilk](https://github.com/emilk)
* Derive `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq` for the column descriptors [#32](https://github.com/rerun-io/quiver/pull/32) by [@emilk](https://github.com/emilk)
* Loosen the argument types of `Column::with_metadata` and `DynColumn::try_new` [#53](https://github.com/rerun-io/quiver/pull/53) by [@emilk](https://github.com/emilk)
* Follow-ups from the 0.6 post-merge reviews [#55](https://github.com/rerun-io/quiver/pull/55) by [@emilk](https://github.com/emilk)


## 0.5.0 - 2026-07-03

Full diff at https://github.com/rerun-io/quiver/compare/0.4.0..0.5.0

#### New features
* Add `try_newtype_datatype!` for fallible domain conversions [#21](https://github.com/rerun-io/quiver/pull/21) by [@emilk](https://github.com/emilk)


## 0.4.0 - 2026-07-02

Full diff at https://github.com/rerun-io/quiver/compare/0.3.0..0.4.0

This release pins `arrow` to version 58.

#### ⚠️ Breaking changes
* Require `arrow` 58, previously `>=57, <60` [#19](https://github.com/rerun-io/quiver/pull/19) by [@IsseW](https://github.com/IsseW)

#### Other improvements
* Add a supported `arrow` versions table to the README [#19](https://github.com/rerun-io/quiver/pull/19) by [@IsseW](https://github.com/IsseW)


## 0.3.0 - 2026-06-12

Full diff at https://github.com/rerun-io/quiver/compare/0.2.0..0.3.0

A performance-focused release: reading and iterating a validated `Column` now skips arrow's per-element bounds checks, plus a new record-batch constructor.

#### ⚠️ Breaking changes
* Replace the implicit by-value `IntoIterator` for `Column` with an explicit `.into_iter_owned()` [#16](https://github.com/rerun-io/quiver/pull/16) by [@emilk](https://github.com/emilk)

#### New features
* Add `Column::from_record_batch_and_name` [#13](https://github.com/rerun-io/quiver/pull/13) by [@emilk](https://github.com/emilk)

#### Performance
* Skip per-element bounds checks when iterating `Column`/`ListValue` [#15](https://github.com/rerun-io/quiver/pull/15) by [@emilk](https://github.com/emilk)
* Unchecked reads for `AnyList`, an unchecked null probe for `Option`, and `#[inline]` on the per-element accessors [#17](https://github.com/rerun-io/quiver/pull/17) by [@emilk](https://github.com/emilk)

#### Other improvements
* Add benchmarks [#14](https://github.com/rerun-io/quiver/pull/14) by [@emilk](https://github.com/emilk)


## 0.2.0 - 2026-06-10

Full diff at https://github.com/rerun-io/quiver/compare/0.1.1..0.2.0

This release adds a family of new logical types, "any-encoding" types that abstract over the multiple arrow encodings of the same logical value, and support for `arrow` 59.

#### ⚠️ Breaking changes
* Replace `Column<String>` with `Utf8`/`LargeUtf8`/`Utf8View` markers [#5](https://github.com/rerun-io/quiver/pull/5) by [@emilk](https://github.com/emilk)

#### New logical types
* Add four arrow logical types: `BinaryView`, `LargeList`, `Map`, `Run` [#6](https://github.com/rerun-io/quiver/pull/6) by [@emilk](https://github.com/emilk)
* Add `ListView` and `LargeListView` logical types [#7](https://github.com/rerun-io/quiver/pull/7) by [@emilk](https://github.com/emilk)
* Add `AnyList<L>`: one logical type for any list encoding [#8](https://github.com/rerun-io/quiver/pull/8) by [@emilk](https://github.com/emilk)
* Add `AnyBinary`: one logical type for any binary encoding [#9](https://github.com/rerun-io/quiver/pull/9) by [@emilk](https://github.com/emilk)
* Add `AnyUtf8`: one logical type for any UTF-8 encoding [#10](https://github.com/rerun-io/quiver/pull/10) by [@emilk](https://github.com/emilk)

#### Other improvements
* Give `ListValue` a `Column`-like read API [#11](https://github.com/rerun-io/quiver/pull/11) by [@emilk](https://github.com/emilk)
* Add support for `arrow` 59 [#12](https://github.com/rerun-io/quiver/pull/12) by [@emilk](https://github.com/emilk)


## 0.1.1 - 2026-06-05

Full diff at https://github.com/rerun-io/quiver/compare/0.1.0..0.1.1

#### PRs
* Expose the datatype-matching hook: `Datatype::matches` [#1](https://github.com/rerun-io/quiver/pull/1) by [@emilk](https://github.com/emilk)
* Bulk zero-copy `as_slice()` for fixed-size binary columns [#2](https://github.com/rerun-io/quiver/pull/2) by [@emilk](https://github.com/emilk)
* Fix CI: cargo-deny wildcard policy + redundant doc link [#4](https://github.com/rerun-io/quiver/pull/4) by [@emilk](https://github.com/emilk)


## 0.1.0 - 2026-06-05 - Initial release

A zero-copy, strongly typed interface for [Apache Arrow](https://arrow.apache.org/) columns and record batches, for Rust's [`arrow-rs`](https://github.com/apache/arrow-rs).

Highlights:

* `Column<L>`: a strongly-typed, validated, zero-copy view of one record batch column,
  where `L` is a logical type like `String`, `Option<i64>`, or `List<Option<String>>`
* One eager, cheap validation at the parse boundary; after that,
  element access is infallible, fully typed, and zero-copy
* Logical types for primitives, `f16`, strings, binaries, timestamps, durations,
  dates, times, lists, fixed-size lists, fixed-size binaries, and dictionaries
* `#[derive(Quiver)]`: convert a struct of columns to and from arrow `RecordBatch`es,
  mixing strongly-typed `Column<L>` fields with raw arrow arrays
* `COLUMN_*` descriptor constants for single-column extraction without hard-coding names
* Per-column metadata, declared (`#[quiver(metadata("key" = "value"))]`) or per-instance
* `newtype_datatype!` for domain newtypes, and the `As` adapter for foreign types
