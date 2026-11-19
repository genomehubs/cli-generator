---
date: 2026-05-02
agent: GitHub Copilot
model: Claude Haiku 5.5
task: Change the 3-value arc donut so the centre shows the cumulative share and an inner thin arc highlights it; extend to a multi-ring rainbow comparing ranks
files_changed:
  - crates/genomehubs-query/src/report/mod.rs
  - crates/genomehubs-api/src/report/arc.rs
---

## Task summary

For the 3-value arc donut (feature, reference-only, context-only), the centre showed feature/reference rather than the cumulative share of context. The user asked for the centre to show feature plus reference relative to context, with a thin inner arc highlighting that cumulative share while the outer ring shows individual category percentages.

## Key decisions

- **Decision:** Centre headline in the 3-value case is `reference_count / context_count`, since the reference count already includes the feature count.
  - **Alternative considered:** Sum feature and reference-only counts explicitly. Rejected because the API `reference_count` already equals the cumulative total.
- **Decision:** The cumulative arc is a separate `arc` layer using raw radians (`"scale": null`) with `theta` and `theta2`, drawn just inside the donut hole (`inner_radius - 7` to `inner_radius - 4`).
  - **Alternative considered:** Reuse the stacked `portion` encoding. Rejected because it cannot draw an independent overlay sweep.
- **Decision:** Thousands separators are applied to centre counts in both the 2-value and 3-value layouts.
  - **Impact:** The 2-value detail label changed from `1538 / 10384` to `1,538 / 10,384`. The existing test was updated.

## Interaction log

| Turn | Role  | Summary                                                                                |
| ---- | ----- | -------------------------------------------------------------------------------------- |
| 1    | User  | Requested the 3-value layout: cumulative centre and inner thin arc                     |
| 2    | Agent | Refactored `arc_values_from_report_data`, `vl_arc`, and added the cumulative helpers   |
| 3    | Agent | Added a 3-value unit test; ran fmt, tests, clippy                                      |
| 4    | Agent | Rendered the spec with Vega in a scratch directory and checked the geometry            |
| 5    | User  | Reported anticlockwise outer segments that did not align with the inner arc            |
| 6    | Agent | Switched segments to explicit `start`/`end` angles; added in-segment percentage labels |
| 7    | User  | Asked for styling to match the reference screenshot (ring thickness and font sizes)    |
| 8    | Agent | Set inner ratio to 0.5; scaled fonts to the outer radius; regular weight               |

## Changes made

- `crates/genomehubs-query/src/report/mod.rs`:
  - `arc_values_from_report_data` uses the module-level `count_field` helper.
  - `vl_arc` adds `cumulative_arc_layer` when `arc_cumulative_fraction` returns `Some`.
  - New helpers: `arc_cumulative_fraction`, `cumulative_arc_layer`, `arc_centre_labels`, `count_field`, `format_count`.
  - Tests: updated `arc_vega_lite_donut_has_two_segments_and_centre_labels`; added `arc_vega_lite_three_value_donut_centres_reference_share_of_context`.

Verification:

- `cargo fmt --all` applied.
- `cargo test -p genomehubs-query arc` passes (14 tests).
- `cargo clippy -p genomehubs-query --all-targets -- -D warnings` is clean.
- Rendered the live spec for `examples/report/arc.json` (3-value, context `""`) with Vega in a scratch directory. The thin arc sits inside the donut hole, starts at 12 o'clock, and sweeps clockwise to 9.2%. The centre shows `9.2%` and `1,155 / 12,583`. Outer segments are feature 8.0%, reference-only 1.2%, and context-only 90.8%.

## Later changes

- **Explicit angles:** Vega-Lite's automatic `stack` on `theta` reordered segments, so the outer ring ran anticlockwise. `arc_segment_angles` now writes cumulative `start`, `end`, and `mid` radians with `"scale": null`, so segments run clockwise from 12 o'clock and line up with the inner arc.
- **Segment labels:** `segment_label_layer` draws `{:.1}%` at `mid_radius`. `arc_segment_label_values` keeps only segments whose arc length fits the text (`ARC_LABEL_CHAR_EM` = 0.6 em per character).
- **Styling to match reference:** inner radius is 0.5 of the outer radius (was 0.6). Font sizes scale with the outer radius: headline 0.20, detail 0.10, segment labels 0.12. Headline, detail, and segment labels use regular weight.

## Rainbow and per-rank context

- **Per-rank context (API):** `arc.rs` `per_rank_queries` now adds a context query for each rank when `context` is set. `per_rank_stride` returns 2 or 3 counts per rank, and `per_rank_ring_entry` adds `arc2`, `context_count`, and `contextTerm` only when context is set. Previously the `ranks` path silently ignored `context`.
- **Multi-ring renderer (genomehubs-query):** `vl_arc` dispatches to `vl_arc_rings` when `data.arc` is an array. Ring 0 (first entry in `ranks`) is the outermost ring. Each ring draws a grey track and a feature arc, with segment labels after all arcs.
- **Colours:** Feature layers use a named Vega-Lite scheme from `RING_COLOUR_SCHEME` (`category10`), with an explicit `scale.domain` on each layer so the shared colour scale stays consistent. Change the constant to recolour.
- **Ring geometry:** `ring_pitch` derives the band from the ring count and `RING_GAP_RATIO` (see the layout fixes section). `RING_TRACK_FILL` sets the track colour.
- **Known gap:** The ring renderer draws feature share only. The per-ring `arc2` (reference or context) is not drawn yet.

## Semicircle rainbow, legend, labels, and outlines

- **Semicircle layout:** `vl_arc_rings` draws a 180° semicircle (`semicircle_angle`, from -π/2 to π/2). Each ring is a grey track, a reference band (`RING_REFERENCE_OPACITY`), and a feature arc, so the arcs reach the centre with a small hole (`RING_HOLE_RATIO`).
- **Shares from counts:** `ring_fractions` uses `context_count` when present, so the figures match the reference screenshot's context-based shares rather than the stored `arc` field.
- **Percentage labels:** reference share labels sit in each band. Text marks need a data source, so `positioned_text_layer` uses `"data": {"values": [{}]}`. Labels are white (`#ffffff`) and sized to the band (see the layout fixes section).
- **Legend:** three layers under the arcs: coloured bars, bold rank names, and grey counts (`format_count_compact`, three significant figures). Vega-Lite cannot mix bold and colour within one text mark, so the counts line is uniform grey.
- **Outlines:** `RING_OUTLINE_WIDTH` (2.0) is applied to every arc in `ring_arc_layer`. Track and reference strokes match their fill, and the feature stroke uses the scheme colour, so partial segments line up with the rest of the arc.

## Layout fixes for many rings

- **Dynamic thickness:** `ring_pitch(R, n) = R * (1 - RING_HOLE_RATIO) / (n - RING_GAP_RATIO)`. Each band is `pitch * (1 - RING_GAP_RATIO)`, so a small gap separates arcs for any ring count.
- **Label font:** reference share labels use `band * 0.4`, so they scale with arc thickness.
- **Zero-width arcs:** the feature arc is drawn only when `feature > 0`, and the reference band only when `reference > feature`. A stroked zero-length arc otherwise draws a visible line.
- **Legend wrap:** each item's width comes from its own name and counts (`ring_legend_item_em`). Items fill a grid of `columns` row by row, and the last row is right-packed (`ring_legend_column`). `ring_legend_column_em` gives each column the width of its widest item, so wrapped rows align. `ring_layout` picks the most columns that fit left of the arc's right edge (`width / 2 + R`). The grid is right-anchored so its rightmost bar meets the outer arc. Each item has a right-aligned name and counts, with the text block centred on its bar (alphabetic baseline, `RING_LEGEND_CAP_EM`).
- **Legend bars:** width doubled (`RING_LEGEND_BAR_RATIO` 0.024).
- **Verification:** 15 arc tests pass and clippy is clean. Renders checked for a 3-rank case (one legend row) and a 6-rank case (2×3 legend grid, no overlap).

## Notes / warnings

- The render was checked with a scratch Vega install, not inside VS Code's Vega-Lite preview.
- The rainbow render was checked with `/tmp/vegarender/render.mjs` for the 3-value case (`species`, `genus`, `family`). Species is the outer ring and the colours come from `category10`.
- Verification: `cargo test -p genomehubs-query arc` (15 passed), `cargo test -p genomehubs-api arc` (20 passed), and clippy clean for both crates.
- The 2-value label now uses thousands separators.
- Open items for the user: the screenshot shows family outermost, but the agreed order is first-named (species) outermost. The screenshot's feature counts also differ from the live feature filter, so it likely used a different feature term. Single-donut percentages still use one decimal; the screenshot uses whole numbers.
- `arc_centre_labels` in the 2-value case still shows feature/reference, which is unchanged.
