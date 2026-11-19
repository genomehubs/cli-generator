---
date: 2026-11-18
agent: GitHub Copilot
model: Claude Haiku 5.5
task: Let phylopic --batch read a bare list of taxon IDs, then add the metadata and status CLI commands with the approved flags
files_changed:
  - src/core/runtime_query.rs
  - src/commands/metadata.rs
  - src/commands/status.rs
  - src/commands/mod.rs
  - src/main.rs
  - docs/planning/phases/phase-14-phylopic.md
---

## Task summary

The user asked for `phylopic --batch` to accept a bare list file as well as
the JSON array, then for the `metadata` CLI, then approved a `status` CLI with
no flags. Each command is a thin wrapper around an existing API route. The
batch parser, metadata and status logic live in `src/core/runtime_query.rs`.
The command modules only print the JSON response.

## Key decisions

- **Decision:** a bare list is one numeric taxon ID per line. Blank lines are
  skipped. A file whose first non-space character is `[` takes the JSON path.
  Any other line that is not all ASCII digits is an error, which also rejects
  a `{...}` object.
  **Alternative considered:** accept comma-separated IDs too. Not added, since
  the user did not ask for it. This is an open question.
- **Decision:** `metadata` with no subcommand prints the aggregated
  `/v3/metadata` response. `indices`, `taxonomies`, and `ranks` each map to
  their own route. `fields --index <NAME>` sends the value as the API param
  `result`.
  **Alternative considered:** send the CLI value as `index`. Rejected because
  the API route reads `result`. Keeping the flag name `--index` matches the
  approved CLI.
- **Decision:** `status` takes no flags. It reads `/v3/status` and prints the
  `ready`, `supported`, `last_updated`, and `feature_index_structure` fields as
  JSON.
  **Alternative considered:** add an option to print only a summary. Not added,
  since the user approved the no-flags form.
- **Decision:** the CLI reuses `get_json` through `run_metadata` and `run_status`.
  The command modules hold no logic.

## Interaction log

| Turn | Role  | Summary                                                                                    |
| ---- | ----- | ------------------------------------------------------------------------------------------ |
| 1    | User  | Asked for bare-list batch input, then the metadata CLI                                     |
| 2    | Agent | Added the bare-list parser, `MetadataRequest`, `run_metadata`, and the `metadata` variants |
| 3    | Agent | Fixed a stray closing tag in `commands/metadata.rs`, then ran fmt, clippy, and tests       |
| 4    | Agent | Smoke tested metadata and bare-list batch against the local API                            |
| 5    | User  | Approved the status CLI with no flags and asked for the agent log                          |
| 6    | Agent | Added `run_status`, `commands/status.rs`, the `Status` variant, and a mockito test         |
| 7    | Agent | Verified and smoke tested `status`, then wrote this log                                    |

## Changes made

- `src/core/runtime_query.rs`: `parse_taxon_ids` dispatches on a leading `[`.
  `parse_bare_taxon_ids` reads one numeric ID per line. `MetadataRequest`,
  `metadata_endpoint`, and `run_metadata` cover the four metadata routes.
  `run_status` reads `/v3/status`. Tests cover the bare-list path, rejecting
  non-numeric lines, the endpoint mapping, the `result` mapping for `fields`,
  and the status route.
- `src/commands/metadata.rs`: prints the metadata response as pretty JSON.
- `src/commands/status.rs`: new module that prints the status response as
  pretty JSON.
- `src/commands/mod.rs`: `pub mod status;`.
- `src/main.rs`: `Commands::Metadata` with an optional `MetadataCommand`,
  `Commands::Status` with no flags, and the dispatch arms. The `--batch` help
  text mentions the bare list.
- `docs/planning/phases/phase-14-phylopic.md`: the `--batch` bullet mentions
  the bare list.

## Verification

- `cargo fmt --all`: pass.
- `cargo clippy --workspace --all-targets -- -D warnings`: pass.
- `cargo test -p cli-generator`: 133 unit tests passed, plus the integration
  suites (14 and 15 passed).
- Live smoke test against `http://localhost:3000/api`:
  - `metadata indices`, `metadata taxonomies`, `metadata ranks`, and
    `metadata fields --index assembly` each returned JSON with
    `status.success` true.
  - `metadata` with no subcommand returned the aggregated response.
  - `phylopic --batch` with the bare list `9606` and `10090` returned results
    for both IDs.
  - `status` returned `ready: true`, the supported endpoint list, and
    `feature_index_structure: "v1"`.

## Notes / warnings

- Comma-separated batch input is not supported. Ask if it is wanted.
- The `status` command shows `feature_index_structure` as `v1` on the live API.
  That is the API's reported value, not a CLI choice.
- SDK phylopic wrappers (Python, R, JS templates, and docs) are still pending a
  go-ahead. The `summary` CLI is deferred.
- Changes are uncommitted. Several files from earlier phases are also
  uncommitted.
