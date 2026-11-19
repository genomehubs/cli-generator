---
date: 2026-11-18
agent: GitHub Copilot
model: Claude Haiku 5.5
task: Forward assembly and sample filters to the v3 API and stop rank-only queries returning zero hits
files_changed:
  - crates/genomehubs-query/src/query_builder.rs
  - crates/genomehubs-api/src/request_shape.rs
  - crates/genomehubs-api/tests/search_builder.rs
---

## Task summary

CLI smoke tests showed that `--assembly` and `--sample` were ignored by the v3
search path, and that rank-only queries (`tax_rank`) returned zero hits. The
user asked for the API to forward these identifiers and for a non-empty query
to be scoped to a root taxon (or for the builder to return all results when the
taxon is missing). Only the API-side fix was in scope for this session.

## Key decisions

- **Decision:** forward `identifiers.assemblies` and `identifiers.samples` from
  `query_to_body_input` into `SearchBodyInput`, emitted as `terms` filters on
  `assembly_id` and `sample_id`.
  **Alternative considered:** match on `assembly_id` via `query_string`. Not chosen;
  `terms` is exact and cheaper.
- **Decision:** a non-empty query without an explicit taxa expression is scoped to
  the default root `2759` (NCBI Eukaryota) for `taxon`, `assembly`, and `sample`
  groups. The rank restriction is a separate `match` on `taxon_rank`.
  **Alternative considered:** discover the root at startup from the first taxon
  document's lineage. Not implemented; it needs `AppState` plumbing and a startup
  Elasticsearch call. Awaiting user approval.
- **Decision:** empty queries stay match-all. Any identifier, rank, attribute,
  or field makes the query non-empty.

## Interaction log

| Turn | Role  | Summary                                                                                                                              |
| ---- | ----- | ------------------------------------------------------------------------------------------------------------------------------------ |
| 1    | User  | Approved list-flag convention and flag names (`--rank`, `--field`, `--name`, `--filter`, `--assembly`, `--sample`, `--taxon-filter`) |
| 2    | User  | Asked to forward to the API and fix rank-only handling (scope to root or allow missing taxon)                                        |
| 3    | Agent | Added forwarding, default-root scope, unit and integration tests; restarted API                                                      |
| 4    | Agent | Re-ran CLI smoke tests with `--api-base http://localhost:3000/api`                                                                   |

## Changes made

- `crates/genomehubs-query/src/query_builder.rs`: `SearchBodyInput` gains
  `assemblies` and `samples`. Match-all early return now also requires those
  to be empty. Non-empty taxa-less queries are wrapped with `2759`. Assembly and
  sample `terms` filters are added before the rank restriction. Four unit tests
  added.
- `crates/genomehubs-api/src/request_shape.rs`: `query_to_body_input` forwards
  `assemblies` and `samples`.
- `crates/genomehubs-api/tests/search_builder.rs`: two integration tests
  (`assembly_and_sample_identifiers_reach_body`,
  `rank_only_query_is_not_scoped_to_rank_string`).

Verification: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D
warnings`, and `cargo test --workspace` pass. The `with_overrides` tests passed
on three repeat runs; the earlier single failure did not reproduce.

Smoke tests against `http://localhost:3000/api` (run with `--api-base`):

| Command                                                                | Result                                                   |
| ---------------------------------------------------------------------- | -------------------------------------------------------- |
| `search --result assembly --assembly GCA_000001405.29`                 | 1 hit, `assembly-GCA_000001405.29`                       |
| `search --result assembly --assembly GCA_000001405.29,GCF_000002305.6` | 1 hit (second accession not present in the index)        |
| `search --result sample --sample SAMN00000060`                         | 1 hit, `sample-SAMN00000060`                             |
| `search --taxon-rank genus`                                            | 138946 hits                                              |
| `search --taxon-rank species`                                          | 2158607 hits                                             |
| `search --taxon Mammalia --taxon-filter tree --taxon-rank species`     | 12583 hits                                               |
| `search --taxon Mammalia --taxon-filter tree --taxon-rank genus`       | 1447 hits                                                |
| `search --taxon Mammalia --taxon-rank species` (name filter)           | 0 hits (expected: the name filter matches only Mammalia) |

## Notes / warnings

- **Smoke-test pitfall:** `--site goat` resolves to the public GoaT API, not the
  local server. Use `--api-base http://localhost:3000/api` for local checks.
- **`--rank` is the return rank-column flag** (`ranks` in the YAML). The rank
  filter is `--taxon-rank`. `--rank genus` alone does not filter.
- **Scope caveat:** scoping to `2759` excludes assemblies and samples outside
  Eukaryota when only an accession filter is given.
- **Deviation:** root discovery from the index at startup is not implemented.
  Follow-up needs approval.
- **Open:** the `positional` endpoint needs a v2 feature index
  (`feature--ncbi--goat--2026.11.17` is v1). The `report` histogram needs `x`.
- **Behaviour change:** `search --field x` now routes to v3.
