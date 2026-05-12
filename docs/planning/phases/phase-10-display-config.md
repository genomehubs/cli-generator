# Phase 10: Display Configuration (`display_yaml`)

**Depends on:** Phase 6 (report endpoint exists)
**Blocks:** Phase 12 (PlotSpec uses DisplaySpec as its presentation layer)
**Estimated scope:** 1 new type file, amendment to `ReportRequest`/`ReportResponse`, SDK method additions

---

## Goal

Extend the `/api/v3/report` endpoint with an optional `display` field that controls
how the result should be presented.

This separates two orthogonal concerns:

- `report` — **what** data to aggregate and in what shape (histogram, scatter, etc.)
- `display` — **how** to present that data (title, dimensions, colours, labels)

`display` is optional everywhere. The server parses it, attaches a `DisplaySpec` to
the response, and returns it unchanged. Rendering is always client-side.

**Do not add a separate `style` field.** Layout and style are both presentation concerns
and the consumer needs them together.

---

## API Field Naming Convention

Following the established pattern for `query`, `params`, and `report`:

- The request field is named `display` (accepts **YAML string OR JSON object**)
- The response field is named `display` (always a parsed object, never a raw string)
- The `ReportBuilder` setter is named `set_display(value)` — consistent with all other setters
- The deserialization helper `to_yaml()` in `deserialize_helpers.rs` already handles
  both string and object inputs — the same pattern used for `query`, `params`, `report`

There is no separate `display_yaml` vs `display` split in the wire format: the field
is called `display` and accepts either form, just like `query` and `report`.

---

## Files to Create

```
crates/genomehubs-query/src/report/display.rs   — DisplaySpec type
```

## Files to Modify

| File                                         | Change                                                                       |
| -------------------------------------------- | ---------------------------------------------------------------------------- |
| `crates/genomehubs-query/src/report/mod.rs`  | `pub mod display; pub use display::DisplaySpec;`                             |
| `crates/genomehubs-api/src/routes/report.rs` | Add `display: Option<String>` to `ReportRequest`; parse + attach to response |
| `python/cli_generator/query.py`              | `set_display()` method on `ReportBuilder`; pass in `QueryBuilder.report()`   |
| `templates/python/query.py.tera`             | Mirror `set_display()`                                                       |
| `templates/js/query.js`                      | `setDisplay()` method on `ReportBuilder`                                     |
| `templates/r/query.R`                        | `set_display()` method on `ReportBuilder`                                    |

Note: there is no `report/builder.rs` in `genomehubs-query` — the `ReportBuilder`
lives in `python/cli_generator/query.py`, `templates/js/query.js`, and
`templates/r/query.R`. Rust does not have a `ReportBuilder` type.

---

## `display` Field Format

The `display` field accepts either a YAML string or a JSON/dict object:

```json
// As a JSON object (preferred in programmatic use)
{
  "query": { "index": "taxon", ... },
  "params": { "size": 10 },
  "report": { "report": "histogram", "x": "genome_size" },
  "display": {
    "title": "Genome size distribution in Mammalia",
    "width": 800,
    "color_scheme": "tableau10"
  }
}

// As a YAML string (same as query/params/report_yaml pattern)
{
  "report_yaml": "report: histogram\nx: genome_size\n",
  "display_yaml": "title: Genome size\nwidth: 800\n"
}
```

All keys are optional. Missing keys use renderer defaults.

### Supported fields

```yaml
# Presentation / layout
title: "Genome size distribution in Mammalia"
subtitle: "" # optional secondary title
width: 800 # plot width in pixels (default: 600)
height: 500 # plot height in pixels (default: 400)
legend: right # none | top | right | bottom | left
x_label: "Genome size (Gb)"
y_label: "Count"
tooltip_fields:
  - assembly_level
  - assembly_span
number_format: ".2s" # d3 format string; default auto

# Visual style
color_scheme: tableau10 # named Vega/ColorBrewer scheme, or list of hex values
font_size: 12
line_width: 1
opacity: 0.8
marker_size: 4 # for scatter points
```

---

## `DisplaySpec` Type (`crates/genomehubs-query/src/report/display.rs`)

```rust
use serde::{Deserialize, Serialize};

/// Legend position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LegendPosition {
    None,
    Top,
    #[default]
    Right,
    Bottom,
    Left,
}

/// Presentation and visual style options for a report.
///
/// Parsed from the `display` field in the API request and attached to the
/// response unchanged. Rendering is always client-side.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DisplaySpec {
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    #[serde(default)]
    pub legend: LegendPosition,
    pub x_label: Option<String>,
    pub y_label: Option<String>,
    #[serde(default)]
    pub tooltip_fields: Vec<String>,
    pub number_format: Option<String>,

    // Visual style
    pub color_scheme: Option<serde_json::Value>, // string name or array of hex
    pub font_size: Option<f32>,
    pub line_width: Option<f32>,
    pub opacity: Option<f32>,
    pub marker_size: Option<f32>,
}
```

---

## API Request / Response Amendment

`ReportRequest` gains an optional `display` field. The `custom Deserialize` impl
already uses `to_yaml()` for all fields — add the same pattern:

```rust
// In the ReportRequest Deserialize impl:
let display_yaml = map.get("display").or_else(|| map.get("display_yaml"))
    .map(|v| to_yaml(v))
    .transpose()?;

pub struct ReportRequest {
    pub query_yaml: String,
    pub params_yaml: String,
    pub report_yaml: String,
    pub display_yaml: Option<String>,   // new
}
```

The response includes a `display` key alongside `report` when `display_yaml` was present:

```json
{
  "status": { "success": true, "hits": 5432, "took": 18 },
  "report": { ... },
  "display": {
    "title": "Genome size",
    "width": 800,
    "color_scheme": "tableau10"
  }
}
```

```rust
pub struct ReportResponse {
    pub status: ApiStatus,
    pub report: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<DisplaySpec>,   // new
}
```

---

## SDK: `ReportBuilder.set_display()` Method

Consistent with the existing `set_x()`, `set_rank()` etc. naming on `ReportBuilder`.
Accepts either a dict/object or a YAML string — mirrors `QueryBuilder.set_query()`.

```python
# Python — dict form (preferred)
rb = (
    ReportBuilder("histogram")
    .set_x("genome_size")
    .set_display({"title": "Genome size", "width": 800})
)

# Python — YAML string form
rb = (
    ReportBuilder("histogram")
    .set_x("genome_size")
    .set_display("title: Genome size\nwidth: 800\n")
)
```

`set_display()` stores the value in `self._display`. `to_display_yaml()` serialises it
(same pattern as `to_report_yaml()`). `QueryBuilder.report()` passes it as the
`display` key when non-None.

```javascript
// JavaScript
const rb = new ReportBuilder("histogram")
  .setX("genome_size")
  .setDisplay({ title: "Genome size", width: 800 });
```

```r
# R
rb <- ReportBuilder$new("histogram")$
  set_x("genome_size")$
  set_display(list(title = "Genome size", width = 800L))
```

---

## Vega-Lite Mapping (JS SDK only)

This mapping lives in the JS SDK layer (`templates/js/query.js`), not in Rust. It is
used in Phase 12 when `plotSpecToVegaLite()` is called.

| `DisplaySpec` field | Vega-Lite path              |
| ------------------- | --------------------------- |
| `title`             | `spec.title`                |
| `width`             | `spec.width`                |
| `height`            | `spec.height`               |
| `legend` = `none`   | `encoding.*.legend: null`   |
| `x_label`           | `encoding.x.title`          |
| `y_label`           | `encoding.y.title`          |
| `tooltip_fields`    | `encoding.tooltip`          |
| `color_scheme`      | `config.range.category`     |
| `font_size`         | `config.axis.labelFontSize` |
| `opacity`           | `encoding.opacity.value`    |
| `marker_size`       | `config.point.size`         |

---

## CLI plotting note

Phase 12 adds CLI rendering via `plotters`. `DisplaySpec` drives dimensions and style.
No additional changes to `DisplaySpec` are needed for CLI support — `width`, `height`,
`color_scheme`, `x_label`, `y_label` map directly to `plotters` chart configuration.

---

## Testing

- Unit test: `DisplaySpec::default()` produces all-None struct
- Unit test: round-trip `serde_yaml` → `DisplaySpec` → `serde_json`
- Proptest: fuzz YAML input, assert no panic on deserialisation
- Unit test: `to_yaml()` helper accepts both string and object for `display` field
- API integration test: `POST /api/v3/report` with `display` returns `display` key in response
- API integration test: `POST /api/v3/report` without `display` omits `display` key from response

---

# Phase 10b: Per-Report-Type Display Options

**Depends on:** Phase 10 (`DisplaySpec` implemented)
**Extends:** `DisplaySpec` with per-report-type sub-structs
**Estimated scope:** 7 new sub-structs, amendments to `display.rs` and `ReportRequest`/`ReportResponse`

---

## Design Decision: Nested Sub-structs

The v2 UI has a large flat parameter space (`stacked`, `cumulative`, `yScale`,
`catToX`, `compactLegend`, `colorPalette`, `pointSize`, `zScale`, `mapType`,
`mapTheme`, …). Adding all of these as top-level `DisplaySpec` fields would make the
struct unreadable and would allow meaningless combinations (e.g. `stacked` on a map).

**Strategy:** Keep a small set of universal top-level fields in `DisplaySpec`
(already implemented in Phase 10), plus a `report_options` field that holds one of
several typed sub-structs depending on the report type. The outer layer stays clean;
consumers cast to the appropriate sub-struct using the `report` field value.

```yaml
# Request example — histogram with categorical and display options
report: histogram
x: genome_size
cat: assembly_level
```

```json
{
  "display": {
    "title": "Genome size by assembly level",
    "width": 900,
    "compact_legend": false,
    "cat_label": "Assembly level",
    "histogram": {
      "stacked": true,
      "cumulative": false,
      "y_scale": "log10",
      "cat_to_x": false
    }
  }
}
```

The per-report sub-struct is accessed via `display.histogram`, `display.scatter`, etc.
This means:

- A single `DisplaySpec` can carry all options without enum dispatch at the top level
- Fields are self-documenting in JSON schema (no mysterious `stacked` on a map)
- Renderers read `display.scatter.z_scale`, not `display.z_scale`

---

## Universal Fields Added to `DisplaySpec`

These options apply to most report types and live at the top level:

| Field            | Type             | Default | Notes                                             |
| ---------------- | ---------------- | ------- | ------------------------------------------------- |
| `compact_legend` | `Option<bool>`   | auto    | Compact (small squares) vs detailed legend        |
| `compact_width`  | `Option<u32>`    | `400`   | Width below which compact legend is auto-selected |
| `cat_label`      | `Option<String>` | —       | Override the category axis/legend label           |
| `caption`        | `Option<String>` | —       | Figure caption shown below the plot               |
| `point_size`     | `Option<f32>`    | `15`    | Base font/point size (drives all text sizes)      |
| `color_palette`  | `Option<String>` | —       | Named palette (Vega/ColorBrewer) overriding theme |

`compact_legend` replaces the v2 `compactLegend` flag. When `None`, renderers auto-
select based on `compact_width` vs available plot width.

`cat_label` was missing from Phase 10 — it provides the display label for the
category (series) axis / legend header.

`point_size` is already in Phase 10 as `font_size` / `marker_size`. The v2 parameter
is a unified base size used for fonts, legend entries, and tick labels simultaneously.
**Replace** the separate `font_size` and `marker_size` fields in `DisplaySpec` with a
single `point_size: Option<f32>` to match v2 semantics. (See migration note below.)

---

## `AxisOptions` Shared Sub-struct

Axis label orientation and overflow handling is per-axis, not per-report-type. A
shared `AxisOptions` sub-struct is embedded in any per-report struct that has configurable axes:

```rust
/// Per-axis tick label and overflow options.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AxisOptions {
    /// Override label text for this axis.
    pub label: Option<String>,
    /// Rotate tick labels by this many degrees (positive = counter-clockwise).
    ///
    /// Default: 0 (horizontal). Set to -90 for vertical labels when tick values
    /// are long strings. The v2 UI uses -90 automatically when max tick label
    /// length exceeds `compact_width`.
    pub tick_label_angle: Option<i16>,
    /// Maximum characters per tick label before truncation with "…".
    /// `None` means no truncation.
    pub tick_label_max_length: Option<usize>,
    /// Show or hide tick labels entirely.
    /// Default: `true` when plot width ≥ `compact_width`.
    pub show_tick_labels: Option<bool>,
    /// Number format string (d3 format, e.g. `".2s"`).
    /// Overrides `DisplaySpec.number_format` for this axis.
    pub number_format: Option<String>,
}
```

`AxisOptions` is embedded inside `HistogramOptions`, `ScatterOptions`, etc. rather
than added to `DisplaySpec` directly, because not every report has an X or Y axis.

---

## Per-Report-Type Sub-structs

### `HistogramOptions`

Sourced from: `stacked`, `cumulative`, `yScale`, `catToX`, `xOpts` (index 4 = x-label),
`compactLegend`, `compactWidth`, `pointSize`, `colorPalette` in the v2 UI.

```rust
/// Histogram-specific display options.
///
/// Set via `display.histogram` in the API request.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HistogramOptions {
    /// Stack category series instead of overlaying them.
    /// Default: `false`.
    pub stacked: Option<bool>,

    /// Cumulative sum mode: each bar shows the sum of all bars up to that point.
    /// Default: `false`.
    pub cumulative: Option<bool>,

    /// Y-axis scale. One of `"linear"`, `"log10"`, `"proportion"`.
    /// `"proportion"` normalises each bar to 1.0.
    /// Default: `"linear"`.
    pub y_scale: Option<String>,

    /// When `true` and the X axis is an ordinal lineage field, each category
    /// becomes its own X-axis bin (category-to-X transpose). Produces a bar
    /// chart where each lineage is a bar rather than a series.
    /// Default: `false`.
    pub cat_to_x: Option<bool>,

    /// X-axis options.
    pub x_axis: Option<AxisOptions>,

    /// Y-axis options.
    pub y_axis: Option<AxisOptions>,
}
```

**`y_scale` values:**

- `"linear"` — raw counts (default)
- `"log10"` — log-transformed Y axis
- `"proportion"` — each bar normalised to 1.0; Y axis is 0–1; tick labels are fractions

**`cat_to_x` semantics:** Only meaningful when `cat` is set and the X field is a
lineage/ordinal field. Transposes the histogram so each category becomes an X bin.

---

### `ScatterOptions`

Sourced from: `zScale`, `stacked`, `reversed`, `highlightArea`, `yOpts` (index 4 = y-label),
`xOpts`, `scatterThreshold`, `compactLegend`, `compactWidth`, `pointSize`, `colorPalette`.

```rust
/// Scatter-specific display options.
///
/// Set via `display.scatter` in the API request.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScatterOptions {
    /// Z-axis (heatmap density cell colour) scale.
    /// One of `"linear"`, `"log10"`, `"proportion"`, `"sqrt"`.
    /// Default: `"linear"`.
    pub z_scale: Option<String>,

    /// Stack mode — colour cells by category proportion rather than raw count.
    /// Default: `false`.
    pub stacked: Option<bool>,

    /// Reverse the Y axis (high values at bottom).
    /// Default: `false`.
    pub reversed: Option<bool>,

    /// Highlight a rectangular region: `"x_min,x_max,y_min,y_max"`.
    /// Draws a highlighted box over the scatter grid.
    pub highlight_area: Option<String>,

    /// Maximum number of raw data points to overlay on the heatmap before
    /// switching to heatmap-only mode. Default: from `ReportBuilder`.
    pub scatter_threshold: Option<u32>,

    /// X-axis options.
    pub x_axis: Option<AxisOptions>,

    /// Y-axis options.
    pub y_axis: Option<AxisOptions>,
}
```

---

### `CountPerRankOptions`

The v2 `countPerRank`/`xPerRank` report has few display options beyond the universal
fields. The only additional option is axis orientation for long rank labels.

```rust
/// Count-per-rank bar chart display options.
///
/// Set via `display.count_per_rank` in the API request.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CountPerRankOptions {
    /// Rank-axis label options (the horizontal bar labels).
    pub rank_axis: Option<AxisOptions>,
    /// Value-axis label options.
    pub value_axis: Option<AxisOptions>,
}
```

---

### `MapOptions`

Sourced from: `mapType`, `mapTheme`, `mapProjection`, `geoBinResolution`, `geoBounds`.
(`locationField` and `regionField` belong in the `report` config not `display`.)

```rust
/// Map report display options.
///
/// Set via `display.map` in the API request.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MapOptions {
    /// Map projection. One of `"cylindricalEqualArea"` (default),
    /// `"mercator"`, `"naturalEarth1"`, `"orthographic"`.
    pub projection: Option<String>,

    /// Visual theme for the basemap. One of `"light"`, `"dark"`.
    pub theme: Option<String>,

    /// Display mode: `"hex"` (hexbin aggregation), `"point"` (individual points).
    /// Default: auto-selected based on point count vs `map_threshold`.
    pub map_type: Option<String>,

    /// Geographic bounding box to fit. Format: `"lon_min,lat_min,lon_max,lat_max"`.
    pub geo_bounds: Option<String>,
}
```

`geoBinResolution` belongs in the report config (`report` field) because it affects
the ES aggregation bucket size, not purely the rendering.

---

### `TreeOptions`

Sourced from: `treeStyle`, `hideSourceColors`, `hideErrorBars`, `hideAncestralBars`,
`showPhylopics`, `phylopicRank`, `phylopicSize`.

```rust
/// Tree report display options.
///
/// Set via `display.tree` in the API request.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TreeOptions {
    /// Tree layout style. One of `"rect"` (default, rectangular cladogram),
    /// `"circ"` (circular), `"radial"`.
    pub tree_style: Option<String>,

    /// Hide source colour bars on tree nodes.
    /// Default: `false`.
    pub hide_source_colors: Option<bool>,

    /// Hide error bars on tree node summary boxes.
    /// Default: `false`.
    pub hide_error_bars: Option<bool>,

    /// Hide ancestral value bars (show only direct/descendant).
    /// Default: `false`.
    pub hide_ancestral_bars: Option<bool>,

    /// Overlay PhyloPic silhouettes on tree nodes.
    /// Default: `false`.
    pub show_phylopics: Option<bool>,

    /// Taxonomic rank at which to show PhyloPic images.
    /// Only relevant when `show_phylopics` is `true`.
    pub phylopic_rank: Option<String>,

    /// Scale factor for PhyloPic silhouette size.
    /// Default: `1.0`.
    pub phylopic_size: Option<f32>,
}
```

---

### `ArcOptions`

The arc (Venn-style 3-circle) report has minimal display options. Point size and
color palette come from the universal fields.

```rust
/// Arc (Venn) report display options.
///
/// Set via `display.arc` in the API request.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ArcOptions {
    /// Show percentage labels inside arc segments.
    /// Default: `true`.
    pub show_labels: Option<bool>,
}
```

---

### `SourcesOptions`

The sources report (data attribution bar chart) has only axis orientation options.

```rust
/// Sources report display options.
///
/// Set via `display.sources` in the API request.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SourcesOptions {
    /// Source-axis label options (the horizontal bar labels).
    pub source_axis: Option<AxisOptions>,
    /// Value-axis label options.
    pub value_axis: Option<AxisOptions>,
}
```

---

## Updated `DisplaySpec` with `report_options`

Add the per-report sub-struct fields directly to `DisplaySpec`. Using flat optional
fields (rather than an enum) means partial deserialization works correctly and the
schema remains ergonomic.

```rust
/// Presentation and visual style options for a report.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DisplaySpec {
    // ── Universal ─────────────────────────────────────────────────────────────
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub caption: Option<String>,          // new (Phase 10b)
    pub width: Option<u32>,
    pub height: Option<u32>,
    #[serde(default)]
    pub legend: LegendPosition,
    /// Compact (small squares) vs full legend with sample patches.
    /// `None` = auto-select based on `compact_width` vs available width.
    pub compact_legend: Option<bool>,     // new (Phase 10b)
    /// Width threshold below which compact legend is auto-selected.
    pub compact_width: Option<u32>,       // new (Phase 10b), default 400
    pub x_label: Option<String>,
    pub y_label: Option<String>,
    /// Override the category axis / legend header label.
    pub cat_label: Option<String>,        // new (Phase 10b)
    #[serde(default)]
    pub tooltip_fields: Vec<String>,
    /// Base size for fonts, legend entries, and tick labels (unified like v2).
    pub point_size: Option<f32>,          // replaces font_size + marker_size
    pub number_format: Option<String>,
    pub color_scheme: Option<serde_json::Value>,
    pub color_palette: Option<String>,    // new (Phase 10b) — named palette
    pub line_width: Option<f32>,
    pub opacity: Option<f32>,

    // ── Per-report-type (at most one will be non-None) ─────────────────────────
    pub histogram: Option<HistogramOptions>,
    pub scatter: Option<ScatterOptions>,
    pub count_per_rank: Option<CountPerRankOptions>,
    pub map: Option<MapOptions>,
    pub tree: Option<TreeOptions>,
    pub arc: Option<ArcOptions>,
    pub sources: Option<SourcesOptions>,
}
```

**Migration note:** `font_size` and `marker_size` from Phase 10 are **removed** and
replaced by the single `point_size` field. Any code that read `display.font_size` or
`display.marker_size` should switch to `display.point_size`. The Phase 10 unit tests
need updating accordingly.

---

## Files to Create / Modify

```
crates/genomehubs-query/src/report/display.rs  — extend with all new structs
```

| File                                            | Change                                                                                   |
| ----------------------------------------------- | ---------------------------------------------------------------------------------------- |
| `crates/genomehubs-query/src/report/display.rs` | Add `AxisOptions`, 7 sub-structs, update `DisplaySpec`; remove `font_size`/`marker_size` |
| `crates/genomehubs-api/src/routes/report.rs`    | No change — `DisplaySpec` is already deserialized from `display_yaml`                    |
| `python/cli_generator/query.py`                 | `set_display()` accepts the richer dict; no code change needed                           |
| `templates/python/query.py.tera`                | Mirror — no code change needed                                                           |
| `templates/js/query.js`                         | `setDisplay()` accepts the richer object; no code change needed                          |
| `templates/r/query.R`                           | `set_display()` accepts the richer list; no code change needed                           |

No SDK code changes are needed because `set_display()` already accepts an opaque
dict/object/list — the richer structure is transparent to the wiring layer.

---

## Wire Format Examples

```json
// Histogram: stacked log10 with compact legend and cat label
{
  "display": {
    "title": "Genome size by assembly level",
    "cat_label": "Assembly level",
    "compact_legend": false,
    "histogram": {
      "stacked": true,
      "y_scale": "log10",
      "x_axis": { "label": "Genome size (Gb)", "number_format": ".2s" }
    }
  }
}

// Scatter: z-scale log10, highlight region, reversed Y
{
  "display": {
    "title": "Assembly span vs genome size",
    "scatter": {
      "z_scale": "log10",
      "reversed": true,
      "highlight_area": "1e8,1e10,1e8,1e10",
      "x_axis": { "label": "Genome size (Gb)" },
      "y_axis": { "label": "Assembly span" }
    }
  }
}

// Tree: rectangular style, no ancestral bars, PhyloPic at class level
{
  "display": {
    "width": 1200,
    "tree": {
      "tree_style": "rect",
      "hide_ancestral_bars": true,
      "show_phylopics": true,
      "phylopic_rank": "class"
    }
  }
}

// Map: dark theme, orthographic projection, geographic bounds
{
  "display": {
    "map": {
      "projection": "orthographic",
      "theme": "dark",
      "geo_bounds": "-20,35,50,72"
    }
  }
}
```

---

## v2 reportTerms → Phase 10b Field Mapping

| v2 term             | Applies to         | Phase 10b location                                      | Notes                                                      |
| ------------------- | ------------------ | ------------------------------------------------------- | ---------------------------------------------------------- |
| `compactLegend`     | histogram, scatter | `display.compact_legend`                                | Universal — top level                                      |
| `compactWidth`      | histogram, scatter | `display.compact_width`                                 | Universal — top level                                      |
| `caption`           | all                | `display.caption`                                       | Universal — top level                                      |
| `colorPalette`      | most               | `display.color_palette`                                 | Universal — top level                                      |
| `pointSize`         | all except sources | `display.point_size`                                    | Universal — top level                                      |
| `catLabel`          | histogram, scatter | `display.cat_label`                                     | Universal — top level                                      |
| `stacked`           | histogram, scatter | `display.histogram.stacked` / `display.scatter.stacked` |                                                            |
| `cumulative`        | histogram          | `display.histogram.cumulative`                          |                                                            |
| `yScale`            | histogram          | `display.histogram.y_scale`                             |                                                            |
| `catToX`            | histogram          | `display.histogram.cat_to_x`                            | Server-side param → stays in `report` if it affects ES agg |
| `xOpts[4]`          | histogram, scatter | `display.x_label` OR `display.histogram.x_axis.label`   | Prefer top-level `x_label`                                 |
| `yOpts[4]`          | scatter            | `display.y_label` OR `display.scatter.y_axis.label`     | Prefer top-level `y_label`                                 |
| `xOpts[2]`          | scatter            | `display.scatter.x_axis.show_tick_labels`               |                                                            |
| `yOpts[2]`          | scatter            | `display.scatter.y_axis.show_tick_labels`               |                                                            |
| `zScale`            | scatter            | `display.scatter.z_scale`                               |                                                            |
| `reversed`          | scatter            | `display.scatter.reversed`                              |                                                            |
| `highlightArea`     | scatter            | `display.scatter.highlight_area`                        |                                                            |
| `scatterThreshold`  | scatter            | `display.scatter.scatter_threshold`                     |                                                            |
| `treeStyle`         | tree               | `display.tree.tree_style`                               |                                                            |
| `hideSourceColors`  | tree               | `display.tree.hide_source_colors`                       |                                                            |
| `hideErrorBars`     | tree               | `display.tree.hide_error_bars`                          |                                                            |
| `hideAncestralBars` | tree               | `display.tree.hide_ancestral_bars`                      |                                                            |
| `showPhylopics`     | tree               | `display.tree.show_phylopics`                           |                                                            |
| `phylopicRank`      | tree               | `display.tree.phylopic_rank`                            |                                                            |
| `phylopicSize`      | tree               | `display.tree.phylopic_size`                            |                                                            |
| `mapType`           | map                | `display.map.map_type`                                  |                                                            |
| `mapTheme`          | map                | `display.map.theme`                                     |                                                            |
| `mapProjection`     | map                | `display.map.projection`                                |                                                            |
| `geoBounds`         | map                | `display.map.geo_bounds`                                |                                                            |

**v2 terms deliberately omitted:**

- `dropShadow` — ribbon only (Phase 11), add to ribbon sub-struct then
- `reorient` — oxford/ribbon only (Phase 11)
- `plotRatio` — positional only (Phase 11)
- `treeThreshold` — belongs in `report` config, affects ES query
- `mapThreshold` — belongs in `report` config, affects ES query
- `geoBinResolution` — belongs in `report` config, affects ES query
- `locationField`, `regionField` — belong in `report` config
- `highlight` — table only (table is out of v3 scope)
- `treeStyle` = `"circ"` etc — add when tree rendering is implemented in Phase 12

---

## Testing

- Unit test: `HistogramOptions::default()` round-trips through `serde_yaml`
- Unit test: `DisplaySpec` with nested `histogram` sub-struct deserializes correctly
- Unit test: `DisplaySpec` with nested `scatter.highlight_area` round-trips
- Unit test: `DisplaySpec` with `compact_legend: true` serializes correctly
- Unit test: `cat_label` is preserved through YAML → JSON round-trip
- Unit test: `TreeOptions` with all bool flags serializes to correct JSON
- Unit test: unknown sub-struct fields are ignored (no `deny_unknown_fields`)
- Proptest: fuzz nested YAML input, assert no panic
