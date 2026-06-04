# arrow-quiver

Wanted: a schema specification and validator for arrow, with codegen for integrating with `arrow-rs` (Rust)

Working name: **`arrow-quiver`** (a quiver holds arrows; this struct holds typed Arrow arrays).
`quiver` itself is taken on crates.io by a dead crate (v0.0.1, 2018, owned by Andy Grove of DataFusion fame) — reach out and ask for a transfer if this project becomes good. Free fallbacks: `fletching`, `quivers`, `quiver-arrow`, `batchema`, `arrow-record`.


### Proposal

In a perfect world, each record batch would have a clear definition that is then leveraged to automate construction and parsing of the record batches.

For an example of a (manually written!) strongly typed wrapper, see struct RawRrdManifest and struct RrdManifest

Similar to pydantic: we need a arrow-schema definition, with code-gen for Rust. This is general for the entire arrow eco-system. Does it already exist? If not, we could make it.

Key goal: **zero-copy parsing**. Keep structure-of-arrays — downcast columns to typed arrays (`StringArray` etc.), NEVER transpose into `Vec<RowStruct>`.

### Prior art (researched 2026-06-04)

**Conclusion: nothing fills this niche, in any language.** The Arrow maintainers confirm the gap: when asked "where's the protoc/flatc for Arrow schemas?" (apache/arrow discussion #47530), maintainer lidavidm confirmed no comprehensive tool exists. There is also no official Arrow IDL / textual schema format (apache/arrow#25078, open, PR unmerged).

#### Rust

| Crate            | Status            | What it does                                                              | Zero-copy SoA?                            |
|------------------|-------------------|---------------------------------------------------------------------------|-------------------------------------------|
| `typed-arrow`    | Active (tonbo-io) | `#[derive(Record)]` on *logical* types → builders, schema, lazy row views | **Yes** (`views` feature)                  |
| `arrow_convert`  | Active            | serde-style derive, Rust types ↔ Arrow arrays                             | No — transposes + copies into `Vec<T>`     |
| `serde_arrow`    | Very active       | `Vec<Struct>` ↔ RecordBatch via serde                                     | No — serde data model forces owned values  |
| `arrow2-convert` | Dead (2023)       | Predecessor of `arrow_convert`, targets legacy arrow2                     | No                                         |

`typed-arrow` is the closest match but misses the mold:

1. Positional column matching (index + datatype), not name-based. No `optional` columns, no `allow_extra_fields`.
2. Nullability validated lazily per-row, not eagerly at the parse boundary.
3. No metadata schema validation at all.
4. Schema declared as Rust *logical* types (`String`, `i64`); generates builder machinery we don't need. Our derive goes directly on *array* types (`StringArray`) — simpler, inherently zero-copy.

#### Python (validation half only, no Rust codegen)

* `pandera` — best-in-class declarative validation ergonomics, pyarrow support since v0.20. Good API inspiration.
* `dataframely` (Quantco) — Polars/Arrow-native declarative schemas, failure introspection.
* `patito` — pydantic + polars models.
* `pydantic-to-pyarrow` — one-way pydantic → pyarrow schema, not a validator.

### Layered design

**The runtime `Schema` is the engine; the proc-macro is sugar on top.** The macro generates `fn schema() -> Schema` plus a `TryFrom<RecordBatch>` that calls the runtime validator and then downcasts. This gives both dynamic validation (CLI tools, future Python bindings) and static typing from one codebase. An IDL becomes an optional third layer later, only if cross-language demand materializes — it would emit the same derive structs.

### Runtime schema
Whatever our approach, the schema should look similar to this:

```rs
struct Schema {
    name: String,
    docstring: String,
    metadata_schema: MetadataSchema,
    fields: Vec<FieldSchema>,
    allow_extra_fields: bool, // Are unknown fields ok, or an error?
}

struct FieldSchema {
    name: String,
    docstring: String,
    metadata_schema: MetadataSchema,
    optional: bool, // Is this field allowed to be missing?
    datatype: DatatypeSchema,
}

enum DatatypeSchema {
    /// Any datatype is accepted
    Any,

    /// Future work: allow "maybe nullable" etc.
    /// For starters: only support a handful of datatypes
    Specific(arrow::DataType),
}

struct MetadataSchema {
    required_fields: BTreeSet<String>,
    allow_extra_fields: bool, // Are unknown fields ok, or an error?
}
```

### procmacro-based

Design details that apply to either alternative:

1. **Two independent axes: column *presence* and inner (arrow) *nullability*.** `Option<T>` always means column presence: `Option<StringArray>` = the whole column may be missing. Inner nullability is a separate marker: `#[non_null]` checks `null_count == 0` eagerly at `try_from`. An absent column is NOT the same as an all-null column.
2. **No magic field names.** Detect `metadata`/`other_columns` via attributes (`#[record(metadata)]`, `#[record(extra_columns)]`), not by name or type — otherwise a real column named "metadata" breaks. Absence of an `extra_columns` field ⇒ `allow_extra_fields: false`.
3. **Column names ≠ Rust idents.** Support `#[field(name = "special:name")]`.
4. **Field-level metadata requirements** via attributes: `#[field(required_metadata("unit"))]`.
5. **`DatatypeSchema::Any`** ⇒ field type `ArrayRef`.
6. **Doc comments** are extracted into the generated `Schema` docstrings (and optionally into Arrow field metadata).

#### procmacro-based, alternative 1 (REJECTED)

``` rust
/// Important thing
#[derive(Record)]
struct Thing {
    /// All the data
    rb: RecordBatch,

    /// Name
    #[non_null]
    name: StringArray,

    /// Date of birth
    dob: Option<TimestampNanosecondArray>,
}

// Proc-macro generates:
// * `impl TryFrom<RecordBatch> for Thing`
// * `impl Into<RecordBatch> for Thing`
// * Read-only accessors to the members (read-only so that `Thing` cannot be put into an invalid state)
```

PRO: invariant by construction — holding a `Thing` means it is valid ("parse, don't validate").
PRO: `Into<RecordBatch>` trivially returns `rb`; extra columns/metadata/column order preserved for free.
CON: `rb` + downcast arrays = two sources of truth; the "rb columns == typed fields" invariant must hold forever.
CON: write path clunky — everything must go through a generated `new()`.

#### procmacro-based, alternative 2 (CHOSEN)

``` rust
/// Important thing
#[derive(Record)]
struct Thing {
    /// …of the record-batch
    pub metadata: BTreeMap<String, String>,

    /// Name
    #[non_null]
    pub name: StringArray,

    /// Date of birth
    pub dob: Option<TimestampNanosecondArray>,

    /// If missing, the proc-macro enforces no additional columns may exist
    pub other_columns: Vec<Column>,
}

// Proc-macro generates:
// * `fn schema() -> Schema` (the runtime schema above)
// * `impl TryFrom<RecordBatch> for Thing` - validates via the runtime schema, then downcasts
// * `impl TryInto<RecordBatch> for Thing` - fails on column length mismatch
```

PRO: struct literal = free builder, free pattern matching, simpler macro, simpler mental model.
PRO: extra columns + metadata are explicit fields, not hidden state.
CON: invalid states representable — a length mismatch is only caught at `try_into()`, far from the mistake site. Acceptable: length is the *only* such invariant, and it cannot be checked at field-assignment time anyway.
CON: pub fields mean a `Thing` can be mutated into invalidity after parsing. In practice batches are parse→read or build→emit, not long-lived mutable state. If the invariant matters for some type, add an opt-in `#[record(readonly)]` generating the alternative-1 style.

### IDL-based codegen (FUTURE, only if needed)
Could be something to consider for the future, and if we want to support more languages.

Codegen would emit the same `#[derive(Record)]` structs as the proc-macro layer. Worth emitting/consuming Arrow's FlatBuffers-JSON schema format for forward compatibility if apache/arrow#25078 ever lands.

```
/// Important thing
struct Thing {
    /// Name
    name: required nonnull String,

    /// Date of birth
    dob: optional nonnull TimestampNs,
}
```

``` rust
/// Important thing
struct Thing {
    /// All the data
    rb: RecordBatch,

    /// Name
    name: StringArray,

    /// Date of birth
    dob: Option<TimestampNanosecondArray>,
}

impl Thing {
    /// Validates the schema and extracts columns
    pub fn try_from(rb: RecordBatch) -> Result<Self> {
      …
    }

    /// Error if the lengths are wrong, or nullability is wrong.
    pub fn new(name: StringArray, dob: Option<TimestampNanosecondArray>)
      -> Result<Self>
    {
      …
    }


    /// Name
    pub fn name(&self) -> &StringArray { &self.name }

    /// Date of birth
    pub fn dob(&self) -> Option<&TimestampNanosecondArray> { self.dob.as_ref() }
}

impl Into<RecordBatch> for Thing { … }
```

### Strongly-typed array wrappers (DRAFT, 2026-06-04)

Problem: raw `arrow` array types stop at the first nesting level. A `ListArray` field is
validated by downcast only — the *inner* type is unchecked, and reading values means untyped
`ArrayRef` + manual downcasts. Same for nullability: `#[quiver(non_null)]` is an attribute,
invisible to the type system, and doesn't exist at all for inner values (`List` items etc.).

Idea: our own generic wrapper, parameterized by a *logical type* `L`:

``` rust
#[derive(Quiver)]
struct Thing {
    /// Required non-null column of `List<Utf8>`, with non-null items:
    many_strings: quiver::Array<List<String>>,

    /// Same datatype, but the items may be null:
    sparse_strings: quiver::Array<List<Option<String>>>,

    /// The *values* may be null:
    maybe_name: quiver::Array<Option<String>>,

    /// The *column* may be missing (column presence stays on the struct axis):
    dob: Option<quiver::Array<Timestamp<Nanosecond>>>,
}
```

``` rust
/// Validated-once typed view of one column. Zero-copy (wraps the downcast arrow array).
pub struct Array<L: Datatype> {
    array: L::ArrowArray,           // e.g. arrow::array::ListArray for List<T>
    _marker: PhantomData<fn() -> L>,
}

pub trait Datatype {
    /// The arrow array this logical type is stored in.
    type ArrowArray: arrow::array::Array + Clone + 'static;

    /// Zero-copy element view: `&'a str` for `String`, `i64` for `i64`,
    /// `ListValues<'a, T>` (an iterator) for `List<T>`, `Option<…>` for `Option<T>`.
    type Value<'a>;

    /// The exact arrow datatype, built recursively (incl. inner field nullability).
    fn datatype() -> arrow::datatypes::DataType;

    /// # Safety/contract: only called after validation; index < len.
    fn value(array: &Self::ArrowArray, index: usize) -> Self::Value<'_>;
}
```

* `Array::<L>::try_from(&ArrayRef)` validates **exactly once, eagerly**:
  datatype equality against the recursive `L::datatype()` (this finally validates inner types
  of `List`/`Struct`/…), plus `null_count == 0` at every non-`Option` nesting level.
  After that, `array.get(i)` / `array.iter()` are infallible and fully typed.
* Nullability moves from attribute into the type: non-`Option` ⇒ non-null, enforced at parse.
  `#[quiver(non_null)]` becomes redundant for wrapper columns.
  The two axes stay distinct: `Option<Array<T>>` = column may be missing (struct axis);
  `Array<Option<T>>` = values may be null (data axis).
* Timezones fall out for free: `Timestamp<Nanosecond, Utc>` marker types (like typed-arrow's
  `TimestampTz<U, Z>`), where the timezone is part of `L::datatype()`.
* Write path: `Array<L>` implements `FromIterator<L::Owned>` (builder under the hood) and
  validated `TryFrom<the arrow array>` for zero-copy wrapping of existing arrays.
* Interop escape hatch: `.as_arrow() -> &L::ArrowArray` and `.into_arrow()`.

Inspiration from `typed-arrow` (researched their source):
* `ArrowBinding` trait = Rust logical type → `{Builder, Array, DataType}`; recursive
  `data_type()` with item nullability from `Option`. We want the same recursion.
* `ArrowBindingView` (their `views` feature) = `type View<'a>` GAT + `get_view(array, i)`;
  `Option<T>`'s view is `Option<T::View>`. This is exactly our `Datatype::Value<'a>` —
  except typed-arrow's unit of reading is the *row* (generated `FooView<'a>` per record);
  ours is the *column*, which preserves the SoA/zero-copy goal (their row-first `List<T>`
  is an owned `Vec<T>` — copying, which we reject).
* Their `get_view` returns `Result` per element (null/type errors at access time).
  We validate at the parse boundary instead, so element access is infallible — cheaper
  inner loops, and errors surface where the data enters.

Alternatives considered:
1. `quiver::ListArray<String>` per-shape wrappers — less uniform; doesn't nest
   (`ListArray<ListArray<…>>`?), no single validation entry point. The generic
   `Array<List<String>>` subsumes it; per-shape aliases can be added for ergonomics.
2. Keep raw arrow arrays + declare inner types in attributes
   (`#[quiver(item = "Utf8")]`) — validates, but reading stays untyped; strings in
   attributes instead of types defeats the point.

Open questions:
* `Struct<T>` columns: needs a per-struct derive for typed field access (typed-arrow
  generates `{Name}View`). Phase 2; `Array<Struct>` could start as downcast-only.
* Do raw arrow array fields (`StringArray`) stay supported alongside wrappers? Probably
  yes — zero-friction interop — but wrappers become the documented default.
* Naming: `quiver::Array<L>` vs `Col<L>` vs `Column<L>` (clashes with existing
  `quiver::Column` extra-columns type).
* `Dictionary<K, V>`: typed keys matter less than typed values; start with values only?
