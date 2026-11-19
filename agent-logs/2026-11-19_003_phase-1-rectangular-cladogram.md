---
date: 2026-11-19
agent: GitHub Copilot
model: Claude Haiku 5.5
task: Phase 1 of tree report plan v2 (rectangular cladogram with branch lengths)
files_changed:
  - crates/genomehubs-query/src/report/tree_rect.rs
  - crates/genomehubs-query/src/report/mod.rs
---

## Task summary

Phase 1 of the tree report plan. Lay out `treeNodes` as a rectangular cladogram in
pixel coordinates and emit Vega v5 marks: elbow connectors as `links` and tip labels
as `tips`. Horizontal position is cumulative branch length, so default lengths give
hop depth. The live Canidae example renders.

## Key decisions

- **Decision:** Layout lives in a new pure module, `report/tree_rect.rs`. It has no Vega dependency beyond building JSON. `mod.rs` only wires `rect_tree_vega` into `plot_spec_to_vega_json`.
  **Alternative considered:** Put the layout in `mod.rs`. Rejected, because the core rule is that logic lives in the core module and the wiring is thin.
- **Decision:** Parents come from each node's `children` map, since the API payload has no parent pointer. An optional numeric `branch_length` on a node is used as its edge length.
- **Decision:** Tips are placed evenly down the plot in depth-first order, with children sorted by `taxon_id` for determinism. Internal nodes sit at the midpoint of their first and last child.
- **Decision:** Tip font size shrinks with leaf count, clamped to 6–12 px.
- **Decision:** Layout returns an error for multiple roots or invalid branch lengths. It does not guess a root.

## Interaction log

| Turn | Role  | Summary                                                                |
| ---- | ----- | ---------------------------------------------------------------------- |
| 1    | User  | Approved Phase 1 with "ok to proceed"                                  |
| 2    | Agent | Read the tree data contract (`treeNodes`, `spec_builder`, `plot_spec`) |
| 3    | Agent | Created `tree_rect.rs` and wired it into `report/mod.rs`               |
| 4    | Agent | Ran focused tests (21 passed), clippy (clean), and a live API render   |

## Changes made

- `report/tree_rect.rs`: new module with `RectNode`, `layout_rectangular`, `tip_font_size`, `layout_nodes_from_tree_nodes`, and `rect_tree_vega`. Private helpers: `children_by_parent`, `find_root`, `depth_first_order`, `horizontal_positions`, `vertical_positions`, `rect_node`, `tip_label`.
- `report/mod.rs`: declared `pub mod tree_rect;`. `plot_spec_to_vega_json` now reads `display` and `data.treeNodes` and calls `rect_tree_vega`. The empty placeholder is removed. The error branches are kept.

## Verification

- `cargo test -p genomehubs-query -- tree_rect render_dispatch tree_layout`: 21 passed. This covers the empty, single, chain, internal-range, order-independence, branch-length, multi-root, font-size, Vega emission, and proptest leaf-row cases.
- `cargo clippy -p genomehubs-query --all-targets -- -D warnings`: clean.
- `cargo build --bin plot_to_spec`: builds.
- Live check: `examples/report/tree.json` POSTed to `localhost:3000/api/v3/report` with `include_plot_spec: true` returned 200. The plot spec piped through `plot_to_spec` produced Vega v5 with 206 link paths and one labelled tip per leaf (172 tips, scientific names).

## Notes / warnings

- The API does not call `validate_report_yaml`. Flat `y:` and `fields:` on tree requests still succeed silently. This is deferred by the user.
- No `title` appears in the live output because the request sets no `display.title`.
- Visual review in a browser has not been done.
- Tip label font now falls to 1 px minimum when rows are tight, so labels do not overlap. At 137 leaves in a default 400 px plot the font is about 2.2 px, which is barely legible. Taller plots are the practical fix.
