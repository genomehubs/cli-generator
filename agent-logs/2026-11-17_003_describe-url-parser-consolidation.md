---
date: 2026-11-17
agent: GitHub Copilot
model: Claude Haiku 5.5
task: Consolidate describe into one canonical describer, add v3 describe input, then consolidate the flat-param URL parser on the CLI path
files_changed:
  - crates/genomehubs-query/src/describe.rs
  - src/core/describe.rs
  - src/core/runtime_query.rs
  - src/core/count.rs
  - src/core/query/mod.rs
  - src/core/query/adapter.rs
  - src/lib.rs
  - src/main.rs
  - src/commands/new.rs
  - templates/rust/lib.rs.tera
  - templates/r/lib.rs.tera
  - examples/adapter_demo.rs
  - examples/README.md
  - tests/integration/es_integration.rs
  - tests/python/run_query_builder_fixtures.py
---

## Task summary

The user asked for the describe implementations to be consolidated, since only one CLI remains, and for a check that `describe` works with pure v3 syntax (`--query` is legacy v2). The describer now lives once in the subcrate (`crates/genomehubs-query/src/describe.rs`). The root copy (`src/core/describe.rs`) was removed, and WASM, PyO3, the CLI, and generated-SDK templates all route through `describe_yaml`. A v3 `--query-yaml` input was added beside the v2 `--query` input.

A redundancy audit followed, and the user then asked for CLI-scoped consolidation of validation and `parse_url_params`. This entry also covers that second phase. The flat-param parser `core::query::adapter::parse_url_params` was removed. Its only compiled caller, `count_docs_from_url_params`, now goes through the canonical v2 converter (`runtime_query::search_query_from_request`). Its only other consumers were an example and an integration sketch, which were updated or removed.

## Key decisions

- **Decision:** one canonical describer in the subcrate, returned as YAML by `describe_yaml`. Every surface routes through it.
  **Alternative considered:** keep the root describer for the CLI. Rejected because it duplicated the subcrate logic and drifted from the WASM and SDK output.
- **Decision:** v3 describe input is `--query-yaml` (a `SearchQuery` YAML body), conflicting with `--query`, `--result`, and `--field`.
  **Alternative considered:** overloading `--query` with format detection. Rejected because it would blur the legacy v2 contract.
- **Decision:** `count_docs_from_url_params` takes a `QueryRequest` and converts it with `search_query_from_request`, so the v2 URL vocabulary has one owner.
  **Alternative considered:** keep `parse_url_params` and make it call the converter. Rejected because the flat parameter names and the `query_yaml` mode had no CLI caller.
- **Decision:** the raw `q`/`query` branch of the old count helper was dropped. `count_docs_from_query_str` already covers raw ES query strings.
- **Decision:** the root phf validator (`src/core/query/validation.rs`) was left unchanged. The CLI does not call it. Only template-generated Rust SDKs use it, and the project is moving away from that model.
  **Alternative considered:** port it onto the subcrate validator now. Deferred because it means changing `sdk.rs.tera`, `field_meta.rs.tera`, `main.rs.tera`, and the `new.rs` embedding, which is template-path work.

## Interaction log

| Turn | Role  | Summary                                                                                                       |
| ---- | ----- | ------------------------------------------------------------------------------------------------------------- |
| 1    | User  | Consolidate describe across files; confirm v3 syntax works; `--query` is legacy v2                            |
| 2    | Agent | Consolidated describer into the subcrate, added `--query-yaml`, removed root describer, ran checks            |
| 3    | User  | Asked to stop the dev-site check (template-based SDKs are being moved away from)                              |
| 4    | Agent | Wrote the redundancy audit in chat; no consolidation applied yet                                              |
| 5    | User  | Approved the agent log; asked to stay CLI-focused and consolidate validation and `parse_url_params`           |
| 6    | Agent | Mapped every consumer; found the CLI has no validator call; `parse_url_params` only feeds the count helper    |
| 7    | Agent | Routed `count_docs_from_url_params` through `search_query_from_request`; deleted `adapter.rs` and the example |

## Changes made

- **Describer (earlier in session):** `crates/genomehubs-query/src/describe.rs` is the canonical describer. Its `describe_yaml` takes the query YAML, field metadata, and a verbose flag. `src/core/describe.rs` was deleted. `src/core/runtime_query.rs` adds `DescribeInput` (`V2` and `V3Yaml`). `src/main.rs` adds `--query-yaml`. `src/lib.rs`, `templates/rust/lib.rs.tera`, and `templates/r/lib.rs.tera` route through `describe_yaml`. `src/commands/new.rs` copies the shared core modules into generated projects.
- **Parser (this phase):** `src/core/query/adapter.rs` was deleted, and `pub mod adapter;` was removed from `src/core/query/mod.rs`. In `src/core/count.rs`, `count_docs_from_url_params` was replaced by `count_docs_from_request(base_url, index, &QueryRequest)`. A private `match_body(&SearchQuery)` builds the body. Tests: `count_docs_from_request_posts_match_filters` and `match_body_is_match_all_without_taxa_or_fields`.
- **Example:** `examples/adapter_demo.rs` was deleted because it only demonstrated the removed parser. Its section in `examples/README.md` was removed.
- **Stale references:** `tests/integration/es_integration.rs` and `tests/python/run_query_builder_fixtures.py` (a comment) were updated to the new API.

## Verification

- `cargo fmt --all`: clean.
- `cargo clippy --all-targets -- -D warnings`: clean.
- `cargo test --workspace`: all green. Lib 111 passed, subcrate 387 passed, API endpoints 14 passed, generated-goat 15 passed.
- Earlier in the session: `cargo check -p genomehubs-query --features wasm` passed, `maturin develop --features extension-module` built, and `pytest tests/python/ -q` gave 558 passed and 20 skipped (live API).
- The dev-site check (`scripts/dev_site.sh --python goat`) was not run. The user stopped it because it exercises template-based SDKs.

## Notes / warnings

- **Validation consolidation is open.** The CLI's only validation is `metadata::unknown_field_names`, which is separate from the subcrate validator. The root phf validator duplicates the subcrate validator, but only template-generated SDKs use it. Options: port `sdk.rs.tera` onto the subcrate validator, or delete template SDK validation with the template path. A decision is needed.
- **`core::count` is dead for the CLI.** `count_docs`, `count_docs_raw`, `count_docs_from_query_str`, and `count_docs_from_request` are called only by their own tests and by the uncompiled integration sketch. The CLI count path goes through the API. Per the "no dead code" rule, the module should be deleted or kept with a stated reason. Not done yet.
- **`tests/integration/es_integration.rs` is not compiled.** No Cargo test target or `main.rs` picks it up. It also has pre-existing problems: `crate::core` paths, a missing `toml` dependency, and a reference to a non-existent `cfg.index` field. Its two references were updated, but it should be fixed or deleted.
- `config/es_integration.toml` has `index_suffix = "--ncbi--goat--2026.11.16"`, while live ES holds `2026.11.17` indices. This was not verified.
- `examples/README.md` lists a `live_query_demo` example that does not exist in `examples/`. This was already stale and was not changed.
- `docs/` contains historical references to `adapter::parse_url_params`. These are planning and audit records and were left as written.
- `api_get_reports_http_errors` failed once earlier and passed on every rerun. It may be flaky.
