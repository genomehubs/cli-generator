---
date: 2026-05-08
agent: GitHub Copilot
model: claude-sonnet-4-6
task: Phase 6e/6f — Python SDK gaps and R/JS SDK parity with v3 transport
files_changed:
  - src/commands/new.rs
  - python/cli_generator/query.py
  - python/cli_generator/__init__.py
  - templates/python/query.py.tera
  - templates/r/query.R
  - templates/js/query.js
  - tests/python/test_sdk_parity.py
  - tests/python/test_batch_operations.py
  - tests/python/test_batch_integration.py
---

## Task summary

Two-part session. First fixed a generated CLI build failure (`crate::report` unresolved import in `validation.rs`) by adding broad `crate::` → `crate::embedded::core::` path rewriting in the embedded module copy step (`new.rs`). Then implemented Phase 6e Python SDK gaps: `probe_api_capability()`, `search_df`/`search_polars`, redesigned `record`/`lookup`/`summary` with explicit positional params, and `to_tidy_records(records=None)` auto-fetch. Finally implemented Phase 6f to bring R and JS templates to full parity with the Python SDK: v3 POST transport for `count`/`search`, `search_all` cursor loop, `to_v2_url`/`toV2Url` canonical method (with deprecated `to_url`/`toUrl` wrappers), and private field naming bug fixes in R.

## Key decisions

- **Broad `crate::` rewrite over targeted `use crate::` rewrite:** `validation.rs` used `crate::report::ReportType` in an inline expression, not just in a `use` import. A targeted `use crate::` replacement would have missed the inline reference. The broad replacement is correct because the subcrate files only ever refer to peer modules via `crate::`, and in the generated project those modules live at `crate::embedded::core::`.

- **`to_v2_url` as canonical, `to_url` as deprecated wrapper:** The v2 URL is the only query URL format (v3 uses POST bodies). Keeping `to_url` as a deprecated wrapper preserves backward compatibility for existing user code while making the naming unambiguous.

- **R private field names fixed:** Found and corrected 6 locations where R template used `private$.api_base`, `private$.api_version`, `private$.index` — the correct names are `private$api_base_url`, `private$api_version`, `private$index_name`. These were silent bugs causing runtime errors only when those methods were called.

- **`search_all` cursor loop (R and JS):** Uses the same cursor-based pagination as the Python template — fetches pages until the response contains no scroll cursor, restores original `size` via `on.exit`/`try/finally`, and accumulates all hits. This matches Python semantics.

- **`_postJson` extracted as JS helper:** Rather than duplicating the fetch + JSON + error-handling boilerplate in `count`, `search`, `report`, `searchBatch`, and `countBatch`, a private `_postJson(url, payload)` method centralises the v3 POST logic.

## Interaction log

1. Fixed embedded module `crate::` path rewriting in `src/commands/new.rs` (copy_embedded_modules).
2. Added `probe_api_capability()` module-level function to Python library and template.
3. Added `search_df` / `search_polars` DataFrame wrappers with `TYPE_CHECKING`-guarded type annotations.
4. Redesigned `record(record_id, ...)`, `lookup(search_term, ...)`, `summary(record_id, fields, ...)` with GET transport and explicit required positional params.
5. Made `to_tidy_records(records=None)` auto-call `search()` → `parse_search_json()` when records is `None`.
6. Updated test files to pass required positional args after signature change.
7. Fixed R private field naming bugs (6 locations).
8. Added `to_v2_url` + deprecated `to_url` wrapper to R template.
9. Migrated R `count` and `search` to v3 POST; added `search_all` cursor loop.
10. Added `toV2Url` + deprecated `toUrl` wrapper to JS template.
11. Added `_postJson` helper to JS template; migrated all v3 methods to use it.
12. Added `to_v2_url` and `search_all` entries to `CANONICAL_METHODS` in parity test.
13. `verify_code.sh` passed; `dev_site.sh --no-rebuild-wasm --python goat` passed.
