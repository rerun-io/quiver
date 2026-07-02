# `quiver` changelog

All notable changes to the `quiver` crates will be documented in this file.

This file is updated upon each release by `./scripts/generate_changelog.py`.
Do NOT add entries here manually — they are generated from PR titles and labels.


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
