---
date: 2026-04-15
agent: GitHub Copilot
model: claude-sonnet-4-6
task: Fix generated goat-cli so describe() and snippet() work end-to-end; complete Python snippet integration
files_changed:
  - src/commands/new.rs
  - templates/rust/lib.rs.tera
  - templates/python/query.py.tera
  - tests/generated_goat_cli.rs
---

## Task summary

Resumed a session that had been summarised mid-implementation. The final blocking bug
was that generated projects' `lib.rs.tera` template did not register `describe_query` or
`render_snippet` PyO3 functions, so neither feature was accessible in generated SDKs.
Additionally, `core/snippet.rs` was not embedded into generated projects, `tera` was
missing from injected Cargo.toml dependencies, the snippet template files were not
copied, and the template `query.py.tera` had a `NameError` in `describe()` due to a
missing `field_metadata` parameter.

## Key decisions

- **Copy snippet Tera templates into `src/templates/snippets/`**: The `include_str!` in
  `snippet.rs` uses a path relative to the source file. Rather than rewriting the path
  substitution logic, we copy the `.tera` files to the location the macro expects in the
  generated project hierarchy.
- **Remove `version` from generated `__init__.py`**: The generated SDK extension does
  not export a `version` function (unlike cli-generator itself), so importing it caused
  an `ImportError` at runtime. Removed it from the generated `__init__.py`.
- **Replaced obsolete `cli_generator_git_dep_injected_in_cargo_toml` test**: Generated
  projects are now self-contained (embedded modules), not git-dependent. Replaced with
  `tera_dep_injected_in_cargo_toml` which asserts the newly injected dep is present.

## Interaction log

| Turn | Role  | Summary                                                                    |
| ---- | ----- | -------------------------------------------------------------------------- |
| 1    | Agent | Resumed from summary; applied 4 pre-planned changes in multi_replace call  |
| 2    | Agent | Discovered snippet.rs + tera + core/mod.rs changes were already in place   |
| 3    | Agent | Built; found `include_str!` path error — snippet templates not copied      |
| 4    | Agent | Added `copy_snippet_templates()` logic in `copy_embedded_modules()`        |
| 5    | Agent | Built; found `version` ImportError — removed from generated `__init__.py`  |
| 6    | Agent | Built; found `field_metadata` NameError in template `describe()` method    |
| 7    | Agent | Added `field_metadata` param to `describe()` in `query.py.tera`            |
| 8    | Agent | End-to-end test passed: both `describe()` and `snippet()` work in goat-cli |

## Changes made

### `templates/rust/lib.rs.tera`

Added `describe_query` and `render_snippet` PyO3 functions (mirroring `src/lib.rs`) with
`crate::embedded::core::` prefixes. Both registered in the `#[pymodule]` block.
Added `use pyo3::exceptions::{PyRuntimeError, PyValueError}` and `use std::collections::HashMap`.

### `src/commands/new.rs`

- `required_deps`: added `("tera", "tera = { version = \"1\", default-features = false }")` so
  generated Cargo.toml gets the tera dependency (needed by snippet engine).
- `copy_embedded_modules()`: added a block that copies `.tera` files from
  `templates/snippets/` to `src/templates/snippets/` in the generated project — required
  because `include_str!("../../templates/snippets/python_snippet.tera")` in the embedded
  `snippet.rs` resolves relative to that file's location.
- `patch_python_init()`: removed `version` from imports and `__all__` — the generated
  SDK extension does not export it.

### `templates/python/query.py.tera`

- `describe()`: added missing `field_metadata: dict[str, Any] | None = None` parameter.
  Without it `field_metadata or {}` raised a `NameError` at runtime.

### `tests/generated_goat_cli.rs`

- Replaced `cli_generator_git_dep_injected_in_cargo_toml` with
  `tera_dep_injected_in_cargo_toml` — old test asserted a git dependency that no longer
  applies; new test verifies `tera` is present in the generated `Cargo.toml`.

## Notes / warnings

- `isort` is not installed in the `dev11` conda environment; the `cargo test` harness
  warns but does not fail. Install with `conda run -n dev11 pip install isort` if needed.
- R and JavaScript SDKs are the next planned phases. `query.py.tera` already has
  `snippet()` for those languages; the Rust `SnippetGenerator` will need corresponding
  `.tera` template files added to `templates/snippets/` before they will work.
- The `snippet()` method in `query.py.tera` uses hardcoded defaults `site_name="site"`,
  `sdk_name="sdk"`. Generated projects should pass real values from their site config;
  this is left as a follow-up.
