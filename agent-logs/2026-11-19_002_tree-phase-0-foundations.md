---
date: 2026-11-19
agent: GitHub Copilot
model: Claude Haiku 5.5
task: Phase 0 of tree report plan v2 (foundations, no visual change)
files_changed:
  - src/bin/plot_to_spec.rs
  - scripts/generate_reports.sh
  - crates/genomehubs-query/src/lib.rs
  - crates/genomehubs-query/src/report/mod.rs
  - crates/genomehubs-query/src/report/tree_layout.rs
---

## Task summary

Start Phase 0 of the tree report plan. Rename the spec-conversion bin, route tree
reports to a Vega v5 output instead of the Vega-Lite placeholder, add the
`TreeLayoutNode` core input type with branch-length depths, and decide on the
`--vega` spike. The API scatter e2e test was rerun after the restart and passed.

## Key decisions

- **Decision:** Tree reports route to `plot_spec_to_vega_json` (Vega v5) through `plot_spec_to_render_json`. All other reports stay on Vega-Lite v6.
  **Alternative considered:** Keep the Vega-Lite placeholder for tree. Rejected, because the plan requires tree output to move to Vega v5.
- **Decision:** The Vega v5 tree output is an empty placeholder sized from `display`. No marks yet, so there is no visual change.
- **Decision:** `TreeLayoutNode` and `cumulative_depths` live in `report/tree_layout.rs`. The root's own `branch_length` is validated but not counted in depth. A missing length counts as 1.0, so all-default trees give hop depth.
- **Decision:** `--vega` spike deferred. `vl-convert` is not in `Cargo.lock`. Adding it means a V8-based dependency in the root `Cargo.toml`, which AGENTS.md says needs user approval.
  **Alternative considered:** Add the dependency and measure build cost. Not done, to avoid an unapproved change to shared infrastructure.
- **Decision:** The tree stub arm was removed from the Vega-Lite match. Tree input to `plot_spec_to_vega_lite_json` now returns the base spec with no mark.

## Interaction log

| Turn | Role  | Summary                                                             |
| ---- | ----- | ------------------------------------------------------------------- |
| 1    | User  | Restarted API; asked to check the scatter matrix and start Phase 0  |
| 2    | Agent | Ran focused scatter e2e test (passed, 84.8s and 166.9s runs)        |
| 3    | Agent | Renamed bin, added Vega v5 dispatch and TreeLayoutNode, added tests |
| 4    | Agent | Deferred the `--vega` spike, pending approval of the new dependency |

## Changes made

- `src/bin/plot_to_vl.rs` renamed to `src/bin/plot_to_spec.rs`. It now calls `plot_spec_to_render_json`.
- `scripts/generate_reports.sh`: all four `--bin plot_to_vl` references changed to `--bin plot_to_spec`.
- `report/mod.rs`: added `plot_spec_to_render_json`, `plot_spec_to_vega_json`, and helpers `plot_spec_value` and `report_type_of`. Removed the tree stub arm. Refactored the Vega-Lite envelope check to use `plot_spec_value`. Added three dispatch tests.
- `report/tree_layout.rs`: new module with `TreeLayoutNode` and `cumulative_depths`. Seven tests, including a proptest for the all-default hop-depth invariant.
- `lib.rs`: exposes `plot_spec_to_render_json` at crate root.

Verification: `cargo test -p genomehubs-query tree_layout` (7 passed), `render_dispatch` (3 passed), `cargo clippy --all-targets -- -D warnings` clean, `cargo build --bin plot_to_spec` builds and emits valid Vega v5 for a tree spec.

## Notes / warnings

- The API does not call `validate_report_yaml`. Flat `y:` and `fields:` on tree requests are accepted by the API. The validator only runs from the `genomehubs-query` wrapper and its tests. Wiring it into the API route is a follow-up that needs a decision.
- `TreeLayoutNode` is not yet used by the API. The `branch_length` source is deferred to a later phase.
- The `--vega` spike is deferred. Record the decision before Phase 1 starts.
