# Phase XX: Report Styling and Format Parity

**Status:** Design
**Depends on:** Phase 12 (PlotSpec), Phase 13 (plot/local-report subcommands)
**Blocks:** Phase 17 (report gallery in docs) — partially; does not block any other active phase
**Estimated scope:** `src/report/mod.rs` rework (~200 lines), new `src/report/vl_defaults.rs` module, validation additions in `src/commands/report.rs`
**Deferred:** Yes — does not block the main phased development cycle

---

## 1. Motivation

The current `plot_spec_to_vega_lite_json()` produces functional but visually
bare output. Comparing with the equivalent GoaT UI report reveals several
categories of divergence:

| Issue                    | Current CLI output               | GoaT UI                                       |
| ------------------------ | -------------------------------- | --------------------------------------------- |
| Colour palette           | Vega-Lite default (#4c78a8 blue) | Viridis (purple → yellow)                     |
| Legend                   | Absent                           | Shows series name, `n=N [min–max]` summary    |
| Histogram bar width      | 5px fixed                        | Spans full bin width (bucket step)            |
| Histogram bar position   | X = bucket midpoint              | X = bucket left edge, width = step            |
| Tick marks               | Vega-Lite auto                   | Aligned to bucket boundaries                  |
| Y-axis label             | Hardcoded "Count"                | "Count of {rank}" (e.g. "Count of species")   |
| Axis number format       | Raw integers (1,000,000,000)     | SI suffix (1G, 2.5G)                          |
| Background               | White                            | White with subtle dashed grid                 |
| Tree                     | Unsupported — empty point spec   | Radial/rectangular phylogenetic tree          |
| Map                      | Skeleton projection only         | Hex-bin choropleth with Leaflet               |
| Arc                      | Skeleton mark only               | Chord diagram                                 |
| Rank requirement         | Not enforced                     | Histogram/scatter/map always scoped to a rank |
| Data format alternatives | JSON only                        | —                                             |

Additionally, the `plot` subcommand currently has no non-visual output formats
(TSV summary of histogram bins, Newick for trees). These are useful for
downstream analysis without a graphics dependency.

---

## 2. Scope

This phase covers:

1. **Styling defaults** — viridis palette, legend with summary counts, SI axis
   labels, correct histogram bar geometry, grid style
2. **Rank enforcement** — error/fallback when `histogram`, `scatter`, or `map`
   is requested on the `taxon` index without a rank
3. **Non-visual output formats** — `--format tsv` (histogram/scatter), `--format newick`
   (tree), as alternatives to `--format svg/png/json`
4. **Render coverage audit** — document what Vega-Lite can and cannot do for
   each report type; identify where alternatives are required
5. **Per-report-type defaults** — a `vl_defaults` module that encodes
   GoaT-matching defaults per `PlotReportType`, consumed by `plot_spec_to_vega_lite_json`

Out of scope for this phase:

- Full interactive tree rendering (requires a JS tree library; blocked on separate
  investigation — see §6)
- Full hex-bin map rendering (requires geojson data embedded in PlotSpec)
- Arc/chord diagrams

---

## 3. Render coverage audit

### 3.1 Histogram

**Current gaps:**

- Bar width is 5px regardless of bin step. Must be `"band"` or explicit `"binStep"`.
- Bar x-position uses bucket midpoint. GoaT positions bars at the left bin edge
  (`bin.extent` in Vega-Lite: `{field: "key", bin: {step: …}}`).
- Tick marks do not align with bin boundaries.
- No legend.
- Y-axis title is hardcoded "Count", not "Count of {rank}".
- Axis labels use raw numbers; GoaT uses SI suffix formatting.

**Fix:** Use Vega-Lite `bin: {binned: true}` with `field: "key"` (bin start) and
`field2: "key2"` (bin end), where `key2 = key + step`. This requires either:
(a) computing `key2` server-side and emitting it in `PlotSpec.data.buckets`, or
(b) computing it client-side in `vl_histogram()` from the `step` in `AxisMeta`.

Preferred: option (b) — `AxisMeta` already carries `domain` and bucket count;
derive `step = (domain[1] - domain[0]) / bucket_count`. Emit both `key` and `key2`
in the values array inside `vl_histogram()`.

**Newick / TSV alternative:** histogram data is a simple `[(bin_start, count)]`
array — trivially serialisable as TSV.

### 3.2 Scatter

**Current gaps:**

- No colour encoding (GoaT colours by `cat` field when present).
- No legend.
- Axis labels use raw numbers.
- Point opacity/size not set.

**Fix:** Add `"color"` encoding when a `cat` axis is present in `PlotSpec`. Use
`"opacity": 0.6`, `"size": 30` defaults. Apply viridis for continuous cat; nominal
colour scale for discrete cat.

**TSV alternative:** scatter data is `[(x, y, cat?)]` — trivially TSV.

### 3.3 Tree

**Current state:** Emits `{"mark": "point", "data": {"values": []}}` — a no-op.

**Vega-Lite support:** Vega-Lite has no native tree/dendrogram mark. Options:

| Option                                       | Effort                                      | Result                                   |
| -------------------------------------------- | ------------------------------------------- | ---------------------------------------- |
| Vega (not Vega-Lite) treeLayout transform    | High — requires switching to full Vega spec | Full phylogenetic tree                   |
| D3 in a custom JS layer                      | Very high                                   | Full tree                                |
| Reingold-Tilford via Vega-Lite layered marks | Medium                                      | Works for small trees                    |
| **Newick text output**                       | Low                                         | Downstream tools (iTOL, FigTree, ggtree) |
| ASCII tree via Rust                          | Low                                         | Useful for terminal inspection           |

**Recommendation for this phase:** Emit Newick as `--format newick` output.
Add a stub Vega-Lite spec that renders the tree as a Reingold-Tilford layout
using Vega's `treeLayout` transform — output as a **Vega** spec (not Vega-Lite)
so `vl-convert` can still render it via `vl2svg`/`vl2png` in its Vega mode.
Full Vega spec support in `vl_convert_render()` requires passing `--format vg`
to the `vl-convert` binary or calling `vlc.vega_to_svg()` in Python.

**Newick:** `PlotSpec.data` for tree reports contains a nested node structure;
a recursive Rust function can serialise to Newick. This is the highest-value
low-effort deliverable for trees.

### 3.4 Map

**Current state:** Sets `"projection"` only — no data, no layer.

**Vega-Lite support:** Vega-Lite supports geoshape marks and choropleth via
`topojson`. Hex-bin maps require either:

- Geojson of hex centroids (must come from PlotSpec or be computed client-side)
- A Vega-Lite `geoshape` layer with colour encoding

**Recommendation for this phase:** Emit a minimal `geoshape` choropleth from
`PlotSpec.data.hexes` if present. Document hex-bin map as deferred. SVG/PNG
output will work once data is present.

### 3.5 CountPerRank / Sources (bar charts)

**Current gaps:**

- Orientation may be wrong (currently horizontal, GoaT is vertical for countPerRank).
- No colour.
- Labels may overflow.

**Fix:** Use vertical bars for `countPerRank` (ranks on x-axis, count on y-axis),
horizontal for `sources` (source name on y-axis). Apply viridis.

### 3.6 Arc

**Current state:** Emits `{"mark": "arc"}` — Vega-Lite pie/donut only, not a
chord diagram.

**Vega-Lite support:** Vega-Lite `arc` mark = pie/donut. A proper chord diagram
requires a custom Vega spec or a D3 layer. GoaT's arc report is a chord diagram.

**Recommendation for this phase:** Emit a Vega-Lite pie/donut as the default
`--format svg/png` approximation. Document chord diagram as requiring full Vega.
Add `--format tsv` which emits the adjacency matrix.

---

## 4. Rank enforcement

Histogram, scatter, and map reports on the `taxon` index aggregate across all
taxa at every rank simultaneously unless a `rank` is specified. This produces
double-counting (a genus is counted in addition to its species). GoaT always
scopes these report types to a single rank.

### Rule

In the report request builder (both `client.rs.tera` and `ReportOptions`), add
a validation step:

```
if report_type in {histogram, scatter, map}
   AND index == taxon
   AND rank is None:
     if index default_rank is configured in site config: use it
     else: error "rank required for histogram/scatter/map on taxon index"
```

### Touch-points

| Location                                                 | Change                                                |
| -------------------------------------------------------- | ----------------------------------------------------- |
| `templates/rust/client.rs.tera` `report()`               | Pre-flight check before POST                          |
| `templates/rust/main.rs.tera` `Commands::Report` handler | Validate `rank` is set for these types on taxon index |
| `python/cli_generator/query.py` `report()`               | Python-side check                                     |
| `templates/python/query.py.tera`                         | Mirror                                                |
| `src/core/report/mod.rs` (optional)                      | Add `ReportType::requires_rank(index)` helper         |

**Site config fallback:** `sites/goat.yaml` already has a `default_rank: species`
field. The generator should pass this as the fallback rank when the user omits it.

---

## 5. Styling defaults

### 5.1 Colour palette

GoaT uses the Viridis palette throughout. Vega-Lite supports this via:

```json
"scale": {"scheme": "viridis"}
```

For single-series histograms (no `cat`), use the darkest viridis colour
(`#440154`) as a fixed fill — matching the GoaT default.

### 5.2 Legend

The GoaT legend shows:

- Series name (e.g. "all taxa")
- `n=N [min–max]` summary

These come from `PlotSpec.data` (total count, domain min/max). Add a Vega-Lite
`title` expression to the legend encoding:

```json
"color": {
  "legend": {"title": "all taxa\nn=82 [1–57]"}
}
```

The count and range are computed from `PlotSpec.data.buckets` in `vl_histogram()`.

### 5.3 Axis number formatting

Use Vega-Lite `"format": "~s"` (SI prefix) for all quantitative axes with
`domain[1] > 1e6`. The `~` strips trailing zeros.

### 5.4 Grid style

```json
"config": {
  "axis": {
    "gridDash": [4, 4],
    "gridColor": "#ccc",
    "gridOpacity": 0.7
  }
}
```

### 5.5 Default dimensions

| Report type  | Width | Height |
| ------------ | ----- | ------ |
| histogram    | 600   | 400    |
| scatter      | 500   | 500    |
| countPerRank | 400   | 300    |
| sources      | 400   | 300    |
| tree         | 600   | 600    |
| map          | 800   | 500    |
| arc          | 400   | 400    |

These are overridable via `PlotSpec.display.width` / `.height`.

---

## 6. Non-visual output formats

The `--format` flag on `plot` and `local-report` gains additional values:

| Format   | Applicable types                               | Output                              |
| -------- | ---------------------------------------------- | ----------------------------------- |
| `json`   | all                                            | Vega-Lite JSON (current default)    |
| `svg`    | histogram, scatter, countPerRank, sources, map | SVG via vl-convert                  |
| `png`    | same as svg                                    | PNG via vl-convert                  |
| `tsv`    | histogram, scatter, countPerRank, sources, arc | Tab-separated data table            |
| `newick` | tree                                           | Newick-format string                |
| `vega`   | tree, arc (full)                               | Full Vega (not Vega-Lite) JSON spec |

`--format vega` defers rendering to the user (useful for pasting into the
Vega editor or feeding a JS renderer). The Vega spec is richer than Vega-Lite
for trees and chord diagrams.

### TSV schema

**Histogram:**

```
bin_start\tbin_end\tcount
1000000000\t1197000000\t23
…
```

**Scatter:**

```
x\ty\tcat
1.2e9\t22\t…
```

**CountPerRank / Sources:**

```
rank\tcount
species\t1234
…
```

### Newick schema

Tree node labels include taxon name + genome_size value in brackets if a `y`
field is present:

```
(Canis_lupus:1[genome_size=2.4G],Vulpes_vulpes:1[genome_size=2.7G])Canidae;
```

---

## 7. Implementation plan

### Step 1 — Histogram bar geometry (highest visual impact)

- In `vl_histogram()`: compute `step` from `AxisMeta.domain` + bucket count
- Mutate each bucket value to add `"key2": key + step`
- Switch encoding to `bin: {binned: true}` with `x2: {field: "key2"}`
- Align x-axis ticks to bucket edges

### Step 2 — Viridis + legend

- Add viridis colour defaults to all `vl_*` functions
- Add legend title with `n=N [min–max]` from data summary

### Step 3 — SI axis formatting + grid style

- Add `"format": "~s"` to quantitative axes where `domain[1] > 1e6`
- Add `config.axis` grid defaults

### Step 4 — Rank enforcement validation

- Add `ReportType::requires_rank()` helper
- Wire validation in client template and Python SDK

### Step 5 — TSV output format

- Add `--format tsv` handling in `vl_convert_render()` or a new
  `plot_spec_to_tsv()` Rust function (no external dependency)
- Handle each report type's data shape

### Step 6 — Newick output

- Add `plot_spec_to_newick()` in `report/mod.rs`
- Recursive serialisation from `PlotSpec.data.tree` node structure
- Wire `--format newick` in the `plot` handler

### Step 7 — CountPerRank / Sources orientation fix

- Swap x/y axes for `countPerRank` to vertical bars

### Step 8 — Tree Vega spec stub

- Emit a minimal Vega (not Vega-Lite) tree spec using `treeLayout` transform
- Update `vl_convert_render()` to detect `"$schema"` prefix and pick
  `vl2svg` vs `vg2svg` backend accordingly

---

## 8. Touch-points

| File                                              | Change                                                                                               |
| ------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `crates/genomehubs-query/src/report/mod.rs`       | `vl_histogram`, `vl_scatter`, `vl_bar`, `plot_spec_to_tsv`, `plot_spec_to_newick`                    |
| `crates/genomehubs-query/src/report/plot_spec.rs` | Add `step` field to `AxisMeta`; add `summary` field to `PlotSpec.data`                               |
| `crates/genomehubs-query/src/report/mod.rs`       | `ReportType::requires_rank(index: &str) -> bool` helper                                              |
| `templates/rust/main.rs.tera`                     | Add `tsv` and `newick` to `--format` value parser; add `vl_convert_render` branch for `tsv`/`newick` |
| `templates/rust/client.rs.tera`                   | Pre-flight rank validation for histogram/scatter/map on taxon                                        |
| `python/cli_generator/query.py`                   | `report()` rank validation                                                                           |
| `templates/python/query.py.tera`                  | Mirror                                                                                               |
| `src/lib.rs`                                      | Expose `plot_spec_to_tsv`, `plot_spec_to_newick` via PyO3                                            |
| `python/cli_generator/cli_generator.pyi`          | Stubs                                                                                                |
| `python/cli_generator/__init__.py`                | Exports                                                                                              |

---

## 9. Open questions

1. **`step` in `AxisMeta`** — Does `spec_builder.rs` currently emit a `step` value
   for histogram axes? If not, it must be added server-side (preferred) or derived
   client-side from `domain / bucket_count`.

2. **Tree data shape in `PlotSpec`** — The current `PlotSpec.data` for tree reports
   is not fully specified. A recursive node structure (`{name, children: [...], value?}`)
   needs to be confirmed before Newick serialisation can be written.

3. **Map hex data** — Whether hex centroid coordinates are present in `PlotSpec.data`
   determines whether a choropleth is feasible in this phase. If not, map SVG output
   remains a stub.

4. **`vl-convert` Vega mode** — The `vl-convert` binary supports `vg2svg` for full
   Vega specs. The Python binding (`vlc.vega_to_svg()`) also supports this.
   `vl_convert_render()` should detect a full Vega spec (no `$schema` containing
   `vega-lite`) and dispatch accordingly.

5. **`default_rank` fallback** — Should the rank fallback be a generator-level
   default (from `sites/goat.yaml`) or a runtime default in the generated CLI?
   Recommended: generator injects `const DEFAULT_RANK: &str = "species"` into
   `cli_meta.rs` from the site config.
