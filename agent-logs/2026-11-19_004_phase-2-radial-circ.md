---
date: 2026-11-19
agent: GitHub Copilot
model: Claude Haiku 5.5
task: Phase 2 of tree report plan v2 (circular and radial tree layouts)
files_changed:
  - crates/genomehubs-query/src/report/tree_rect.rs
  - crates/genomehubs-query/src/report/tree_polar.rs
  - crates/genomehubs-query/src/report/display.rs
  - crates/genomehubs-query/src/report/mod.rs
---

## Task summary

Phase 2 of the tree report plan. Add two polar layouts next to the rectangular
cladogram: `circ` (arc connectors, angular span configurable) and `radial` (ray
connectors, full 360 degrees). The style is chosen with `display.tree.tree_style`.
Shared geometry helpers move into `tree_rect.rs` so both layouts reuse them.

## Key decisions

- **Decision:** Polar layout lives in a new pure module, `report/tree_polar.rs`. `mod.rs` only dispatches on `tree_style`.
  **Alternative considered:** Add polar branches to `tree_rect.rs`. Rejected, because each layout is a separate concern and the file would grow past the function-size guidance.
- **Decision:** Shared helpers (`topology`, `spread_positions`, `tip_font_for_row`, `tree_vega_spec`, `tip_label`) are `pub(super)` in `tree_rect.rs`. `LayoutNode` replaces `RectNode` because it now serves both layouts.
- **Decision:** Leaves take evenly spaced angles across `total_angle`, starting at `start_angle` (default -90 degrees, i.e. 12 o'clock, clockwise). Internal nodes sit at the mid-angle of their children. Radius scales with cumulative depth, using `outer_radius` = 75% of the half-min dimension minus padding.
- **Decision:** `circ` draws the bar as an arc at each internal node's radius. The root sits at radius 0, so it gets no arc. `radial` ignores `total_angle` and draws rays from parent to child.
- **Decision:** Tip labels carry their anchor and rotation in the data. Labels on the left half of the circle are flipped 180 degrees so they read outward-to-inward without upside-down text.
- **Decision:** Tip font uses the same height-driven rule as the rectangular layout, measured along the outer arc per leaf.

## Interaction log

| Turn | Role  | Summary                                                                                           |
| ---- | ----- | ------------------------------------------------------------------------------------------------- |
| 1    | User  | Approved Phase 2 with "ok agree height is the key and simple fix so ready for phase 2"            |
| 2    | Agent | Renamed `RectNode` to `LayoutNode` and extracted shared helpers into `tree_rect.rs`               |
| 3    | Agent | Created `tree_polar.rs` with `circ_tree_vega` and `radial_tree_vega`                              |
| 4    | Agent | Added `total_angle` and `start_angle` to `TreeOptions`, and dispatch in `mod.rs`                  |
| 5    | Agent | Ran tests, fixed a test fixture (star tree has no internal node away from the centre), and clippy |
| 6    | Agent | Live-checked all three styles through the API                                                     |

## Changes made

- `report/tree_rect.rs`: `RectNode` renamed to `LayoutNode`. Added `Topology`, `topology`, `spread_positions`, `tip_font_for_row`, `tree_vega_spec`. Tips now carry `x`, `y`, `label`, `angle`, `align`.
- `report/tree_polar.rs`: new module. `circ_tree_vega`, `radial_tree_vega`, `layout_polar`, `validate_angles`, `outer_radius`, `Polar`, `polar_node`, `arm_path`, `arc_path`, `label_orientation`, and tests (7 unit, 1 proptest).
- `report/display.rs`: `TreeOptions` gains `total_angle: Option<f64>` (default 360) and `start_angle: Option<f64>` (default -90). The `tree_style` doc lists `rect`, `circ`, `radial`.
- `report/mod.rs`: `pub mod tree_polar;`. `plot_spec_to_vega_json` matches on `tree_style`. Unknown styles return `unknown tree_style: {style}`. Four dispatch tests were added.

## Verification

- `cargo test -p genomehubs-query -- tree_rect tree_polar tree_layout render_dispatch`: 33 passed.
- `cargo clippy -p genomehubs-query --all-targets -- -D warnings`: clean.
- `cargo build -q --bin plot_to_spec` and `cargo build -q`: built.
- Live check: POST to `localhost:3000/api/v3/report` with `display.tree.tree_style` set, piped through `plot_to_spec`:
  - `rect`: 206 paths, 0 arcs.
  - `circ`: 205 paths, 34 arcs (one per internal node except the root).
  - `radial`: 171 paths, 0 arcs, one ray per non-root node. No zero-length paths.
  - Tip count is 137 for all three.

## Notes / warnings

- `display` must be a top-level key in the API request body. A `display` block nested inside the `report` object is silently ignored. The CLI `report --report-yaml` path puts the report under `report`, so `tree_style` cannot be set through that path without a change to how the CLI builds the request. This is not fixed here.
- Visual review in a browser has not been done. Label legibility at high leaf counts is still limited by row spacing (same as Phase 1).
- Phase 2 log test fixtures use a nested tree (root, internal node `a` with leaves `a1`, `a2`, and leaf `b`). A star tree has no arc to draw, which is correct behaviour but not useful for testing.
