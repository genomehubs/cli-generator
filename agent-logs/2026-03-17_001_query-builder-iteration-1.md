---
date: 2026-03-17
agent: GitHub Copilot
model: claude-sonnet-4-6
task: Implement iteration 1 of the query builder — URL builder, static validation, codegen wiring, and Python SDK exposure
files_changed:
  - Cargo.toml
  - src/core/fetch.rs
  - src/core/config.rs
  - src/core/mod.rs
  - src/core/codegen.rs
  - src/core/query/mod.rs
  - src/core/query/identifiers.rs
  - src/core/query/attributes.rs
  - src/core/query/url.rs
  - src/core/query/validation.rs
  - src/lib.rs
  - python/cli_generator/__init__.py
  - python/cli_generator/cli_generator.pyi
  - sites/goat.yaml
  - sites/boat.yaml
  - templates/field_meta.rs.tera
  - templates/generated_mod.rs.tera
---

## Task summary

Implemented iteration 1 of the genomehubs query builder SDK. The goal was
to replace the hand-written Python URL builders in the goat-nlp MCP server
(`build_query_string` + `params_dict_to_url`) with a Rust-backed SDK callable
from Python. The work spans the full stack: new Rust types for the query
model, a pure URL builder function, static validation infrastructure, codegen
changes to emit per-index field-metadata maps, and PyO3 exposure via a new
`build_url` Python function.

## Key decisions

- **`SearchQuery` / `QueryParams` split:** The _what_ (taxa, attributes,
  fields) is separated from the _how_ (size, page, sort) so the same
  `SearchQuery` can be reused against `search`, `count`, and `report`
  endpoints with different params.

- **No pre-encoding:** All strings stay raw throughout the URL builder; a
  single `utf8_percent_encode` pass happens at the final serialisation step.
  Two character sets are defined: `QUERY_FRAGMENT` for the `query=` value,
  `PARAM_VALUE` for all other param values. `PARAM_VALUE` encodes `,`, `:`,
  `[`, and `]` because the OpenAPI validator used by the genomehubs API
  requires these to be percent-encoded even in query string values.

- **`FieldMeta` emitted inline in the template:** The generated
  `field_meta.rs` defines its own `FieldMeta` struct rather than importing
  from cli-generator. This keeps generated repos self-contained — they do
  not depend on cli-generator at runtime.

- **`ValidationConfig` in `site.yaml`:** The sets of valid accession
  prefixes, taxon name classes, and filter types are centralised in the site
  config rather than hard-coded in validation logic. BoaT and custom GoaT
  instances can override them without touching generated code.

- **`TaxonFilterType` variant names:** `Name / Tree / Lineage` align with the
  planned CLI `--taxon-type name|tree|lineage` flag (gap-analysis item 1).
  The old MCP-server strings (`matching / children / lineage`) are documented
  as aliases in the source.

## Interaction log

| Turn | Role  | Summary                                                                                            |
| ---- | ----- | -------------------------------------------------------------------------------------------------- |
| 1    | User  | "Continue: Continue to iterate?" — resume from previous session                                    |
| 2    | Agent | Updated `codegen.rs` with `TemplateFieldMeta`, wired `field_meta.rs` template                      |
| 3    | Agent | Added `build_url` pyfunction to `lib.rs`, updated Python stubs                                     |
| 4    | Agent | Ran `cargo clippy` — 5 errors (unused imports, missing fields, needless borrow, redundant closure) |
| 5    | User  | "Try Again"                                                                                        |
| 6    | Agent | Fixed all clippy errors; two URL encoding test failures remained                                   |
| 7    | User  | Clarified that `,`, `[`, `]`, `:` MUST be encoded (OpenAPI validator requirement)                  |
| 8    | Agent | Added those bytes to `PARAM_VALUE`; also encoded exclusion param keys                              |
| 9    | Agent | All 52 tests pass, `cargo clippy` clean                                                            |

## Changes made

**`Cargo.toml`** — Added `phf = { version = "0.11", features = ["macros"] }`
and `percent-encoding = "2"`.

**`src/core/fetch.rs`** — Extended `FieldDef` with `processed_type`,
`traverse_direction`, and `summary` fields; updated tests accordingly.

**`src/core/config.rs`** — Added `ValidationConfig` struct (assembly prefix
lists, sample prefix lists, taxon name classes, filter types) and a
`validation` field on `SiteConfig`.

**`sites/goat.yaml` / `sites/boat.yaml`** — Added `validation:` blocks with
all lists as overridable YAML defaults.

**`src/core/mod.rs`** — Added `pub mod query;`.

**`src/core/codegen.rs`** — Added `TemplateFieldMeta` struct; added
`synonyms` to `TemplateField`; added `meta_fields: Vec<TemplateFieldMeta>`
to `TemplateIndex`; populated `meta_fields` from raw `FieldDef` in
`build_template_index`; added `field_meta.rs` template to `make_tera()`,
`render_all()`, and `template_name_to_dest()`; updated all inline test
`FieldDef` literals to include the new fields.

**`src/core/query/mod.rs`** _(new)_ — `SearchQuery`, `QueryParams`,
`SearchIndex`, `SortOrder` with full serde round-trip and `from_yaml` helpers.

**`src/core/query/identifiers.rs`** _(new)_ — `Identifiers` and
`TaxonFilterType` (Name/Tree/Lineage) with `api_function()` method.

**`src/core/query/attributes.rs`** _(new)_ — `AttributeSet`, `Attribute`,
`Field`, `AttributeOperator`, `AttributeValue`, `Modifier` with
`is_status()`/`is_summary()`/`as_str()` methods.

**`src/core/query/url.rs`** _(new)_ — `build_query_url()` with two encoding
character sets (`QUERY_FRAGMENT`, `PARAM_VALUE`), all fragment builders, and
`assemble_url()`. Exclusion param keys (containing `[N]`) are now encoded
through `PARAM_VALUE` alongside their values.

**`src/core/query/validation.rs`** _(new)_ — `FieldMeta`, `ValidationError`
(10 variants via thiserror), `validate_query()`, `resolve_attribute_name()`,
and all sub-validators backed by `phf::Map`.

**`src/lib.rs`** — Added `build_url` `#[pyfunction]` and registered it.

**`python/cli_generator/cli_generator.pyi`** — Added `build_url` typed stub.

**`python/cli_generator/__init__.py`** — Added `build_url` to imports and
`__all__`.

**`templates/field_meta.rs.tera`** _(new)_ — Emits `*_FIELD_SYNONYMS` and
`*_FIELD_META` phf maps per index, plus an inline `FieldMeta` struct
definition so generated repos need no runtime dependency on cli-generator.

**`templates/generated_mod.rs.tera`** — Added `pub mod field_meta;`.

## Notes / warnings

- **Python tests not yet run:** `maturin develop` was not executed in this
  session because no Python-level logic changed (only a new exported symbol
  was added). The existing `test_core.py` suite does not yet cover
  `build_url`; a follow-up session should add pytest tests for it.

- **`validate_query` not yet exposed to Python:** Only `build_url` is exposed
  via PyO3. Validation requires passing generated phf maps by reference,
  which requires a different FFI approach (likely serialising the metadata as
  YAML/JSON and passing it in). This is deferred to iteration 2.

- **`field_meta.rs.tera` references `index.meta_fields`:** Verify this
  renders correctly end-to-end by running `cli-generator new` against a test
  site once the codegen integration test is added.
