# Phase 12: Reusable Plot Specifications

**Depends on:** Phase 6 (report types defined), Phase 10 (DisplaySpec exists), Phase 11 (positional family)
**Blocks:** nothing downstream
**Estimated scope:** 1 new module in `genomehubs-query`, CLI rendering via `plotters`, JS Vega-Lite mapping

---

## Goal

Define a `PlotSpec` type in `crates/genomehubs-query` (WASM-compatible, shared across
all SDKs) that wraps processed report data alongside display metadata. This enables:

1. **CLI rendering** — `genomehubs plot` command outputs SVG/PNG without a browser
2. **JS SDK → Vega-Lite** — `to_vega_lite()` exposed via WASM for interactive browser rendering
3. **Python SDK** — return `PlotSpec` as a dict consumable by `altair` / `plotly`
4. **R SDK** — return `PlotSpec` as a list consumable by `ggplot2` / `vegalite`

The key principle: **Rust defines the data shape and axis metadata; each platform
renders natively.** Rust does not call into rendering libraries for any platform other
than SVG/PNG (CLI). The JS SDK converts `PlotSpec` to Vega-Lite via a thin JS layer;
Python/R receive the dict/list directly.

---

## Architecture Decision: Why Not Vega-Lite from Rust?

Generating a complete Vega-Lite spec in Rust is feasible but creates tight coupling to
the Vega-Lite spec version and forces the Rust type system to mirror Vega-Lite's
extensive optional schema. Instead:

- Rust produces `PlotSpec` (a clean domain type)
- The JS SDK layer contains `plotSpecToVegaLite(spec)` (a ~100-line JS function)
- This conversion function is co-located with the rendering code and easy to update
  when Vega-Lite versions change

For the WASM path, Rust exposes `PlotSpec` as a JS object via `wasm-bindgen`. The JS
SDK receives this object and runs `plotSpecToVegaLite()` locally — no Rust ↔ JS
serialization round-trips for the conversion logic.

---

## `PlotSpec` Design (`crates/genomehubs-query/src/report/plot_spec.rs`)

A `PlotSpec` is the normalized output of any report after the data pipeline runs. It
contains:

1. `report_type` — discriminant for renderer dispatch
2. `data` — plot-ready rows (same as the `report` body the API currently returns)
3. `axes` — axis metadata (bounds, labels, scale, tick marks)
4. `display` — the `DisplaySpec` from Phase 10 (or defaults)
5. `series` — category/series breakdown for multi-series plots

```rust
use serde::{Deserialize, Serialize};
use serde_json::Value;
use super::display::DisplaySpec;

/// The type of report this spec describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportType {
    Histogram,
    Scatter,
    XPerRank,
    Sources,
    Tree,
    Map,
    Arc,
    Oxford,
    Ribbon,
    Painting,
}

/// Axis metadata for a single axis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxisMeta {
    pub field: String,
    pub label: Option<String>,
    pub scale: String,          // "linear" | "log10" | "log2" | "sqrt"
    pub domain: [f64; 2],
    pub tick_values: Vec<f64>,
    pub tick_labels: Vec<String>,
    pub value_type: String,     // "integer" | "float" | "keyword" | "coordinate"
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
/// on any platform. Produced by the report pipeline in `genomehubs-api`
/// and returned in the API response `plot_spec` field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlotSpec {
    pub report_type: ReportType,
    pub x: Option<AxisMeta>,
    pub y: Option<AxisMeta>,
    pub z: Option<AxisMeta>,    // for 2D histograms / heatmaps
    pub series: Vec<SeriesMeta>,
    pub display: DisplaySpec,
    /// Serialised plot data. Shape depends on `report_type`.
    pub data: Value,
}
```

---

## API Response Amendment

Phase 6's report endpoint currently returns:

```json
{ "status": {...}, "report": { "type": "histogram", "buckets": [...], "bounds": {...} } }
```

Phase 12 adds an optional `plot_spec` field alongside `report`:

```json
{
  "status": {...},
  "report": { ... },
  "plot_spec": {
    "report_type": "histogram",
    "x": { "field": "genome_size", "scale": "log10", "domain": [1e6, 1e12], ... },
    "series": [{ "key": "chromosome", "label": "Chromosome" }, ...],
    "display": { "title": "Genome size", "width": 800 },
    "data": { ... }
  }
}
```

`plot_spec` is generated when the request includes `display_yaml` (from Phase 10) OR
when the `include_plot_spec: true` flag is set in `params_yaml`. Without either, the
`plot_spec` field is omitted for backward compatibility.

---

## Files to Create

```
crates/genomehubs-query/src/report/plot_spec.rs   — PlotSpec type
crates/genomehubs-api/src/report/spec_builder.rs  — PlotSpec construction from pipeline output
src/cli/plot.rs                                    — `genomehubs plot` CLI subcommand
```

## Files to Modify

| File                                         | Change                                            |
| -------------------------------------------- | ------------------------------------------------- |
| `crates/genomehubs-query/src/report/mod.rs`  | `pub mod plot_spec; pub use plot_spec::PlotSpec;` |
| `crates/genomehubs-api/src/report/mod.rs`    | `pub mod spec_builder;`                           |
| `crates/genomehubs-api/src/routes/report.rs` | Build `PlotSpec` when requested                   |
| `crates/genomehubs-api/Cargo.toml`           | No new deps needed                                |
| `Cargo.toml` (workspace)                     | Add `plotters` dep to CLI crate when ready        |
| `src/main.rs`                                | Register `plot` subcommand                        |
| `crates/genomehubs-query/src/lib.rs`         | WASM export `PlotSpec`                            |
| `src/lib.rs`                                 | PyO3 export `PlotSpec`                            |
| `templates/js/query.js`                      | `plotSpecToVegaLite(spec)` helper                 |

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

| `report_type`       | Renderer                                          |
| ------------------- | ------------------------------------------------- |
| `histogram`         | `plotters::HistogramRenderer`                     |
| `scatter`           | `plotters::ScatterRenderer`                       |
| `map`               | `plotters::MapRenderer` (GeoJSON overlay)         |
| `tree`              | Newick → SVG dendrogram (custom)                  |
| `oxford` / `ribbon` | `plotters::ScatterRenderer` with chromosome track |
| `painting`          | `plotters::PaintingRenderer`                      |
| `xPerRank`          | `plotters::BarRenderer`                           |
| `sources`           | `plotters::BarRenderer`                           |
| `arc`               | `plotters::ArcRenderer` (3-circle Venn-like)      |

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

This is a pure JS function in `templates/js/query.js` (and the corresponding
generated file). It is **not** in Rust — it translates `PlotSpec` to Vega-Lite JSON.

```javascript
// Conceptual mapping — actual implementation in query.js template
export function plotSpecToVegaLite(plotSpec) {
  const base = {
    $schema: "https://vega.github.io/schema/vega-lite/v5.json",
    title: plotSpec.display?.title,
    width: plotSpec.display?.width ?? 600,
    height: plotSpec.display?.height ?? 400,
    config: buildVegaConfig(plotSpec.display),
  };

  switch (plotSpec.report_type) {
    case "histogram":
      return buildHistogramSpec(plotSpec, base);
    case "scatter":
      return buildScatterSpec(plotSpec, base);
    case "oxford":
      return buildOxfordSpec(plotSpec, base);
    // ...
  }
}
```

For positional plots (oxford, ribbon, painting), Vega-Lite's `layer` and `repeat`
primitives are used to combine the chromosome track with the scatter/ribbon layer.
Interactivity (zoom, pan, tooltip) is added via Vega-Lite's `selection` API.

---

## Python / R

Python and R receive `plot_spec` as a dict/list from the API response. No conversion
to Vega-Lite is done in Rust or via PyO3/extendr. Users apply their own renderer:

```python
# Python
import altair as alt

result = QueryBuilder().taxa(["Mammalia"]).report("histogram").x("genome_size").fetch()
spec = result["plot_spec"]

# Convert to altair (example helper, not part of the SDK itself)
chart = alt.Chart.from_dict(plot_spec_to_vega_lite(spec))
chart.save("genome_size.html")
```

The SDK provides `plot_spec_to_vega_lite()` as a utility function (mirrors the JS
version) so Python/R users can render interactively via `altair`/`vegalite` without
reimplementing the conversion.

---

## Scope Boundaries

| In scope                                  | Out of scope                                    |
| ----------------------------------------- | ----------------------------------------------- |
| `PlotSpec` type in `genomehubs-query`     | Full Vega-Lite spec generation in Rust          |
| SVG/PNG CLI rendering via `plotters`      | PNG export from WASM (browser `canvas` concern) |
| `plotSpecToVegaLite()` in JS SDK          | Animation / interactive filtering in Rust       |
| `plot_spec_to_vega_lite()` Python utility | Custom D3 renderers                             |
| `include_plot_spec` API flag              | Streaming / incremental rendering               |

---

## Testing

- Unit test: `PlotSpec` round-trips through serde_json
- Unit test: CLI renderer produces valid SVG for histogram, scatter, oxford
- Unit test: `plotSpecToVegaLite` produces schema-valid Vega-Lite JSON (validate with `vl-convert`)
- API integration test: `include_plot_spec: true` in `params_yaml` returns `plot_spec`
- API integration test: without flag, `plot_spec` is absent
