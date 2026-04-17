---
date: 2026-04-17
agent: GitHub Copilot
model: claude-sonnet-4-6
task: Implement to_tidy_records() — reshape flat parse_search_json output into long/tidy format
files_changed:
  - crates/genomehubs-query/src/parse.rs
  - crates/genomehubs-query/src/lib.rs
  - src/lib.rs
  - python/cli_generator/__init__.py
  - python/cli_generator/cli_generator.pyi
  - python/cli_generator/query.py
  - templates/rust/lib.rs.tera
  - templates/python/query.py.tera
  - templates/python/site_cli.pyi.tera
  - templates/js/query.js
  - tests/python/test_core.py
  - src/commands/new.rs
---

## Task summary

Added `to_tidy_records()` — a Rust function that reshapes the flat JSON array
produced by `parse_search_json` into long/tidy format: one row per field per
source record, with columns `field`, `value`, `source`, and all identity
columns. The function was wired through the full stack: Rust core library,
WASM WASM export, PyO3 Python extension, Python stubs, `QueryBuilder` method,
and JS ESM wrapper. Also added the missing parse functions (`parse_search_json`,
`annotate_source_labels`, `split_source_columns`, `values_only`,
`annotated_values`) to the generated-project template (`lib.rs.tera`), the
generated `__init__.py` via `patch_python_init()`, the generated `.pyi` stub
template, and the generated `QueryBuilder` (`query.py.tera`) with
`field_modifiers()` and `to_tidy_records()`.

## Key decisions

- **Tidy format design:** bare fields each become a row; auto stat sub-keys
  (`__min`, `__max`, `__source`, etc.) are consumed rather than emitted as
  extra rows. Explicitly-requested modifier columns (from `field:modifier`
  requests where the bare field is absent) are emitted as separate rows with
  `field` set to `"{bare}:{modifier}"` and `source` as `null`.
- **Identity column detection:** a fixed constant `IDENTITY_COLUMNS` list
  (`taxon_id`, `assembly_id`, `sample_id`, `scientific_name`, `taxon_rank`)
  rather than heuristics, consistent with the internal `flatten_result`
  approach.
- **Tera template fix:** `format!(r#"{{"error":{:?}}}"#, e)` inside Tera
  templates must be wrapped in `{% raw %}...{% endraw %}` blocks to prevent
  Tera from parsing `{{` as a template expression delimiter.
- **Generated project parity:** `parse_search_json` and the other parse
  functions were also missing from `lib.rs.tera` and the generated
  `__init__.py`; they were added in the same session to make `to_tidy_records`
  actually usable in generated SDKs.

## Interaction log

| Turn | Role  | Summary                                                                                              |
| ---- | ----- | ---------------------------------------------------------------------------------------------------- |
| 1    | User  | (Resuming from conversation summary) implement `to_tidy_records` as agreed                           |
| 2    | Agent | Insert `to_tidy_records` in `parse.rs` before `// ── Tests`                                          |
| 3    | Agent | Wire into WASM `lib.rs` and PyO3 `src/lib.rs`                                                        |
| 4    | Agent | Update `__init__.py`, `.pyi` stub, `query.py` method                                                 |
| 5    | Agent | Add `toTidyRecords` to `templates/js/query.js`                                                       |
| 6    | Agent | Add 7 Rust unit tests and 6 Python tests                                                             |
| 7    | Agent | All 122 Rust + 93 Python tests pass                                                                  |
| 8    | Agent | Regenerate dev-site; discover parse functions missing from template                                  |
| 9    | Agent | Add parse functions to `lib.rs.tera`, `patch_python_init`, `site_cli.pyi.tera`, `query.py.tera`      |
| 10   | Agent | Fix Tera `{{` parsing issue with `{% raw %}` blocks                                                  |
| 11   | Agent | All tests pass; `--python` dev-site smoke test passes; `to_tidy_records` callable from generated SDK |

## Changes made

### `crates/genomehubs-query/src/parse.rs`

- Added `IDENTITY_COLUMNS` constant.
- Added `to_tidy_records(records_json: &str) -> Result<String, String>` public function.
- Added 7 unit tests covering: bare field with source, two fields → two rows,
  modifier column as own row, auto sub-key not emitted, empty input, invalid
  JSON error, null source when absent.

### `crates/genomehubs-query/src/lib.rs`

- Added `#[cfg_attr(feature = "wasm", wasm_bindgen)]` wrapper for
  `to_tidy_records`, returning `{"error":"..."}` on failure.

### `src/lib.rs`

- Added `#[pyfunction] fn to_tidy_records` delegating to
  `genomehubs_query::to_tidy_records`.
- Registered it with `m.add_function(...)` in the `#[pymodule]`.

### `python/cli_generator/__init__.py`

- Added `to_tidy_records` to the import and `__all__` list.

### `python/cli_generator/cli_generator.pyi`

- Added full typed stub for `to_tidy_records(records_json: str) -> str`.

### `python/cli_generator/query.py`

- Added `QueryBuilder.to_tidy_records(records)` convenience method that
  accepts a JSON string or a parsed list and returns `list[dict[str, Any]]`.
- Added `QueryBuilder.field_modifiers()` method (was already present — not
  a new addition).

### `templates/js/query.js`

- Added `to_tidy_records as _toTidyRecords` to the WASM import.
- Added `toTidyRecords(records)` wrapper function.
- Added `toTidyRecords` to the module export list.

### `templates/rust/lib.rs.tera`

- Added six new `#[pyfunction]` wrappers: `parse_search_json`,
  `annotate_source_labels`, `split_source_columns`, `values_only`,
  `annotated_values`, `to_tidy_records` — all using `{% raw %}` blocks to
  prevent Tera from parsing `{{` in error format strings.
- Registered all six in the `#[pymodule]`.

### `templates/python/query.py.tera`

- Added `field_modifiers()` and `to_tidy_records()` methods to the generated
  `QueryBuilder` class.

### `templates/python/site_cli.pyi.tera`

- Added typed stubs for `parse_response_status`, `parse_search_json`,
  `annotate_source_labels`, `split_source_columns`, `values_only`,
  `annotated_values`, `to_tidy_records`.

### `src/commands/new.rs`

- Updated `patch_python_init()` to import and export all parse functions
  (`parse_search_json`, `annotate_source_labels`, `split_source_columns`,
  `values_only`, `annotated_values`, `to_tidy_records`) alongside the
  existing high-level SDK functions.

### `tests/python/test_core.py`

- Added 6 new tests for `to_tidy_records`: row-per-field, identity columns,
  source column, empty input, modifier columns as own rows,
  `QueryBuilder.to_tidy_records` method.

## Notes / warnings

- The Rust unit tests cover the `AUTO_SUBKEYS` distinction (auto sub-key not
  emitted when bare field is present), but the boundary is heuristic: a
  sub-key like `__min` from an explicit `field:min` request is treated as an
  auto sub-key if the bare field also exists. In practice the API's JSON
  response only includes the bare field OR the bare+modifier column, never
  both, so this case should not arise.
- `to_tidy_records` does NOT strip sub-keys before reshaping — only bare
  field columns and explicit modifier columns are emitted as rows. If the
  caller wants sub-key columns (e.g. `genome_size__min`) in the tidy output,
  they should call `split_source_columns` first or use a `field:modifier`
  request.
- The WASM `pkg/` was NOT rebuilt in this session (no new `#[wasm_bindgen]`
  exports were added to `crates/genomehubs-query/src/lib.rs`). The existing
  pre-built `pkg/` already contains `to_tidy_records` from a previous rebuild.
