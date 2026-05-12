# Phase 12: Reusable Plot Specifications

**Depends on:** Phase 6 (report types defined), Phase 10 (DisplaySpec exists)
**Blocks:** Phase 13 (hybrid reports return a PlotSpec)
**Estimated scope:** 1 new module in `genomehubs-query`, 1 new module in `genomehubs-api`,
CLI rendering via `plotters`, JS `plotSpecToVegaLite()` helper, SDK method additions

Note: Phase 11 (positional family endpoint) is **deferred** — it does not block Phase 12. The `PlotSpec` type is defined for histogram, scatter, countPerRank, sources, arc,
map, and tree only. Oxford/ribbon/painting variants of `PlotSpec` are added when
Phase 11 lands.

Note: Phase 10b added per-report-type `DisplaySpec` sub-structs (`histogram`,
`scatter`, `map`, `tree`, etc.), a `TickLabelPlacement` enum, `AxisOptions` hints,
and `equation_line`/`base_type` fields. `PlotSpec.display` carries these through to
renderers unchanged. `AxisMeta` now includes resolved display fields
(`tick_label_placement`, `tick_label_stride`, `tick_label_max_length`) so renderers
never need to auto-detect axis label behaviour.

---

## Goal

Define a `PlotSpec` type in `crates/genomehubs-query` (WASM-compatible, shared across
all SDKs) that wraps processed report data alongside display metadata. This enables:

1. **CLI rendering** — `genomehubs plot` command outputs SVG/PNG without a browser
2. **JS SDK → Vega-Lite** — `plotSpecToVegaLite()` in JS for interactive browser rendering
3. **Python SDK** — return `PlotSpec` as a dict consumable by `altair` / `plotly`
4. **R SDK** — return `PlotSpec` as a list consumable by `ggplot2` / `vegalite`

The key principle: **Rust defines the data shape and axis metadata; each platform
renders natively.** Rust handles SVG/PNG for the CLI. JS converts `PlotSpec` to
Vega-Lite via a thin ~100-line JS function. Python/R receive the dict/list directly.

---

## Architecture

- Rust produces `PlotSpec` (a clean domain type, serde-serialisable)
- `PlotSpec` is returned in the API response as `plot_spec` (optional)
- The JS SDK contains `plotSpecToVegaLite(spec)` — pure JS, not in Rust
- For CLI: Rust renders `PlotSpec` via `plotters` (feature-gated, not in WASM)
- Python/R receive `plot_spec` as a plain dict/list; users bring their own renderer

---

## `PlotSpec` Design (`crates/genomehubs-query/src/report/plot_spec.rs`)

```rust
use serde::{Deserialize, Serialize};
use serde_json::Value;
use super::display::DisplaySpec;

/// The type of report this spec describes.
///
/// Oxford/ribbon/painting are added when Phase 11 (positional family) lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlotReportType {
    Histogram,
    Scatter,
    CountPerRank,
    Sources,
    Tree,
    Map,
    Arc,
}

/// Axis metadata for a single axis.
///
/// All display fields here are **resolved** by the server — renderers consume
/// them directly without any auto-detection. `tick_label_placement` and
/// `tick_label_stride` are derived from `AxisOptions` hints (Phase 10b) plus
/// `value_type`; they are never `Option`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxisMeta {
    pub field: String,
    pub label: Option<String>,
    /// One of `"linear"` | `"log10"` | `"log2"` | `"sqrt"` | `"proportion"`.
    /// `"proportion"` normalises the axis to 1.0.
    /// `"sqrt"` is valid for scatter Z (heatmap density) axes.
    pub scale: String,
    pub domain: [f64; 2],
    pub tick_values: Vec<f64>,
    pub tick_labels: Vec<String>,
    /// One of `"integer"` | `"float"` | `"keyword"` | `"coordinate"`.
    pub value_type: String,
    /// Resolved label placement. Derived from `AxisOptions.tick_label_placement`
    /// if set; otherwise auto-detected: `on_tick` for numeric/range axes,
    /// `between_ticks` for keyword/category axes (`value_type == "keyword"`).
    /// Tick marks always fall on bin boundaries regardless.
    pub tick_label_placement: TickLabelPlacement,
    /// Resolved label stride. Derived from `AxisOptions.tick_label_stride` if
    /// set; otherwise the server picks the largest stride that fits the axis
    /// width at the requested plot dimensions. `1` means show every label.
    pub tick_label_stride: u32,
    /// Maximum characters before truncation with `…`. Passed through from
    /// `AxisOptions.tick_label_max_length`; `None` means no truncation.
    pub tick_label_max_length: Option<usize>,
}

/// A category/series for multi-series plots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeriesMeta {
    pub key: String,
    pub label: String,
    pub color: Option<String>,
}

/// A fully self-contained plot specification.
///
/// Contains processed data and all metadata needed to render the plot
/// on any platform. Produced by `crates/genomehubs-api/src/report/spec_builder.rs`
/// and returned in the API response `plot_spec` field when requested.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlotSpec {
    pub report_type: PlotReportType,
    pub x: Option<AxisMeta>,
    pub y: Option<AxisMeta>,
    pub z: Option<AxisMeta>,   // for 2D histograms / heatmaps
    pub series: Vec<SeriesMeta>,
    pub display: DisplaySpec,
    /// Serialised plot data. Shape depends on `report_type`.
    pub data: Value,
}
```

The `PlotReportType` enum uses `snake_case` so serialised names match the existing
`report_type` strings already returned by the API (`"count_per_rank"` etc.). Note
`ReportType` in `crates/genomehubs-query/src/report/mod.rs` is a separate enum used
only for validation/dispatch — `PlotReportType` is for the output spec.

---

## API Response Amendment

Phase 12 adds an optional `plot_spec` field alongside `report` and `display`:

```json
{
  "status": { "success": true, "hits": 5432, "took": 18 },
  "report": { "type": "histogram", "buckets": [...], "bounds": {...} },
  "display": { "title": "Genome size", "width": 800 },
  "plot_spec": {
    "report_type": "histogram",
    "x": { "field": "genome_size", "scale": "log10", "domain": [1e6, 1e12], ... },
    "series": [{ "key": "chromosome", "label": "Chromosome" }],
    "display": { "title": "Genome size", "width": 800 },
    "data": { ... }
  }
}
```

`plot_spec` is generated when the request includes a `display` field (Phase 10) OR
when `include_plot_spec: true` is set in `params`. Without either, `plot_spec` is
omitted for backward compatibility.

```rust
pub struct ReportResponse {
    pub status: ApiStatus,
    pub report: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<DisplaySpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plot_spec: Option<PlotSpec>,
}
```

The `params` field for `include_plot_spec` uses the existing `QueryParams`-style
pattern: add `include_plot_spec: bool` to `QueryParams` (default false) so it can
be set via the object/YAML `params` field.

---

## `spec_builder.rs` — PlotSpec Construction

```rust
// crates/genomehubs-api/src/report/spec_builder.rs
use genomehubs_query::report::{DisplaySpec, PlotSpec, plot_spec::{AxisMeta, PlotReportType, SeriesMeta}};
use genomehubs_query::report::display::{AxisOptions, TickLabelPlacement};
use serde_json::Value;

/// Build a `PlotSpec` from a completed report response.
///
/// `report_data` is the value at `response["report"]`.
/// `display` is the parsed `DisplaySpec` (may be default if `display` was absent).
/// `axes` is extracted from the report pipeline's axis output.
pub fn build_plot_spec(
    report_type: PlotReportType,
    report_data: &Value,
    axes: (Option<AxisMeta>, Option<AxisMeta>),
    series: Vec<SeriesMeta>,
    display: DisplaySpec,
) -> PlotSpec {
    PlotSpec {
        report_type,
        x: axes.0,
        y: axes.1,
        z: None,
        series,
        display,
        data: report_data.clone(),
    }
}

/// Resolve `AxisMeta` display fields from the pipeline output and any user
/// `AxisOptions` hint.
///
/// `value_type` comes from the pipeline (field schema). `opts` comes from the
/// matching sub-struct in `DisplaySpec` (e.g. `display.histogram.x_axis`).
/// The resolved `tick_label_placement` and `tick_label_stride` are written
/// directly into the returned `AxisMeta`.
pub fn resolve_axis_display(meta: &mut AxisMeta, opts: Option<&AxisOptions>) {
    // Placement: user hint > auto from value_type
    meta.tick_label_placement = opts
        .and_then(|o| o.tick_label_placement)
        .unwrap_or_else(|| match meta.value_type.as_str() {
            "keyword" => TickLabelPlacement::BetweenTicks,
            _ => TickLabelPlacement::OnTick,
        });
    // Stride: user hint > 1 (auto-fitting is a renderer concern)
    meta.tick_label_stride = opts.and_then(|o| o.tick_label_stride).unwrap_or(1);
    meta.tick_label_max_length = opts.and_then(|o| o.tick_label_max_length);
}
```

Axis metadata is already partially available from the `AxisSpec` and `BoundsResult`
types produced by the report pipeline. `spec_builder.rs` bridges from the pipeline
output to the `AxisMeta` wire format. `resolve_axis_display` must be called on every
`AxisMeta` after construction so that renderers never need to guess.

---

## Files to Create

```
crates/genomehubs-query/src/report/plot_spec.rs     — PlotSpec type
crates/genomehubs-api/src/report/spec_builder.rs    — PlotSpec construction from pipeline output
```

## Files to Modify

| File                                         | Change                                                                 |
| -------------------------------------------- | ---------------------------------------------------------------------- |
| `crates/genomehubs-query/src/report/mod.rs`  | `pub mod plot_spec; pub use plot_spec::PlotSpec;`                      |
| `crates/genomehubs-api/src/report/mod.rs`    | `pub mod spec_builder;`                                                |
| `crates/genomehubs-api/src/routes/report.rs` | Build `PlotSpec` when requested; add to response                       |
| `crates/genomehubs-query/src/lib.rs`         | WASM export `to_plot_spec_json(response_json)`                         |
| `src/lib.rs`                                 | PyO3 export `to_plot_spec_json`                                        |
| `templates/js/query.js`                      | `plotSpecToVegaLite(spec)` helper function                             |
| `python/cli_generator/query.py`              | `QueryBuilder.report()` returns full response when `plot_spec` present |
| `templates/python/query.py.tera`             | Mirror                                                                 |
| `templates/r/query.R`                        | Mirror                                                                 |

CLI rendering (`genomehubs plot`) is included in this phase — see below.

---

## CLI Rendering (`src/cli/plot.rs`)

The `genomehubs plot` command:

1. Reads a completed API response from stdin or a file (JSON)
2. Extracts `plot_spec` from the response
3. Dispatches to the appropriate renderer

```
genomehubs search --query query.yaml | genomehubs plot --output genome_size.svg
genomehubs report --report report.yaml --plot --output genome_size.png
```

### Renderer dispatch

| `report_type`       | Renderer                                                                                                             |
| ------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `histogram`         | `plotters::HistogramRenderer`                                                                                        |
| `scatter`           | `plotters::ScatterRenderer`; `equation_line` from `display.scatter.equation_line` rendered as overlay curve          |
| `map`               | `plotters::MapRenderer` (GeoJSON overlay); `base_type` from `display.map.base_type` selects flat vs globe projection |
| `tree`              | Newick → SVG dendrogram (custom)                                                                                     |
| `oxford` / `ribbon` | `plotters::ScatterRenderer` with chromosome track                                                                    |
| `painting`          | `plotters::PaintingRenderer`                                                                                         |
| `count_per_rank`    | `plotters::BarRenderer`                                                                                              |
| `sources`           | `plotters::BarRenderer`                                                                                              |
| `arc`               | `plotters::ArcRenderer` (3-circle Venn-like)                                                                         |

### `plotters` crate usage

```rust
// Cargo.toml dependency (workspace Cargo.toml, feature-gated)
plotters = { version = "0.3", optional = true, features = ["svg_backend"] }

// svg output
use plotters::prelude::*;

fn render_histogram(spec: &PlotSpec) -> Result<String, Box<dyn std::error::Error>> {
    let width = spec.display.width.unwrap_or(600);
    let height = spec.display.height.unwrap_or(400);
    let mut svg = String::new();
    let root = SVGBackend::with_string(&mut svg, (width, height)).into_drawing_area();
    // ... build chart from spec.x, spec.data, spec.series
    root.present()?;
    Ok(svg)
}
```

CLI rendering is feature-gated (`--features cli-plot`) so the rendering dependency
does not affect WASM or PyO3 builds.

---

## JS SDK: `plotSpecToVegaLite()`

A pure JS function in `templates/js/query.js`. Not in Rust. Translates `PlotSpec` to
Vega-Lite JSON. Called by the user when they want interactive rendering.

```javascript
// Added to templates/js/query.js
export function plotSpecToVegaLite(plotSpec) {
  const display = plotSpec.display ?? {};
  const base = {
    $schema: "https://vega.github.io/schema/vega-lite/v5.json",
    title: display.title,
    width: display.width ?? 600,
    height: display.height ?? 400,
    config: _buildVegaConfig(display),
  };
  switch (plotSpec.report_type) {
    case "histogram":
      return _buildHistogramSpec(plotSpec, base);
    case "scatter":
      return _buildScatterSpec(plotSpec, base);
    case "count_per_rank":
      return _buildBarSpec(plotSpec, base);
    case "sources":
      return _buildBarSpec(plotSpec, base);
    case "tree":
      return _buildTreeSpec(plotSpec, base);
    case "map":
      return _buildMapSpec(plotSpec, base);
    case "arc":
      return _buildArcSpec(plotSpec, base);
    default:
      return base;
  }
}
```

`plotSpecToVegaLite` is exported as a named export. Oxford/ribbon/painting cases are
added when Phase 11 lands.

---

## Python / R

Users receive `plot_spec` as a plain dict/list in the response. The SDK exposes a
`plot_spec_to_vega_lite(spec)` utility function (mirrors the JS version).

```python
from cli_generator import plot_spec_to_vega_lite

result = qb.report(rb)          # returns full response dict when plot_spec present
spec = result.get("plot_spec")  # None if not requested
if spec:
    vl = plot_spec_to_vega_lite(spec)
    # pass to altair.Chart.from_dict() or save as JSON
```

`plot_spec_to_vega_lite()` is a pure Python function in `python/cli_generator/query.py`
(not via PyO3 — it's a dict transformation, not a compute-heavy operation).

---

## Scope Boundaries

| In scope                                  | Out of scope                                    |
| ----------------------------------------- | ----------------------------------------------- |
| `PlotSpec` type in `genomehubs-query`     | Full Vega-Lite spec generation in Rust          |
| `spec_builder.rs` in `genomehubs-api`     | PNG export from WASM (browser `canvas` concern) |
| SVG/PNG CLI rendering via `plotters`      | Animation / interactive filtering in Rust       |
| `plotSpecToVegaLite()` in JS SDK          | Custom D3 renderers                             |
| `plot_spec_to_vega_lite()` Python utility | Oxford/ribbon/painting variants (Phase 11 dep)  |
| `include_plot_spec` flag in `params`      | Streaming / incremental rendering               |
| `genomehubs plot` CLI subcommand (basic)  |                                                 |

---

## Testing

- Unit test: `PlotSpec` round-trips through `serde_json`
- Unit test: `build_plot_spec()` populates `x`, `series`, `display` from inputs
- Unit test: CLI renderer produces valid SVG for histogram and scatter
- Unit test: `plotSpecToVegaLite` produces schema-valid Vega-Lite JSON
  (validate with `vl-convert` in JS test suite)
- API integration test: `include_plot_spec: true` in `params` returns `plot_spec`
- API integration test: without flag, `plot_spec` is absent from response
