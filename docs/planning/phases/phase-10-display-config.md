# Phase 10: Display Configuration (`display_yaml`)

**Depends on:** Phase 6 (report endpoint exists)
**Blocks:** Phase 12 (PlotSpec uses DisplaySpec as its presentation layer)
**Estimated scope:** 1 new type module, amendment to Phase 6 request schema, SDK method additions

---

## Goal

Extend every report endpoint (`/api/v3/report`, `/api/v3/arc`, `/api/v3/positional`)
with an optional `display_yaml` field that controls how the result should be presented.

This separates two orthogonal concerns:

- `report_yaml` — **what** data to aggregate and in what shape (histogram, scatter, etc.)
- `display_yaml` — **how** to present that data (title, dimensions, colours, labels)

`display_yaml` is optional everywhere. Servers process it passively: they parse it,
attach a `DisplaySpec` to the response, and return it unchanged. Rendering is always
client-side. The server never draws pixels.

**Do not add a separate `style_yaml`.** Layout and style are both presentation concerns
and the consumer (CLI, browser, SDK) needs them together. Internal grouping (layout vs.
visual) is handled by named subsections within `display_yaml`.

---

## Files to Create

```
crates/genomehubs-query/src/report/display.rs   — DisplaySpec type
```

## Files to Modify

| File                                            | Change                                                                         |
| ----------------------------------------------- | ------------------------------------------------------------------------------ |
| `crates/genomehubs-query/src/report/mod.rs`     | `pub mod display; pub use display::DisplaySpec;`                               |
| `crates/genomehubs-api/src/routes/report.rs`    | Add `display_yaml: Option<String>` to request body; parse + attach to response |
| `crates/genomehubs-query/src/report/builder.rs` | `display()` method on `ReportBuilder`                                          |
| `python/cli_generator/query.py`                 | `display()` method                                                             |
| `templates/python/query.py.tera`                | Mirror `display()`                                                             |
| `templates/js/query.js`                         | `display()` method                                                             |
| `templates/r/query.R`                           | `display()` method                                                             |

---

## `display_yaml` Format

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

All keys are optional. Missing keys use renderer defaults.

The split into presentation (title, dimensions, legend, labels) vs. visual style
(colours, fonts, sizes) is logical, but both live in the same YAML to avoid asking
the user "which file does this go in?".

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
/// Parsed from `display_yaml` in the API request and attached to the response
/// unchanged. Rendering is always client-side.
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

All endpoints that currently accept `{ query_yaml, params_yaml, report_yaml }` gain
an optional fourth field:

```json
{
  "query_yaml": "...",
  "params_yaml": "...",
  "report_yaml": "...",
  "display_yaml": "title: Genome size\nwidth: 800\ncolor_scheme: tableau10\n"
}
```

The response includes a `display` key alongside `report`:

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

If `display_yaml` is absent, the `display` key is omitted from the response.

---

## SDK: `ReportBuilder.display()` Method

```python
# Python
builder = (
    QueryBuilder()
    .taxa(["Mammalia"])
    .report("histogram")
    .x("genome_size")
    .display("title: Genome size\nwidth: 800\n")
)
```

The `display()` method on `ReportBuilder` accepts a YAML string (same interface as
`query()`, `params()`, `report()`). The convenience overloads (`title()`, `width()`,
etc.) are out of scope — keep the interface consistent with the existing builder pattern.

---

## Vega-Lite Mapping (JS SDK)

When the JS SDK renders a report as Vega-Lite, it maps `DisplaySpec` fields to the
Vega-Lite spec as follows:

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

This mapping lives in the JS SDK layer, not in Rust. The Rust `PlotSpec` (Phase 12)
exposes a `to_vega_lite(display: &DisplaySpec) -> Value` method that performs this
mapping via WASM.

---

## Testing

- Unit test: `DisplaySpec::default()` produces all-None struct
- Unit test: round-trip YAML → `DisplaySpec` → serde_json
- Proptest: fuzz YAML input, assert no panic on deserialization
- API integration test: `POST /api/v3/report` with `display_yaml` returns `display` key
- API integration test: `POST /api/v3/report` without `display_yaml` returns no `display` key
