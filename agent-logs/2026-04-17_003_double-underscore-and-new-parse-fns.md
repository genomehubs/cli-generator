# 2026-04-17_003 — Double-underscore sub-key namespace + values_only / annotated_values

## Summary

Completed the double-underscore sub-key rename that was in-progress when the
previous session hit its token budget, then added two new convenience parse
functions and wired them through the full stack.

---

## Changes made

### `crates/genomehubs-query/src/parse.rs`

**Double-underscore rename (carried over from previous session):**

Fixed two test assertions that had been missed by the prior rename pass:

- `parses_taxon_numeric_direct_field`: `genome_size_median`, `genome_size_count`,
  `genome_size_sp_count` → `genome_size__median`, `genome_size__count`,
  `genome_size__sp_count`
- `parses_taxon_ancestor_source_as_string`: `ploidy_source`, `ploidy_sp_count`
  → `ploidy__source`, `ploidy__sp_count`

**New functions:**

`values_only(records_json: &str) -> Result<String, String>`

- Accepts flat JSON from `parse_search_json`
- Strips any column whose name contains `__` (source, stats, labels, split cols)
- Returns identity columns + bare `{field}` values only

`annotated_values(records_json: &str, mode: &str) -> Result<String, String>`

- Chains `annotate_source_labels(mode)` then promotes each `{field}__label`
  into `{field}`, then strips all remaining `__*` columns
- Result: clean rows with labelled strings for non-direct values, numerics
  for direct values (in `non_direct` mode)

**New tests (5):** `values_only_strips_subkey_columns`,
`values_only_on_empty_records_returns_empty_array`,
`annotated_values_labels_ancestral_field`,
`annotated_values_keeps_direct_value_numeric`,
`annotated_values_on_empty_records_returns_empty_array`

### `crates/genomehubs-query/src/lib.rs` (WASM)

Added `#[cfg_attr(feature = "wasm", wasm_bindgen)]` exports for:

- `values_only`
- `annotated_values`

Updated stale doc comment: `{field}_source` / `{field}_direct` →
`{field}__source` / `{field}__direct`.

### `src/lib.rs` (PyO3)

Added `#[pyfunction]` wrappers for `values_only` and `annotated_values`, both
delegating to `genomehubs_query::*`. Registered both in `#[pymodule]`.

### `python/cli_generator/__init__.py`

Added `values_only` and `annotated_values` to the extension import and `__all__`.

### `python/cli_generator/cli_generator.pyi`

- Updated existing stubs to use `__` column names throughout
- Added full typed stubs for `values_only` and `annotated_values`

### `templates/js/query.js`

- Added `values_only` and `annotated_values` to the WASM import block
- Added `valuesOnly(records)` and `annotatedValues(records, mode="non_direct")`
  wrapper functions (accept string or object, return parsed JS array)
- Added both to the `export { ... }` block
- Fixed `annotateSourceLabels` doc comment: `{field}_label` → `{field}__label`

### `crates/genomehubs-query/pkg/`

Rebuilt with `wasm-pack` (via `dev_site.sh --rebuild-wasm`). The updated ESM
`pkg/genomehubs_query.js` now exports all 7 functions:
`build_url`, `parse_response_status`, `parse_search_json`,
`annotate_source_labels`, `split_source_columns`, `values_only`,
`annotated_values`.

### `tests/python/test_core.py`

Added 4 integration tests for the new Python bindings:

- `test_parse_search_json_returns_flat_record` — baseline double-underscore shape
- `test_values_only_strips_subkey_columns`
- `test_annotated_values_direct_stays_numeric_in_non_direct_mode`
- `test_annotated_values_ancestor_becomes_labelled_string`

---

## Verification

```
cargo test -p genomehubs-query   → 109 passed, 0 failed  (+ 3 doc tests)
cargo clippy --all-targets       → 0 warnings
maturin develop                  → built successfully
pytest tests/python/             → 84 passed
bash scripts/dev_site.sh         → Rust + JS smoke tests pass
```

---

## Design decisions

- `values_only` uses a simple `!k.contains("__")` filter — this matches all
  sub-keys added by this crate while being immune to future stat additions.
- `annotated_values` collects `__label`-suffixed keys first, promotes them to
  bare field names, then strips the rest in a second pass — avoids any
  borrow-checker conflicts from mutating a map while iterating it.
- Both functions take `records_json: &str` rather than a parsed `Vec<…>` so
  they have the same FFI-friendly signature as the other parse functions.
