---
date: 2026-11-18
agent: GitHub Copilot
model: Claude Haiku 5.5
task: Add licence and public-domain filters as API params on the PhyloPic routes, then add the phylopic CLI command with flags linked to those params
files_changed:
  - crates/genomehubs-api/src/phylopic_client.rs
  - crates/genomehubs-api/src/routes/phylopic.rs
  - src/core/runtime_query.rs
  - src/commands/phylopic.rs
  - src/commands/mod.rs
  - src/main.rs
  - docs/planning/phases/phase-14-phylopic.md
---

## Task summary

The user approved the phylopic CLI contract and asked that `--license` and
`--public-domain` be API params, with CLI flags linked to them. The CLI is
`phylopic --taxon-id <ID>` or `phylopic --batch <FILE>`, with optional
`--taxonomy`, `--license`, and `--public-domain`. The user approved these flags
by name. No other flags were added.

## Key decisions

- **Decision:** the licence filter is applied after the cache. The cache stays
  unfiltered and keyed by `taxon_id`, so one entry serves any filter.
  **Alternative considered:** key the cache by filter. Rejected because it
  multiplies entries for no gain.
- **Decision:** the filter lives in one pure type, `LicenceFilter`, in
  `phylopic_client.rs`. Both routes call it through the shared helpers
  `resolve_taxon` and `phylopic_response`.
- **Decision:** `license` is a comma-separated, case-insensitive SPDX list.
  Omitting it accepts any licence. `public_domain=true` checks that the licence
  URL contains `publicdomain`, the same rule v2 uses.
- **Decision:** the CLI `--license` flag accepts repeats or commas
  (`value_delimiter = ','`). It maps to the single `license` API param.
- **Decision:** the batch body sends `license` as a string or `null`, plus a
  `public_domain` bool. Both are optional on the API side.
- **Decision:** the CLI builds params and body in pure functions
  (`phylopic_params`, `phylopic_batch_body`) and reuses `get_json` and
  `post_json`.

## Interaction log

| Turn | Role  | Summary                                                                                |
| ---- | ----- | -------------------------------------------------------------------------------------- |
| 1    | User  | Approved the phylopic CLI flags and asked for licence and public-domain params         |
| 2    | Agent | Added `LicenceFilter` and the API params; refactored both handlers onto shared helpers |
| 3    | Agent | Added the core request types, builders, runner, and batch parser to `runtime_query.rs` |
| 4    | Agent | Added `commands/phylopic.rs`, the `Commands::Phylopic` variant, and dispatch           |

## Changes made

- `phylopic_client.rs`: `LicenceFilter { licenses, public_domain }`,
  `LicenceFilter::new`, `LicenceFilter::accepts`. Four tests.
- `routes/phylopic.rs`: `PhylopicQuery` and `PhylopicBatchRequest` gained
  `license` and `public_domain`. Both handlers use `resolve_taxon` and
  `phylopic_response`. Filtered-out images return `success: false` with
  `no image matching licence filter for taxon_id {id}`. utoipa params updated.
- `src/core/runtime_query.rs`: `PhylopicFilter`, `PhylopicRequest`,
  `phylopic_request`, `read_taxon_ids_file`, `parse_taxon_ids`,
  `phylopic_params`, `phylopic_batch_body`, `run_phylopic`. Eight tests,
  including two mockito tests for the GET and POST paths.
- `src/commands/phylopic.rs`: new command module that prints pretty JSON.
- `src/commands/mod.rs`: `pub mod phylopic;`.
- `src/main.rs`: `Commands::Phylopic` with `--taxon-id`, `--batch`,
  `--taxonomy` (default `ncbi`), `--license`, and `--public-domain`. The
  `--taxon-id` and `--batch` flags conflict. One of them is required.
- `docs/planning/phases/phase-14-phylopic.md`: licence params in the GET and
  batch tables, and a CLI section.

## Verification

- `cargo fmt --all`: pass.
- `cargo clippy --workspace --all-targets -- -D warnings`: pass.
- `cargo test -p genomehubs-api phylopic`: 22 passed.
- `cargo test -p cli-generator runtime_query`: 42 passed.
- `cargo test --workspace`: all suites pass, 0 failed.
- Live smoke test (local API restarted on port 3000 to load the route change):
  - `phylopic --taxon-id 9606`: returns `success: true`, licence `unknown`,
    image `Homo sapiens`.
  - `phylopic --taxon-id 9606 --public-domain`: `success: false`, no matching
    image. Correct, since the image is not public domain.
  - `phylopic --taxon-id 9606 --license CC0-1.0`: `success: false`. Correct.
  - `phylopic --batch <file> --license CC0-1.0,CC-BY-4.0` with IDs `"9606"` and
    `10090`: both `success: false`. Batch body is accepted and filtered per ID.

## Notes / warnings

- The batch file format (a JSON array of taxon IDs, strings or integers) still
  needs user confirmation before it is finalised.
- Behaviour change: filters return `success: false` for an image that exists but
  fails the filter. This is a `200 OK` response, consistent with other v3
  not-found cases.
