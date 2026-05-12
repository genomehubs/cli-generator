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

| File                                         | Change                                                                          |
| -------------------------------------------- | ------------------------------------------------------------------------------- |
| `crates/genomehubs-query/src/report/mod.rs`  | `pub mod display; pub use display::DisplaySpec;`                                |
| `crates/genomehubs-api/src/routes/report.rs` | Add `display: Option<String>` to `ReportRequest`; parse + attach to response   |
| `python/cli_generator/query.py`              | `set_display()` method on `ReportBuilder`; pass in `QueryBuilder.report()`     |
| `templates/python/query.py.tera`             | Mirror `set_display()`                                                          |
| `templates/js/query.js`                      | `setDisplay()` method on `ReportBuilder`                                        |
| `templates/r/query.R`                        | `set_display()` method on `ReportBuilder`                                       |

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
subtitle: ""                 # optional secondary title
width: 800                   # plot width in pixels (default: 600)
height: 500                  # plot height in pixels (default: 400)
legend: right                # none | top | right | bottom | left
x_label: "Genome size (Gb)"
y_label: "Count"
tooltip_fields:
  - assembly_level
  - assembly_span
number_format: ".2s"         # d3 format string; default auto

# Visual style
color_scheme: tableau10      # named Vega/ColorBrewer scheme, or list of hex values
font_size: 12
line_width: 1
opacity: 0.8
marker_size: 4               # for scatter points
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
