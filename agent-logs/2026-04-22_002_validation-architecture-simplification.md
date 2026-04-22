# 2026-04-22_002 — Validation architecture simplification

## Summary

Clarified and implemented the correct design for `validate()` across Python, R, and JavaScript SDKs: `validation_level`/`partial` mode was a JS-browser-specific concern and should not have been added to Python or R. Fixed the resulting broken validation, several R YAML serialization bugs, and a mismatched `field_meta.json` deployment path that prevented the R package from finding its field metadata at all.

---

## Changes

### Architecture change

- **`validation_level` / `partial` mode removed from Python and R** — these SDKs always have the local `generated/field_meta.json` file available; no API fetch or mode switching is needed
- **JS SDK retains `validationLevel` constructor option** — browser mode cannot read files, so it fetches from the API and falls back to structural-only validation if the fetch fails
- **Rust validator now skips field-name checks when `field_meta` is empty** — empty `{}` means "structural checks only"; no false "unknown attribute" errors when metadata is unavailable

### Files changed

| File | Change |
|------|--------|
| `crates/genomehubs-query/src/validation.rs` | Skip attribute/field name checks when `field_meta` is empty |
| `templates/python/query.py.tera` | Remove `validation_level` + `api_base` constructor params and all API-fetch logic; `validate()` just loads local file and validates |
| `templates/r/query.R` | Same removal; also fixed: `as.character(NULL)` → conditional value; `as.list(modifiers)` for single-element modifiers; `null = "null"` in `toJSON`; `simplifyDataFrame = FALSE` in `fromJSON` |
| `src/core/codegen.rs` | Fixed R `inst/generated/` path: was using `sdk_name` (`goat_sdk`) but R package dir uses `site.name` (`goat`) |
| `templates/r/extendr-wrappers.R.tera` | Added missing `validate_query_json` wrapper |
| `python/cli_generator/query.py` | Remove `validation_level` + `api_base` from constructor |
| `tests/python/test_sdk_parity.py` | Remove `TestValidationConfiguration` tests for `validation_level`; update `CANONICAL_METHODS` `validate` params; update `CONSTRUCTOR_PARAMS` |
| `tests/python/test_sdk_fixtures.py` | Fix `assembly_name` → `assembly_span` (not a real assembly field) |
| `tests/javascript/test_sdk_fixtures.mjs` | Same fixture fix |
| `tests/r/test_sdk_fixtures.R` | Same fixture fix |

---

## Bugs fixed

### R inst/generated/ path mismatch
`codegen.rs` was writing `r/{sdk_name}/inst/generated/field_meta.json` (e.g. `r/goat_sdk/`) but `create_r_package()` creates the directory as `r/{site.name}/` (e.g. `r/goat/`). The file was written to the wrong location and the R package could not find it.

### R JSON null serialization
R's `jsonlite::toJSON` serializes R `NULL` inside a named list as `{}` (empty object) by default, not `null`. The Rust `FieldMeta.constraint_enum: Option<Vec<String>>` then fails to parse `{}` as an `Option`. Fixed by adding `null = "null"` to all `toJSON` calls.

### R fromJSON data frame simplification
`fromJSON(path)` with default `simplifyDataFrame = TRUE` converts a dict of same-structure objects into a data frame. Re-serializing a data frame with `toJSON` produces a different JSON structure. Fixed by using `simplifyDataFrame = FALSE, simplifyMatrix = FALSE`.

### R attribute value serialization (`as.character(NULL)`)
`as.character(NULL)` produces `character(0)` (length-0 vector), which YAML serializes as an empty sequence `[]`. Rust then fails to parse `attributes[0].value` as `Option<String>`. Fixed by conditionally including `value` only when non-NULL.

### R modifier serialization (single-element vector)
A length-1 R character vector (e.g. `c("median")`) serializes as a YAML scalar `"median"`, not a sequence `["median"]`. Rust expects `Vec<String>` and fails. Fixed by wrapping with `as.list(modifiers)` in both `add_attribute` and `add_field`.

### Invalid test fixture field name
`assembly_index_with_filter` used `.add_field("assembly_name")` but `assembly_name` is not in the GoaT assembly `resultFields`. Field validation (now actually running) caught this. Fixed by replacing with `assembly_span`.

---

## Test results

All checks pass: `cargo fmt`, `cargo clippy`, `cargo test`, `black`, `isort`, `pyright`, `pytest` (414 tests), and fixture tests for all three SDKs (264 Python + 263 JavaScript + 26×6 R, all against 26 cached GoaT API fixtures).
