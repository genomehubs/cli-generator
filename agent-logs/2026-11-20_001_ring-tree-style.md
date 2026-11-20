---
date: 2026-11-20
agent: GitHub Copilot
model: Claude Haiku 5.5
task: Add a ring (sunburst-style) tree layout with labels inside the arcs
files_changed:
  - crates/genomehubs-query/src/report/tree_ring.rs
  - crates/genomehubs-query/src/report/tree_polar.rs
  - crates/genomehubs-query/src/report/mod.rs
  - crates/genomehubs-query/src/report/display.rs
---

## Task summary

The user asked for a ring tree style after the Vega v6 move. Each taxon is drawn as an annular sector, and labels sit inside the arcs, truncated to fit or omitted. This matches the reference image, where labels run along the radius and are white on grey arcs. The style is selected with `display.tree.tree_style: "ring"`.

## Key decisions

- **Decision:** ring by hop count from the root, with equal ring thickness. The root is not drawn.
  **Alternative considered:** rings sized by branch length. Rejected because the reference shows equal rings, and branch lengths are not needed for the ring view.
- **Decision:** leaves share the full sweep evenly and reach the outer edge. Internal nodes span all of their leaves.
- **Decision:** `tip_alignment` and `total_angle` are ignored for `"ring"`, and this is documented in `TreeOptions`. Leaves already reach the outer edge, so alignment does not apply.
  **Alternative considered:** erroring on these options. Rejected to match the existing "ignored for radial" behaviour of `total_angle`.
- **Decision:** labels run radially and reuse `label_orientation` from `tree_polar.rs`, which is now `pub(super)`. Labels are truncated with an ellipsis to the ring thickness, and omitted when the arc is too narrow (font under 5 px) or the ring holds fewer than 3 characters.
  **Alternative considered:** wrapping labels across lines. Rejected as too cluttered for narrow arcs.
- **Decision:** no leaders dataset in the ring spec. The dispatch "every style" test still covers only rect, circ, and radial.

## Interaction log

| Turn | Role  | Summary                                                                          |
| ---- | ----- | -------------------------------------------------------------------------------- |
| 1    | User  | Asked for ring trees with labels inside the arcs, matching the reference image   |
| 2    | Agent | Added `tree_ring.rs` (layout, arc paths, label fitting, Vega spec) and its tests |
| 3    | Agent | Wired `"ring"` into `plot_spec_to_vega_json`, updated docs, added dispatch tests |
| 4    | Agent | Ran fmt, clippy `-D warnings`, and tests (488 passed)                            |
| 5    | Agent | Live check on the Canidae example copied to `/tmp` with `tree_style: ring`       |

## Changes made

- `tree_ring.rs` (new): `RingArc`, `ring_tree_vega`, `layout_ring`, `hop_levels`, `leaf_spans`, `ring_outer_radius`, `arc_path`, `ring_labels`, `fit_label`, `ring_vega_spec`, and tests (including a proptest for radii and angle order).
- `tree_polar.rs`: `label_orientation` changed from private to `pub(super)`.
- `mod.rs`: `pub mod tree_ring;`, a `"ring"` match arm, updated `plot_spec_to_vega_json` docs, and three ring dispatch tests.
- `display.rs`: `TreeOptions` docs for `tree_style`, `total_angle`, `start_angle`, and `tip_alignment`.

## Verification

- `cargo fmt --all`, `cargo clippy -q -p genomehubs-query --all-targets -- -D warnings`: clean.
- `cargo test -q -p genomehubs-query`: 488 passed (474 before, plus 14 new).
- Live spec for the Canidae example with `tree_style: ring` (API on localhost:3000, not restarted):
  - 171 arc paths and 169 label rows.
  - 0 errors against the Vega v6 JSON schema.
  - Vega 6 render: 169 text elements and 173 path elements.
  - `vl_convert` PNG render checked visually: radial labels inside arcs, truncated or omitted where they do not fit.

## Notes / warnings

- The user's `examples/report/tree-query.json` was not edited. It still uses `circ`.
- The running API was not restarted. The ring style was checked by piping the report data through `plot_to_spec`, which renders locally. A restart is needed before the ring style is available through the HTTP API.
- Label font size varies per arc (5 to 10 px), so labels in small arcs can look smaller than those in larger arcs. A uniform size is an option if the reference needs it.
- Not committed. Not pushed.
