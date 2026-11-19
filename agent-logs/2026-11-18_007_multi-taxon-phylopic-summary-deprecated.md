---
date: 2026-11-18
agent: GitHub Copilot
model: Claude Haiku 5.5
task: Let phylopic --taxon-id take several IDs (repeated or comma-separated) and route them to the batch endpoint; mark the summary routes deprecated in the API
files_changed:
  - src/core/runtime_query.rs
  - src/main.rs
  - crates/genomehubs-api/src/routes/summary.rs
  - crates/genomehubs-api/src/routes/summary_batch.rs
  - crates/genomehubs-api/src/main.rs
  - docs/planning/phases/phase-14-phylopic.md
---

## Task summary

The user asked for `phylopic --taxon-id` to accept multiple IDs, given as a
repeated flag or as a comma-separated list, and to route them through the
PhyloPic batch endpoint in addition to `--batch`. The user also asked to leave
summary deferred and mark it deprecated in the API and CLI. No SDK wrapper was
added.

## Key decisions

- **Decision:** `--taxon-id` is a `Vec<String>` with `value_delimiter = ','`.
  One ID calls `GET /v3/phylopic`. Two or more call `POST /v3/phylopic/batch`.
  Blank entries are dropped after trimming, so `9606, 10090` works.
  **Alternative considered:** a separate `--taxon-ids` flag. Rejected, since the
  user asked for `--taxon-id` to accept several values.
- **Decision:** `--taxon-id` and `--batch` stay mutually exclusive, and one of
  them is still required.
- **Decision:** the summary routes are marked with a Rust `#[deprecated]`
  attribute on the handlers. utoipa 5.4 reads deprecation from that attribute,
  so the OpenAPI spec marks `/api/v3/summary` and `/api/v3/summary/batch` as
  deprecated. The routes still work.
  **Alternative considered:** a bare `deprecated` flag in `#[utoipa::path]`.
  Rejected because utoipa 5.4 does not parse it and the build fails.
- **Decision:** no CLI command for summary exists, so there is nothing to mark
  in the CLI. This is stated to the user rather than inventing a command.

## Interaction log

| Turn | Role  | Summary                                                                                    |
| ---- | ----- | ------------------------------------------------------------------------------------------ |
| 1    | User  | Asked for multi-ID `--taxon-id` to use batch, no SDK wrapper, summary deprecated           |
| 2    | Agent | Read the phylopic request builder, CLI variant, summary routes, and utoipa source          |
| 3    | Agent | Changed `phylopic_request` and the `Phylopic` variant, and added tests                     |
| 4    | Agent | Tried a bare `deprecated` path flag, which failed. Switched to `#[deprecated]` on handlers |
| 5    | Agent | Ran fmt, clippy with `-D warnings`, and both crates' tests. All passed                     |
| 6    | Agent | Restarted the local API and smoke tested single, comma-separated, and repeated IDs         |
| 7    | Agent | Confirmed the OpenAPI spec marks both summary routes deprecated, then wrote this log       |

## Changes made

- `src/core/runtime_query.rs`: `phylopic_request` takes `Vec<String>`. It trims
  and drops blank IDs, then picks Single for one ID or Batch for two or more.
  The batch file path is unchanged. Tests cover one ID, many IDs with blanks,
  and an ID combined with a batch file.
- `src/main.rs`: `taxon_id` is `Vec<String>` with `value_delimiter = ','`. Help
  text now describes the batch route.
- `crates/genomehubs-api/src/routes/summary.rs` and `summary_batch.rs`: the
  handlers have `#[deprecated]` and the description starts with "Deprecated".
- `crates/genomehubs-api/src/main.rs`: `#[allow(deprecated)]` on `main()` so
  the router's references do not fail `clippy -D warnings`.
- `docs/planning/phases/phase-14-phylopic.md`: CLI section describes single and
  multiple IDs.

## Verification

- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `cargo test -p cli-generator`: 136 unit tests and integration suites passed.
- `cargo test -p genomehubs-api`: 142 unit tests and integration suites passed.
- Smoke test against `http://localhost:3000/api`:
  - `phylopic --taxon-id 9606` returned the GET result.
  - `phylopic --taxon-id 9606,10090` and `--taxon-id 9606 --taxon-id 10090`
    both returned a `results` map keyed by taxon ID.

## Open questions

- The batch route accepts up to 200 IDs. The CLI does not check this and
  leaves the limit to the API.
- Deprecation has no replacement route named yet. The description only says
  "Deprecated".
