---
date: 2026-11-18
agent: GitHub Copilot
model: Claude Haiku 5.5
task: Discover the root taxon at API startup and remove the v2 transport from the CLI
files_changed:
  - crates/genomehubs-api/src/es_metadata.rs
  - crates/genomehubs-api/src/routes/count.rs
  - crates/genomehubs-api/src/routes/count_batch.rs
  - crates/genomehubs-api/src/routes/search.rs
  - crates/genomehubs-api/src/routes/search_batch.rs
  - crates/genomehubs-api/src/routes/report.rs
  - crates/genomehubs-api/tests/search_builder.rs
  - crates/genomehubs-query/src/query_builder.rs
  - src/core/runtime_query.rs
  - src/core/count.rs
  - src/commands/mod.rs
  - src/commands/record.rs
  - src/commands/search.rs
  - src/commands/count.rs
  - src/commands/describe.rs
  - src/commands/validate.rs
  - src/commands/positional.rs
  - src/commands/report.rs
  - src/main.rs
  - docs/planning/cli-first-architecture.md
  - config/es_integration.toml
---

## Task summary

The root taxon scope was hard-coded to `2759`, which is wrong for the local
instance (root `1`) and for at least one live instance. The root is now read
from the index at startup. The user also asked to drop legacy v2 support from
the CLI. Only the v2-to-v3 URL translation for `--query` remains. Report and
positional refinement are deferred.

## Key decisions

- **Decision:** the root is the last lineage entry of the first taxon document,
  read at startup into `MetadataCache.root_taxon_id`. A failed lookup returns
  an error, so startup retries.
  **Alternative considered:** keep the `2759` constant. Rejected because it is
  wrong for some instances.
- **Decision:** the query builder takes the root as data
  (`SearchBodyInput.root_taxon_id`). `None` means unscoped.
  **Alternative considered:** read the root inside the builder. Rejected because
  it would couple the pure subcrate to I/O.
- **Decision:** `--query` is translated to v3 locally with
  `query_yaml_from_url_params`. No v2 HTTP request is sent.
  **Alternative considered:** drop `--query` entirely. The user chose to keep it
  as a translation.
- **Decision:** overrides (`--taxon`, `--filter`, and the other v3 flags) apply
  to `--query` too. Previously they were rejected on that path.

## Interaction log

| Turn | Role  | Summary                                                                                                |
| ---- | ----- | ------------------------------------------------------------------------------------------------------ |
| 1    | User  | Asked to drop v2 from the CLI, keeping only URL translation. Report and positional refinement deferred |
| 2    | User  | Said root discovery must be dynamic; at least one live instance lacks `2759`                           |
| 3    | User  | Chose to keep `--query` as a translation                                                               |
| 4    | Agent | Added root discovery, root threading, v3-only runtime path, and removed v2 transport and count helpers |

## Changes made

- `es_metadata.rs`: root discovery at startup, with `fetch_root_taxon_id` and a
  pure parser. Two unit tests.
- `query_builder.rs`: `root_taxon_id` replaces the `2759` default.
- Routes `count`, `count_batch`, `search`, `search_batch`, and `report` pass the
  discovered root.
- `runtime_query.rs`: v2 transport and `QueryInput` removed. `run_v3_query`,
  `run_v3_plot`, `v3_query_input`, `v3_query_input_from_v2`, and `apply_overrides`
  on `SplitQueryDocument`.
- Command modules take `&SplitQueryDocument`. `search` and `count` no longer take
  metadata.
- `main.rs`: `--query` goes through `v3_query_input_from_v2`. Help text no longer
  says "(v3 only)".
- `count.rs`: request-based helpers removed.
- `record` (Phase 1 step 4): `record --record-id <IDS> [--result <INDEX>]` calls GET
  `/api/v3/record` with `recordId` (comma-joined). `RecordRequest`, `record_params`,
  `run_record` in `runtime_query.rs`, and `commands/record.rs`. Two unit tests.
  Smoke test: `record --result assembly --record-id GCA_000001405.29` returns the
  assembly.
- `summary` was approved but not implemented. The API route `/api/v3/summary` returns
  empty summaries and ignores the `summary` stats parameter. Waiting on the user.

## Verification

- `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`:
  pass.
- `cargo test --workspace -q`: all test targets pass. The `api_endpoints` suite
  failed 7 of 14 in an earlier run because the test index
  `taxon--ncbi--goat--2026.11.17` had been rotated. The index was bumped to
  `2026.11.18` in `config/es_integration.toml` and the API restarted. The suite
  then passed 14 of 14 in the final run.
- Smoke tests against `http://localhost:3000/api`: assembly `GCA_000001405.29`
  returns 1 hit; sample `SAMN00000060` returns 1 hit; `count --taxon-rank genus`
  returns 138947 (was 138946 before the index rotated).

## Notes / warnings

- `tests/integration/es_integration.rs` was removed in the earlier C4 step. It
  was not a test target and referenced removed items.
- `--site boat search --size 1` exited 1 earlier. Not investigated.
- `positional` needs a v2 feature index. The local `feature--ncbi--goat--2026.11.17`
  is v1.
- Open: `--taxon-rank species` returns 0 hits when combined with `--taxon Mammalia`
  (name filter). `--taxon-filter tree` gives 12583.
