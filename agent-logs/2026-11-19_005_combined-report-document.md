---
date: 2026-11-19
agent: GitHub Copilot
model: Claude Haiku 5.5
task: Combined query and report YAML documents for the report command
files_changed:
  - src/core/report_document.rs
  - src/core/mod.rs
  - src/main.rs
---

## Task summary

Let `--query-yaml` or `--report-yaml` carry a single YAML document that holds the
query and params sections, the `display` settings, `include_plot_spec`, and the
report config keys. Before this change, `examples/report/tree-query.json` failed
because `split_sections` rejects top-level keys alongside `query:` and `params:`.

## Key decisions

- **Decision:** Parsing and resolution live in a new core module,
  `core/report_document.rs`. `main.rs` only calls `resolve_report_request`.
  **Alternative considered:** Change `split_query_document` in the
  genomehubs-query crate. Rejected, because that crate is shared with the API and
  the flat-query path must keep its current behaviour.
- **Decision:** Pre-extract `query`, `params`, `display`, and `include_plot_spec`
  before the query is split. Only the sections-only YAML is passed on as
  `query_yaml`, so the mixed-key rejection is never reached.
- **Decision:** A document is combined when it has a `report` key or a
  `query`/`params` section. A flat query with no `report` key stays flat, so
  flat query keys such as `rank` are not mistaken for report keys.
- **Decision:** Setting the same key in both flags is an error, not a merge:
  report config, query sections, and `display`. The `--display` flag still
  overrides YAML `display`, as before.
- **Decision:** `--report-yaml` becomes optional with
  `required_unless_present = "query_yaml"`, so clap still requires a report
  source.

## Interaction log

| Turn | Role  | Summary                                                                                 |
| ---- | ----- | --------------------------------------------------------------------------------------- |
| 1    | User  | Asked for optional top-level keys in report YAML and one file for query and report      |
| 2    | Agent | Added `core/report_document.rs` with 10 unit tests and wired `Commands::Report` in main |
| 3    | Agent | Fixed a borrow error and a test fixture that had no report keys                         |
| 4    | Agent | Ran live checks: query and report flags with the combined file produce identical output |

## Changes made

- `src/core/report_document.rs`: new module. `ReportRequest`,
  `resolve_report_request`, and the private split, resolve, and single-source helpers,
  plus 10 unit tests.
- `src/core/mod.rs`: `pub mod report_document;`.
- `src/main.rs`: imports `resolve_report_request`. `report_yaml` is now
  `Option<String>`. The Report arm resolves the request, sets `query_yaml` and
  `include_plot_spec`, and falls back to YAML `display`. The positional arm is unchanged.

## Verification

- `cargo test -q --lib report_document`: 10 passed.
- `cargo clippy -q --all-targets -- -D warnings`: clean.
- Live check, POST via the CLI to `localhost:3000/api/`, piped through `plot_to_spec`:
  - `--query-yaml examples/report/tree-query.json --report-yaml examples/report/tree.json`: 25193-byte spec.
  - `--report-yaml combined.yaml` alone: same output.
  - `--query-yaml combined.yaml` alone: same output.
  - `--query-yaml examples/report/tree-query.json` alone: error "no report config".

## Notes / warnings

- `examples/report/tree-query.json` has no `report:` key, so it still needs
  `--report-yaml` unless the user adds the key.
- Combined documents must nest query fields under `query:`. Flat query keys
  alongside `report:` are not supported.
- `search` and `count` still reject `display` keys. Only `report` accepts them.
