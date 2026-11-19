---
date: 2026-05-01
agent: GitHub Copilot
model: Claude Haiku 5.5
task: Centre the single-arc donut (2-value variant) and fix positional and CLI report defaults
files_changed:
  - crates/genomehubs-query/src/report/mod.rs
  - crates/genomehubs-api/src/routes/positional.rs
  - src/main.rs
---

## Task summary

The user asked for GoaT-style 2-value arc donuts in Vega-Lite, which previously rendered off-centre. Two small fixes were also made: a clearer positional error when the feature index is v1, and a tree default for CLI `report --taxon` so that name-only matching no longer returns zero counts.

## Key decisions

- **Decision:** Keep Vega-Lite and rebuild `vl_arc` as a layered donut: a stacked `arc` mark plus two centred `text` layers (headline percentage, `feature / reference` count).
  - **Alternative considered:** Full Vega spec. Not needed: Vega-Lite centres arcs in the view when there are no axes or legend, and the render check confirmed this.
- **Decision:** Remove the `thetaOffset`, `theta2Offset`, `theta2` value and hard-coded `shape` placeholders from `vl_arc`.
  - **Alternative considered:** Patch the offsets. Rejected because they are not valid Vega-Lite mark properties.
- **Decision:** CLI `report` defaults `taxon_filter` to `tree` when taxa are given by flag. The API default (`name`) is unchanged.

## Interaction log

| Turn | Role  | Summary                                                        |
| ---- | ----- | -------------------------------------------------------------- |
| 1    | User  | Positional error message, CLI report default, arc donut parity |
| 2    | Agent | Edited positional error and CLI report tree default            |
| 3    | Agent | Inspected arc API response and converter                       |
| 4    | Agent | Rewrote `vl_arc`, added centre label helpers and a unit test   |
| 5    | Agent | Rendered the spec with vega in a temp dir to confirm centring  |

## Changes made

- `crates/genomehubs-query/src/report/mod.rs`: rewrote `vl_arc`, added `arc_centre_labels` and `centre_text_layer`, added `arc_vega_lite_donut_has_two_segments_and_centre_labels`.
- `crates/genomehubs-api/src/routes/positional.rs`: the "no topLevel sequences" error now names the v1 feature structure and the need to rebuild the index.
- `src/main.rs`: `Commands::Report` sets `taxon_filter` to `tree` when taxa are given by flag and no filter is set.

Verification: `cargo clippy -p genomehubs-query --all-targets -- -D warnings` is clean. `cargo test -p genomehubs-query` passes. A vega render of a 600x400 view puts both arc paths at translate(300,200) with the labels centred horizontally.

## Notes / warnings

- Restart the API (`cargo run -q -p genomehubs-api`) to pick up the arc change.
- The CLI tree default has no unit test yet.
- Positional remains parked until a v2 feature index is available.
- Multi-ring and context-segment arcs still use the same converter path; only the single-arc donut was verified in a render.
