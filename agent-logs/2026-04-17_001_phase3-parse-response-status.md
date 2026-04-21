# 2026-04-17_001 — Phase 3: parse_response_status wiring

## Summary

Completed Phase 3 of the multi-language SDK plan: introduced a canonical
`parse_response_status` function in the `genomehubs-query` subcrate and wired
all `count()` implementations across Python, R, JS, and the generated Rust CLI
to use it.

## Changes

### `crates/genomehubs-query/src/parse.rs` (new, from previous session)

- `ResponseStatus` struct (`hits`, `ok`, `error`)
- `parse_response_status(raw: &str) -> Result<ResponseStatus, String>`
- `response_status_to_json(status: &ResponseStatus) -> String`
- 8 unit tests (corrected round-trip tests to check serialised fields rather
  than attempt a false parse round-trip)

### `crates/genomehubs-query/Cargo.toml`

- Added `[dev-dependencies] proptest = "1"` — this was missing, causing
  pre-existing proptest tests in `url.rs` to fail at compile time.

### `crates/genomehubs-query/src/query/url.rs`

- Fixed `use crate::core::query::` → `use crate::query::` in the test module
  (pre-existing wrong import path for the subcrate context).

### `src/commands/new.rs`

- `copy_embedded_modules()`: copies `crates/genomehubs-query/src/parse.rs`
  into `src/embedded/core/parse.rs` in generated projects.
- `core_mod_rs_content`: added `pub mod parse;` declaration.

### `templates/rust/lib.rs.tera`

- `parse_response_status` PyO3 function now calls
  `crate::embedded::core::parse::parse_response_status` (correct path for
  generated projects) instead of the non-existent
  `crate::embedded::genomehubs_query::`.
- Error branch uses `parse::ResponseStatus` struct to avoid Tera-interpreted
  brace syntax in the template source.

### `templates/rust/sdk.rs.tera`

- `count()`: reads response as text, delegates to
  `crate::embedded::core::parse::parse_response_status`, returns `status.hits`.
  Fixed incorrect `body["count"]` access.

### `templates/rust/client.rs.tera`

- `count()`: same fix — reads text body, delegates to
  `crate::embedded::core::parse::parse_response_status`.

### `python/cli_generator/query.py`

- `count()`: decodes response body as text, calls `parse_response_status` from
  the Rust extension, parses the returned JSON, returns `hits`.

### `templates/r/query.R`

- `count()`: reads response as raw text, calls `parse_response_status(raw_text)`,
  parses with `jsonlite::fromJSON`, returns `hits`.

### `templates/js/query.js`

- `count()`: reads response as text, calls `wasmModule.parse_response_status(text)`,
  parses the returned JSON, returns `hits`.

### `python/cli_generator/__init__.py`

- Added `parse_response_status` to imports and `__all__`.

### `python/cli_generator/cli_generator.pyi`

- Added typed stub for `parse_response_status(raw: str) -> str`.

## Test results

```
cargo test --workspace
  genomehubs-query lib: 84 passed
  cli-generator lib: 71 passed
  integration: 15 passed
  doc-tests: 2 passed
  Total: 172 passed, 0 failed

pytest tests/python/ -v
  80 passed, 0 failed

pyright python/ tests/python/
  0 errors, 0 warnings, 0 informations
```

## Design decisions

- **FFI boundary returns JSON string**: `parse_response_status` returns a
  compact JSON string rather than a struct at all FFI boundaries (PyO3, WASM).
  This avoids complex type mapping and is consistent with the existing pattern
  for `build_url`, `describe_query`, and `render_snippet`.

- **`response_status_to_json` serialises the inner status, not the envelope**:
  The function emits `{"hits":N,"ok":bool,"error":...}` for callers who already
  have a `ResponseStatus`. It is NOT a round-trip partner for
  `parse_response_status` (which expects the full API envelope). Tests updated
  to reflect this.

- **Tera template brace escaping**: `lib.rs.tera` cannot contain raw
  `format!(r#"...{e:?}..."#)` because Tera parses `{e:?}` as a template
  expression. The error path uses `parse::ResponseStatus` struct construction +
  `response_status_to_json` to avoid any format strings with braces.
