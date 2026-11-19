---
date: 2026-11-18
agent: GitHub Copilot
model: Claude Haiku 5.5
task: Write CLI-first architecture note and API/SDK coverage review before SDK refactor
files_changed:
  - docs/planning/cli-first-architecture.md
  - agent-logs/2026-11-18_001_cli-first-architecture-note.md
---

## Task summary

The user asked for an architecture note so future sessions follow the Rust and
CLI-first direction. Templates become scaffolding only, and SDKs move out of
templates one language at a time. The user also asked for a review of API
endpoints and SDK methods, to confirm CLI coverage and to identify functions
to consolidate before the SDK refactor. The review was read-only. The only
file written is the architecture note.

## Key decisions

- **Decision:** Rust (`crates/genomehubs-query`) is the single source of truth.
  The CLI is the reference interface. SDKs are thin packages. Templates are
  scaffolding only.
  **Alternative considered:** keep logic in templates and sync via parity tests.
  Rejected because it keeps re-implementation and drift across languages.
- **Decision:** Migrate one language at a time, gated by parity tests. Proposed
  order is Python, then R, then JS.
  **Alternative considered:** migrate all languages together. Rejected as too
  large to verify.
- **Decision:** The coverage matrix is verified against
  `crates/genomehubs-api/src/routes/*.rs`, not against the older gap-analysis doc.
  **Alternative considered:** use `docs/planning/v3-gap-analysis.md`. Rejected
  because its endpoint paths are stale.
- **Decision:** Consolidation items C1 to C11 are recorded with evidence, and no
  deletion is made.
  **Alternative considered:** delete dead modules now. Rejected because
  deletions need user confirmation.

## Interaction log

| Turn | Role  | Summary                                                                                                                                                                                                   |
| ---- | ----- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1    | User  | Asked for CLI validate, then pure v3 constraint                                                                                                                                                           |
| 2    | User  | Asked for architecture note and coverage review                                                                                                                                                           |
| 3    | Agent | Surveyed routes, CLI, Python/R/JS/Rust SDKs, and duplicate modules                                                                                                                                        |
| 4    | Agent | Confirmed API depends on root crate; confirmed duplicate snippet modules                                                                                                                                  |
| 5    | Agent | Wrote architecture note and this log                                                                                                                                                                      |
| 6    | User  | Approved batch flag, deletions C1/C2/C4/C5, Python-R-JS order, v3 POST                                                                                                                                    |
| 7    | User  | Asked to start Phase 1; approval required for any CLI flag or option                                                                                                                                      |
| 8    | Agent | Surveyed C1 and C2; found generator copy lists and templates still use them. Deferred. Moved C3 types into subcrate, deleted C4 and C5, dropped API root-crate dependency. Build, clippy, and tests pass. |
| 11   | User  | Asked for query-building overrides (`--taxon`, `--taxon-rank`, `--taxonomy`) and `report` and `positional` commands sharing the query and plot processing; flags approved one at a time                   |

## Changes made

- Added `docs/planning/cli-first-architecture.md`. It covers principles, target
  layout and dependency direction, the 21-path coverage matrix, consolidation
  candidates C1 to C11, a phased migration plan with gates, rules for future
  work, and open decisions.
- No source code changed. No files deleted.

### Phase 1 (interaction turn 8)

- **Done (C3):** `query_builder.rs` moved to `crates/genomehubs-query/src/`.
  `attr_types.rs` (pure `TypeMeta`, `TypesMap`, `SynonymsMap` types) created in
  the subcrate. The HTTP `attr_types()` fetch and `set_processed_*` helpers were
  dropped as dead. The API repoints 14 call sites across 9 files, and
  `cli_generator` is removed from `crates/genomehubs-api/Cargo.toml`.
- **Done (C4):** deleted `src/core/count.rs` and `tests/integration/` (contained
  `es_integration.rs`, which was not a test target and was already broken).
- **Done (C5):** deleted `src/core/process_hits.rs` and `tests/process_hits.rs`.
  The user asked for a merge check. The deleted code parsed v2-shaped `inner_hits`
  for `names` and `identifiers`. The v3 API returns `taxon_names` via `_source`,
  so nothing was merged into `parse.rs`.
- **Deferred (C1, C2), user decision:** `validation.rs` and `snippet.rs` are
  copied into generated projects and imported by templates, so they are not dead.
  The user chose to keep them until template migration (Phase 5), when they will
  be deleted with the templates. `QuerySnapshot` moves to the subcrate at that time.
- **Verification:** `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace` pass. The API builds without the root crate.
- **Not done:** the CLI v3 transport (Phase 1 step 3) needs approval for flags.

### Phase 1 step 3 (interaction turn 9)

- **Approved flags:** `--query-yaml` and `--params-yaml`. The user asked for
  file-or-text input on both flags, and for one combined YAML carrying query and
  params keys via `--query-yaml`.
- **Subcrate:** `split_query_document` in `crates/genomehubs-query/src/query/mod.rs`
  splits a combined document by `QUERY_PARAMS_KEYS`, merges an explicit params
  document, and rejects keys set in both. It has no I/O.
- **Nested sections:** a combined document may use `query:` and `params:` sections
  instead of flat keys. Mixing the two forms, or placing keys in the wrong section,
  is an error.
- **CLI:** `src/core/runtime_query.rs` reads an existing path as a file. A missing
  `.yaml` or `.yml` path is an error. Anything else is inline YAML. `search` and
  `count` send POST `{query_yaml, params_yaml}`. `validate` and `describe` use the
  query half. `validate` also reports params errors. `search --size` conflicts
  with `--query-yaml`. `--params-yaml` requires `--query-yaml`.
- **Verification:** `cargo fmt --all`, `cargo clippy --workspace --all-targets
-- -D warnings`, and `cargo test --workspace` pass. Smoke tests against
  `localhost:3000/api` covered inline and file combined documents, `--params-yaml`,
  `size` from both documents (2 hits, 1 result), and the conflict and requires
  errors.

### Phase 1 step 3 (interaction turn 10)

- **Request:** `--query-yaml` and `--params-yaml` are not required. Flags combine to
  build the query, and a bare `search` or `count` is a search-all.
- **Default:** with neither YAML flag, the query is `index: {--result}` (preset
  index, then `taxon`) and params are empty. The API returns results for this query
  (2,521,138 hits, 10 returned by default).
- **Params only:** `--params-yaml` works without `--query-yaml`. It supersedes the
  turn 9 rule that `--params-yaml` requires `--query-yaml`.
- **Overrides:** `--size`, `--page`, `--sort-by`, `--sort-order`, `--tidy`, and
  `--include-estimates` no longer carry a clap `requires` constraint. They apply on
  top of any YAML. On the v2 `--query` path, only `--size` applies; the others error.
- **Verification:** `cargo fmt --all`, `cargo clippy --workspace --all-targets
-- -D warnings`, and `cargo test --workspace` pass. Smoke tests against
  `localhost:3000/api`: bare `search` (10 results), `--params-yaml 'size: 3'` (3),
  `--page 2 --size 2` (2), `--query-yaml 'index: taxon' --sort-order desc --size 2`
  (2), bare `count` (success), and `--query 'tax_tree(Mammalia)' --page 2` (error).
- **Open:** flag-driven query building (taxa, rank, attributes, filters) needs
  flag names approved before implementation.

### Phase 1 step 4 (interaction turn 11)

- **Approved flags:** `--taxon` (repeatable), `--taxon-rank`, `--taxonomy` (overrides on
  `search` and `count`), `--report-yaml`, `--positional-yaml`, `--include-plot-spec`
  (bare flag), and `--display` (JSON string).
- **Overrides:** `QueryOverrides` in `src/core/runtime_query.rs`. `--taxon` replaces
  `identifiers.taxa.names` and keeps `taxon_filter_type`. `--taxon-rank` sets
  `identifiers.rank`, which maps to `tax_rank(X)`. `--taxonomy` sets the params
  `taxonomy`. Overrides apply only when set. On the v2 `--query` path they error.
- **New commands:** `report` (`src/commands/report.rs`) and `positional`
  (`src/commands/positional.rs`) POST `/api/v3/report` and `/api/v3/positional`.
  Both use the shared `run_v3_plot` and `PlotOptions` path with the query flags.
  The v2 input path is rejected.
- **Verification:** `cargo fmt --all`, `cargo clippy --workspace --all-targets
-- -D warnings`, and `cargo test --workspace` pass. Smoke tests against
  `localhost:3000/api`: `--taxon Mammalia --taxon Aves --size 2` (2 hits);
  `report` with `report: histogram` and `x: genome_size` (plot_spec only with
  `--include-plot-spec` or `--display`); `positional` reached the server and
  returned the v1 index error.
- **Open findings:** `--taxon-rank species` returns 0 hits. The v3 API returns 0 for
  `rank: species` and `rank: class` sent raw, so the client sends the expected
  query and the issue is server-side. `positional` needs a v2 feature index
  (`feature--ncbi--goat--2026.11.17` is v1). The `boat` search exit 1 is not
  investigated.
- **Approved and added since:** list flags accept repeat and comma (`--taxon`,
  `--field`, `--assembly`, `--sample`, `--rank`, `--name`). `--rank` sets return
  rank columns, `--name` sets name classes, `--field` sets return fields,
  `--filter` (repeatable) sets attribute filters, `--assembly` and `--sample`
  filter identifiers, and `--taxon-filter` (`name`, `tree`, `lineage`) sets the
  taxon filter type. See `2026-11-18_002_api-identifier-filters-and-rank-scope.md`.

## Notes / warnings

- **Findings:** CLI `search` and `count` use only the legacy v2 `url=` GET
  transport. This violates the pure v3 constraint and is the first fix needed
  (Phase 1, step 3 of the note).
- `genomehubs-api` depends on the root `cli-generator` crate
  (`crates/genomehubs-api/Cargo.toml` line 35). ES-body builders must move to
  the subcrate first.
- Root `src/core/snippet.rs` duplicates `crates/genomehubs-query/src/snippet.rs`.
- JS `validate()` calls `/api/v3/metadata/validation-config`, which the API does
  not serve. JS also references `/v2/search`. Python `search_all` uses v2
  `searchPaginated`.
- `docs/planning/v3-gap-analysis.md` has stale endpoint paths.
  `docs/planning/multi-language-roadmap.md` conflicts with the new direction
  in places. The new note states which parts are superseded. Neither file was
  edited in this session.
- Decisions recorded in section 7 of the note after user review: batch as a flag
  on the base command; deletions C1, C2, C4, C5 approved; language order Python,
  then R, then JS; CLI `search` and `count` default to v3 POST. Phase 1 has not
  started.
