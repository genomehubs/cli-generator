---
date: 2026-11-17
agent: GitHub Copilot
model: claude-haiku-5.5
task: Implement CLI slice of the generic runtime refactor (steps 1-3): runtime site selection via --site, metadata cache, and search/count commands
files_changed:
  - config/site-presets.yaml
  - src/core/config.rs
  - src/core/fetch.rs
  - src/core/metadata.rs
  - src/core/runtime_query.rs
  - src/core/mod.rs
  - src/commands/search.rs
  - src/commands/count.rs
  - src/commands/lookup.rs
  - src/commands/describe.rs
  - src/commands/mod.rs
  - src/main.rs
  - tests/generated_goat_cli.rs
  - GETTING_STARTED.md
---

## Task summary

Implemented the CLI portion of `docs/refactor/generic-runtime-refactor-backlog.md`
steps 1 to 3. The CLI now selects an instance at runtime through a global
`--site` argument (with `--api-base` and `--ui-base` overrides and environment
variables), using built-in goat and boat presets. A metadata cache validates
`--field` names, and `search` and `count` commands run queries against the
API. A `lookup` command was added after the API was fixed, and a `describe`
command was added last. `describe` reuses the canonical v2 URL converter.

## Key decisions

- **Decision:** Reuse `--site` as the runtime selector and rename the scaffold
  argument of `preview` to `--scaffold`.
  **Alternative considered:** a new `--instance` flag. Rejected to keep one
  selector name across commands.
- **Decision:** Make `RuntimeArgs` global on `Cli` via `#[command(flatten)]`.
  **Alternative considered:** per-subcommand args. Rejected because clap
  asserts on duplicate ids between a global arg and a subcommand arg. For the
  same reason, the `New` positional was renamed to `config_name`.
- **Decision:** Preset precedence is CLI > env > preset > derived.
  `require_api_base` fails with a message that lists known sites.
- **Decision:** Ship presets as `config/site-presets.yaml`, embedded with
  `include_str!`.
  **Alternative considered:** hard-coded Rust constants. Rejected so that
  presets can be edited as data.
- **Decision:** Cache result field metadata on disk and reject unknown
  `--field` names before querying.
- **Decision:** `describe` converts its query with the existing canonical v2
  converter (`genomehubs_query::query::query_yaml_from_url_params`, the one
  behind `from_v2_url`) rather than a new parser.
  **Alternative considered:** a fresh Rust expression parser. Rejected to keep
  a single canonical implementation.

## Interaction log

| Turn | Role  | Summary                                                                         |
| ---- | ----- | ------------------------------------------------------------------------------- |
| 1    | User  | Asked for a runtime-configured generic CLI per the refactor backlog             |
| 2    | Agent | Proposed the plan and asked about scope and naming                              |
| 3    | User  | Confirmed reuse of `--site`, CLI-only scope, and deferred steps                 |
| 4    | Agent | Implemented steps 1 to 3 and ran fmt, clippy, and tests                         |
| 5    | Agent | Fixed the mockito query-matching issue in `api_get_reports_http_errors`         |
| 6    | User  | Bumped the index suffix, restarted the API, and asked to resume the plan        |
| 7    | Agent | Added the `lookup` command, ran fmt, clippy, and the full test suite            |
| 8    | User  | Asked for `describe` to convert v2 query syntax as `from_v2_url` does           |
| 9    | Agent | Added `describe` reusing the v2 converter, tested it, and ran a live smoke test |

## Changes made

- `config/site-presets.yaml`: new built-in presets for goat and boat
  (`api_base`, `ui_base`, `default_index`, `description`).
- `src/core/config.rs`: `SitePresetRegistry::builtin()`, `site_names()`,
  `RuntimeConfig::require_api_base()`, and `RuntimeConfig::resolve()` with
  precedence rules. Tests added. Preset tests share `ENV_LOCK` to avoid races.
- `src/core/fetch.rs`: `fetch_index` now takes the index name explicitly.
- `src/core/metadata.rs`: metadata cache (`MetadataOptions`, cache keys,
  `load_result_fields`, `validate_fields`, unknown-field rejection). Tests use
  mockito.
- `src/core/runtime_query.rs`: `QueryRequest`, `resolve_result_index`,
  `build_query_string`, `api_get`, `run_query`. Tests cover HTTP errors with
  `match_query(Matcher::Any)`, because mockito does not match a path-only mock
  against a request that has a query string.
- `src/commands/search.rs`, `src/commands/count.rs`: command wiring that prints
  pretty JSON.
- `src/core/runtime_query.rs`: `LookupRequest`, `run_lookup` (GET
  `/v3/lookup` with `searchTerm`, `result`, and `size`), and a shared `get_json`
  helper used by `api_get`. Mockito test added.
- `src/commands/lookup.rs`: `lookup` command that prints pretty JSON.
- `src/main.rs`: `Lookup { term, --result, --size }` subcommand and dispatch.
- `src/core/runtime_query.rs`: `search_query_from_request` (v2 converter to
  `SearchQuery`), `describe_request` (metadata to `QueryDescriber`), and
  `field_map`. Tests added for the conversion and the describe path.
- `src/commands/describe.rs`: `describe` command that prints the text.
- `src/main.rs`: `Describe { --result, --query, --field, --verbose }` subcommand
  and dispatch.
- `src/main.rs`: global `RuntimeArgs` (`--site`, `--api-base`, `--ui-base`,
  `--refresh-metadata`, `--metadata-cache-dir`), `Search` and `Count`
  subcommands, `--scaffold` on `preview`, and the `New` positional renamed to
  `config_name`.
- `tests/generated_goat_cli.rs`: `preview_new_site_produces_output` uses
  `--scaffold goat`.
- `GETTING_STARTED.md`: preview example updated to `--scaffold`.

## Notes / warnings

- Verification: `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings`,
  and `cargo test` pass. Totals: 107 lib, 14 `api_endpoints`, 15
  `generated_goat_cli`, 1 other. The API ran on `localhost:3000` during the run.
- Smoke test: `cargo run -q -- lookup "Homo sapiens" --site goat --size 3`
  returns the expected hits from the live API.
- Smoke test: `cargo run -q -- describe --site goat --query "tax_tree(Mammalia) AND genome_size>=1000000000" --field genome_size`
  returns the expected prose from the live API.
- Duplication still open: the root `src/core/describe.rs` and
  `crates/genomehubs-query/src/describe.rs` both define `QueryDescriber`.
  `describe` uses the root copy. Not merged in this change.
- Concise prose reads awkwardly for taxa ("Search for taxa in taxa (...)").
  That comes from the existing describer and was not changed here.
- Smoke tests of `search` and `count` against the live API are not run.
- API startup hang (resolved by the user, not changed in code): a stale
  `index_suffix` (`2026.11.16`) returned 404 and `populate_with_retry(..., None)`
  retried forever. The user bumped the suffix to `2026.11.17`. Whether startup
  should fail fast on a permanent 404 is still open and was not changed.
