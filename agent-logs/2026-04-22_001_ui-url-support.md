# 2026-04-22_001 — UI URL support

## Summary

Added `to_ui_url()` / `--ui-url` across all SDK languages and the generated
CLI, alongside a new `ui_base` YAML config field.

## Motivation

The existing `to_url()` / `--url` methods produce REST API URLs suitable for
machines. Tool users (especially MCP server consumers) also need a human-facing
**UI URL** they can paste into a browser to explore results. For GoaT and BoaT
the conversion is trivial (strip `/api/v2` from the API URL), but future sites
may use different subdomains or path structures, so an explicit `ui_base` config
field is cleaner than string manipulation at runtime.

## Changes

### Config (`src/core/config.rs`, `sites/*.yaml`)

- Added optional `ui_base: Option<String>` field to `SiteConfig`.
- Added `resolved_ui_base()` method: returns `ui_base` when present; otherwise
  strips a trailing `/api` segment from `api_base` as a default.
- Added `ui_base` to `goat.yaml` and `boat.yaml`.

### Rust subcrate (`crates/genomehubs-query/`)

- Added `build_ui_url(query, params, ui_base, endpoint) -> String` to
  `src/query/url.rs`. Same query parameters as `build_query_url` but the base
  path is `{ui_base}/{endpoint}?…` with no API version component.
- Re-exported via `src/query/mod.rs`.
- Added WASM export `build_ui_url` to `src/lib.rs`.

### cli-generator library (`src/`)

- Added `build_ui_url` to `src/core/query/mod.rs` re-exports.
- Added `#[pyfunction] build_ui_url` to `src/lib.rs` and registered it in the
  `#[pymodule]`.
- Added `to_ui_url()` method to `python/cli_generator/query.py`.
- Added `build_ui_url` to `python/cli_generator/__init__.py` and the `.pyi` stub.

### Templates

| File                              | Change                                                              |
| --------------------------------- | ------------------------------------------------------------------- |
| `templates/rust/cli_meta.rs.tera` | Added `UI_BASE_URL` constant                                        |
| `templates/rust/sdk.rs.tera`      | Added `UI_BASE_URL` const and `build_ui_url` pyfunction             |
| `templates/rust/lib.rs.tera`      | Registered `sdk::build_ui_url` in `#[pymodule]`                     |
| `templates/rust/client.rs.tera`   | Added `UI_BASE_URL` const and `ui_url()` function                   |
| `templates/rust/main.rs.tera`     | Added `--ui-url` flag and dispatch branch                           |
| `templates/python/query.py.tera`  | Added `UI_BASE` constant and `to_ui_url()` method                   |
| `templates/r/query.R`             | Added `ui_base_url` private field and `to_ui_url()` method          |
| `templates/r/lib.rs.tera`         | Added `build_ui_url` extendr function and registered it             |
| `templates/js/query.js`           | Added `UI_BASE` constant, imported `_buildUiUrl`, added `toUiUrl()` |

### Code generation plumbing (`src/`)

- `src/core/codegen.rs`: Added `ui_base` to Tera context.
- `src/commands/new.rs`: Added `ui_base` to R package Tera context; added
  `build_ui_url` to generated Python `__init__.py`.

### Tests

- `tests/python/test_sdk_parity.py`: Added `to_ui_url` / `toUiUrl` to the
  canonical method table.

## Design decisions

- `to_ui_url()` is a distinct method rather than `to_url("ui")` — the latter
  would require the SDK to switch internally on a magic string, which is fragile
  and a worse API.
- `ui_base` defaults gracefully by stripping `/api` from `api_base`, so existing
  generated projects that haven't re-run `cli-generator update` still get
  sensible behaviour for GoaT/BoaT.
- The UI URL builder reuses `build_query_url`'s parameter set (size, sort,
  fields, etc.) because the GoaT UI honours them, but omits the API version path
  component.

## Verification

- `cargo clippy --all-targets -- -D warnings`: clean
- `cargo test --test generated_goat_cli`: 15/15 pass
- `pytest tests/python/ -q`: 394 passed, 3 skipped (Quarto docs, unrelated)
