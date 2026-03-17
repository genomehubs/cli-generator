---
date: 2026-03-17
agent: GitHub Copilot
model: claude-sonnet-4-6
task: Implement query-builder iterations 2 and 3 — Validator pyclass, site-specific QueryBuilder subclass, generated lib.rs template, type stubs, and generated-repo dependency wiring
files_changed:
  - src/core/query/url.rs
  - src/core/codegen.rs
  - src/commands/new.rs
  - python/cli_generator/__init__.py
  - python/cli_generator/query.py
  - tests/python/test_core.py
  - templates/sdk.rs.tera
  - templates/query.py.tera
  - templates/lib.rs.tera
  - templates/site_cli.pyi.tera
  - templates/generated_mod.rs.tera
  - templates/cli_meta.rs.tera
---

## Task summary

Continued from iteration 1 (session 2026-03-17_001). That session produced the
URL builder and static validation modules. This session implemented:

**Iteration 2** — Python-facing API layer: a `Validator` PyO3 class (baked-in
field metadata, exposes `validate`/`field_info`/`field_names`), a base
`QueryBuilder` Python class (state accumulation, YAML serialisation,
`merge`/`combine` for parallel MCP tool composition), a site-specific
`QueryBuilder` subclass template (`query.py.tera`) that delegates to the Rust
extension, and a `sdk.rs.tera` template generating `build_url`/`search`/`count`
Rust functions.

**Iteration 3** — Generated-repo wiring: resolved the compile blocker where
`sdk.rs.tera` imports `cli_generator::core::query::...` by injecting
`cli-generator` as a git dependency into generated `Cargo.toml`. Turned `lib.rs`
into a Tera template (`lib.rs.tera`) so that PyO3 module registration is
generated rather than hand-written. Added `site_cli.pyi.tera` for type stubs in
generated repos. Added `patch_pyproject_toml` to inject `pyyaml>=6.0` into
generated `pyproject.toml` (required by `QueryBuilder` YAML round-trip).

## Key decisions

- **Validator with baked-in metadata (Option B over Option A):** The `Validator`
  pyclass holds a `phf::Map` of `FieldInfo` structs compiled into the binary,
  avoiding a runtime YAML load and making validation callable without file I/O.

- **cli-generator as git dep (Option A over Option C — separate crate):** A
  dedicated `cli-query` crate would be the right long-term home for the shared
  query types, but premature at this stage. Revisit when a second consumer
  exists.

- **`lib.rs` promoted to a template:** Generated repos need PyO3 module
  registration that varies by site name. Moving this into `lib.rs.tera` keeps
  the pattern consistent and avoids manual post-generation edits.

- **`merge`/`combine` on `QueryBuilder`:** MCP servers often build query
  components in parallel (identifiers in one tool, attributes in another).
  `merge` extends lists and overwrites scalars only when the incoming value is
  non-default; `combine` is its classmethod equivalent.

- **URL encoding:** `,`, `:`, `[`, `]` added to `PARAM_VALUE` AsciiSet because
  the OpenAPI validator used in the GoaT API requires these encoded in query
  parameter values. Exclusion param keys (e.g. `excludeDirect[0]`) are also
  encoded through `encode_param` rather than written verbatim.

## Interaction log

| Turn | Role  | Summary                                                            |
| ---- | ----- | ------------------------------------------------------------------ |
| 1    | User  | Confirmed iteration 2 decisions (Validator, QueryBuilder, merge)   |
| 2    | Agent | Implemented sdk.rs.tera, query.py.tera, QueryBuilder base class    |
| 3    | User  | formatter had mangled sdk.rs.tera and query.py.tera — rewrite both |
| 4    | Agent | Rewrote both templates from scratch; 15/15 Python tests pass       |
| 5    | User  | Confirmed iteration 3 plan (git dep, lib.rs.tera, stubs, pyyaml)   |
| 6    | Agent | Injected cli-generator dep, created lib.rs.tera, site_cli.pyi.tera |
| 7    | Agent | Wired new templates into codegen.rs; added patch_pyproject_toml    |
| 8    | Agent | Fixed pyright unused-import errors; 53 Rust + 15 Python tests pass |

## Changes made

### `src/core/query/url.rs`

Extended `PARAM_VALUE` AsciiSet with `,`, `:`, `[`, `]`. Changed exclusion
param key encoding to use `encode_param(&ep.key)` instead of `ep.key` verbatim.

### `src/core/codegen.rs`

Added `field_meta.rs`, `sdk.rs`, `lib.rs`, `query.py`, `site_cli.pyi` to
`make_tera()` template loading and `render_all()` dispatch. Extended
`template_name_to_dest()` with mappings for the new templates. The function
now takes a `site_name: &str` argument for path interpolation.

### `src/commands/new.rs`

`inject_generated_deps()` now also injects `phf = { version = "0.11", features = ["macros"] }` and `cli-generator = { git = "https://github.com/genomehubs/cli-generator" }`.
Added `patch_pyproject_toml(repo_dir)` which idempotently injects `pyyaml>=6.0`
into the generated repo's `[project.optional-dependencies]` dev list.

### `templates/sdk.rs.tera` _(new)_

Generates `src/generated/sdk.rs`. Contains `build_url`/`search`/`count` Rust
functions exposed as PyO3 functions, and `Validator`/`FieldInfo` PyO3 classes
with baked-in `phf::Map` field metadata per index.

### `templates/lib.rs.tera` _(new)_

Generates `src/lib.rs` (not under `src/generated/`). Registers the PyO3 module
`{site_name}_cli` and re-exports all items from `generated::sdk`.

### `templates/site_cli.pyi.tera` _(new)_

Generates `python/{site_name}/{site_name}_cli.pyi`. Provides type stubs for
`build_url`, `search`, `count`, `FieldInfo`, and `Validator`.

### `templates/query.py.tera` _(rewritten)_

Generates `python/{site_name}/query.py`. Site-specific `QueryBuilder` subclass
that calls Rust extension functions for `to_url`, `search`, `count`, `validate`,
`field_names`, and `field_info`.

### `python/cli_generator/query.py`

Added `merge(other)` and `combine(*builders)` classmethods. `merge` extends
list fields and overwrites scalar fields only when the incoming value differs
from the default. `combine` creates a new builder by merging all provided
builders in sequence.

### `tests/python/test_core.py`

Removed unused `given`/`st` imports (caused pyright `reportUnusedImport` errors).
Tests now cover: 4 version, 3 build_url, 4 QueryBuilder, 4 merge/combine = 15 total.

## Notes / warnings

- **`sdk.rs.tera` depends on `cli-generator` as a Cargo dep** in the generated
  repo. The git dep points to the `main` branch — a future release should pin
  to a tag or use a published crate version (tracked as Option C in session
  notes).

- **Formatter must not be run on `.tera` files.** The Tera block syntax
  `{%- ... -%}` and indentation inside `{% for %}` loops is intentional and
  will be mangled by `rustfmt` or other formatters configured on the workspace.
  Consider adding `*.tera` to `.editorconfig` formatter exclusions.

- **Integration test not yet written.** A test that calls `render_all` on a
  minimal synthetic site config and verifies the rendered file map is the
  correct next step.

- **`conftest.py` hypothesis profile** is set to `dev` (max_examples=50). Add
  `ci` profile with higher examples when this project gets CI.
