---
date: 2026-11-20
agent: GitHub Copilot
model: Claude Haiku 5.5
task: Add label orientation and font size options to tree reports, and refine ring labels and borders
files_changed:
  - crates/genomehubs-query/src/report/tree_layout.rs
  - crates/genomehubs-query/src/report/tree_rect.rs
  - crates/genomehubs-query/src/report/tree_polar.rs
  - crates/genomehubs-query/src/report/tree_ring.rs
  - crates/genomehubs-query/src/report/mod.rs
  - crates/genomehubs-query/src/report/display.rs
---

## Task summary

The user asked for several tree-report changes. The ring `total_angle` now applies, and all circular variants get a `label_orientation` option. All tree styles get a `font_size` option, which is absolute or proportional to the canvas. Ring labels use one font size across all nodes. Wide arcs get curved labels centred on the arc. Radial ring labels are right aligned at the outer edge. Ring arc borders are half their previous thickness (0.5 px).

## Key decisions

- **Decision:** `label_orientation` takes `"flip"` (default, the existing left-half flip) or `"continuous"` (labels read outward all the way round). It applies to `circ`, `radial`, and `ring`.
  **Alternative considered:** a separate option per style. Rejected because one shared option keeps the API small.
- **Decision:** continuous radial labels on the left half are right aligned at the outer edge for ring, and keep the rotation for circ and radial.
  **Alternative considered:** left aligning all labels. Rejected because the reference shows labels ending at the outer edge.
- **Decision:** in flip mode, ring labels on the left half use `align "left"` at the outer edge, so the text stays inside the sector.
- **Decision:** curved ring labels are used when an arc is wider than it is tall (`arc_px > radial_px`). They are drawn one row per glyph at mid radius, each rotated to the arc tangent.
  **Alternative considered:** Vega text-on-path. Not available in Vega, so per-glyph rows are used.
- **Decision:** ring font size is fixed at `DEFAULT_RING_FONT_PX` (10 px) across all sectors unless `font_size` is set. Labels are skipped when the font exceeds 80% of the arc width, and truncated with an ellipsis when fewer than 3 characters fit.
- **Decision:** `font_size` (absolute pixels) and `font_scale` (share of the shorter canvas side) are both accepted, but not together. Supplying both returns an error.
- **Decision:** `total_angle` applies to `ring` as well as `circ`. `start_angle` continues to apply to `circ`, `radial`, and `ring`. `tip_alignment` still does not apply to `ring`.
- **Decision:** `TreeRenderOptions` bundles `alignment`, `label_orientation`, and `font_size`. This keeps renderer signatures within clippy's 7-argument limit.
- **Decision:** ring arc stroke is `RING_BORDER_PX = 0.5`, half the earlier width.
- **Decision:** `circ` and `ring` leave a centre hole of `CENTRE_HOLE_PX = 12.0` px to match the reference image. `radial` keeps a hole of 0. The circ hole is set by `PolarSweep.hole_px`, which replaces the scalar arguments of `layout_polar` and keeps it within clippy's 7-argument limit.
  **Alternative considered:** a user-configurable hole size. Rejected because the reference fixes one size and the rest of the API stays small.
- **Decision:** right-aligned radial ring labels stop one glyph (`GLYPH_WIDTH_EM * font_px`) inside the outer arc edge, on top of `RING_LABEL_PAD_PX`. Text is truncated so it does not run past the inner edge.

## Interaction log

| Turn | Role  | Summary                                                                                                                                        |
| ---- | ----- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| 1    | User  | Requested ring total_angle, continuous label orientation, font size options, curved and right-aligned ring labels, and half-width ring borders |
| 2    | Agent | Added `TreeRenderOptions`, `LabelOrientation`, and `resolve_font_size` in `tree_layout.rs`                                                     |
| 3    | Agent | Updated `tree_rect.rs`, `tree_polar.rs`, and `tree_ring.rs` to use the shared options; rewrote ring labels                                     |
| 4    | Agent | Read `font_size`, `font_scale`, `label_orientation`, and `total_angle` for ring in `mod.rs`; added dispatch tests                              |
| 5    | Agent | Added `label_orientation`, `font_size`, and `font_scale` to `TreeOptions` with a round-trip test                                               |
| 6    | Agent | Ran fmt, clippy `-D warnings`, and tests                                                                                                       |
| 7    | User  | Asked for a small centre hole on ring and circ to match the reference, and one glyph of right padding on right-aligned ring labels             |
| 8    | Agent | Added `CENTRE_HOLE_PX`, `PolarSweep.hole_px`, `radial_outer_gap`, and the matching tests; rendered ring and circ live                          |

## Changes made

- `tree_layout.rs`: `LabelOrientation` (`Flip` default, `Continuous`, `parse`), `TreeRenderOptions { alignment, label_orientation, font_size }`, `with_alignment`, and `resolve_font_size`, with tests.
- `tree_rect.rs`: `rect_tree_vega` takes `TreeRenderOptions`. Tip font comes from `resolve_font_size`.
- `tree_polar.rs`: `circ_tree_vega`, `radial_tree_vega`, and `polar_tree_vega` take `TreeRenderOptions`. `label_orientation` takes `LabelOrientation`. `validate_angles` is shared. Continuous mode returns angle modulo 360 with `"left"` alignment.
- `tree_ring.rs`: `ring_tree_vega` takes `total_angle`, `start_angle`, and `TreeRenderOptions`. Adds `sector_size`, `sector_label_rows`, `radial_label_rows`, `curved_label_rows`, `radial_orientation`, `reading_direction`, and `truncate_label`. Stroke width is 0.5.
- `mod.rs`: `tree_render_options` reads `tip_alignment`, `label_orientation`, `font_size`, and `font_scale`. Dispatch passes one options value to each renderer. Adds dispatch tests for font size, font scale, the mutual exclusion error, label orientation validation and acceptance, and ring `total_angle`.
- `display.rs`: `TreeOptions` gains `label_orientation`, `font_size`, and `font_scale`, with docs. The `total_angle` and `tip_alignment` docs are corrected. Adds a round-trip test.

## Verification

- `cargo fmt --all`: applied.
- Centre-hole phase: `cargo clippy -q -p genomehubs-query --all-targets -- -D warnings` clean; `cargo test -q -p genomehubs-query` 510 passed.
- Centre-hole phase: live PNG renders of ring (180°, continuous, font 8) and circ (360°) show the centre hole.
- `cargo clippy -q -p genomehubs-query --all-targets -- -D warnings`: clean.
- `cargo test -q -p genomehubs-query`: 508 passed.
- Live spec for the Canidae example with `tree_style: ring`, `total_angle: 180`, `label_orientation: continuous`, `font_size: 8` (API on localhost:3000, not restarted):
  - 0 errors against the Vega v6 JSON schema. Vega compiles.
  - Labels render at 8 px. All labels are right aligned radially (continuous mode).
  - Arc borders have `strokeWidth` 0.5.
  - The 180° sweep renders on the right half.
  - No sector in this sample is wider than tall, so no curved labels were produced. The curved path is covered by unit tests only.

## Notes / warnings

- Curved labels are not exercised visually. A tree with a shallow root fan (for example, two hop-1 children spanning 180°) is needed to check them.
- The running API was not restarted. A restart is needed before the new options are available through the HTTP API.
- `examples/report/tree-query.json` was not edited.
- Curved labels use one Vega row per glyph. Long labels on wide arcs create many rows.
- Not committed. Not pushed.
