# `quiver` changelog

All notable changes to the `quiver` crates will be documented in this file.

This file is updated upon each release by `./scripts/generate_changelog.py`.
Do NOT add entries here manually — they are generated from PR titles and labels.


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
