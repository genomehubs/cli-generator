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
