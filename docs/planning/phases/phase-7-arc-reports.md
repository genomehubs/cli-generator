# Phase 7: Shared Filter-Expression Parser + Arc Reports + Tree Field Axes

**Depends on:** Phase 6a (report types implemented)
**Precedes:** Phase 6b (SDK / CLI integration) — Phase 7 moves ahead of 6b so the
full query-string config is pinned before CLI/SDK method signatures are finalised.
**Estimated scope:** ~1 new Rust module (`filter_expr.rs`), arc report file, minor
updates to tree report, SDK method additions

---

## Goal

Three things that share a common foundation and must be specced together:

1. **Shared filter-expression parser** — `parse_filter_string()` compiles a compact
   query-string expression (`genome_size>3e9`, `min(c_value)<3`, `country=BR`,
   `genome_size>1e9 AND assembly_level=Chromosome`) into a nested-attributes ES clause.
   Used by: arc report `x`/`y`/`z`, tree `status_filter`, and any future filter param.
   Phase 6 ships a minimal stub; this phase replaces it with the real thing.

2. **Tree field axes** — align tree `y`/`y_opts` (and multi-field `fields`) with the
   histogram axis config so the same SDK method opts control which summary value is
   shown (min / max / median / mean). This pins the tree SDK method signature before
   Phase 6b integration.

3. **Arc report** — count document overlap between two or three filter expressions.
   Now uses the shared parser rather than its own stub.

---

## Ordering rationale

Phase 6b (SDK integration: `tree()`, `histogram()`, `arc()` methods + CLI flags)
requires stable method signatures. The `tree()` signature depends on whether `fields`
accepts simple strings or axis-style objects with opts. Resolving this in Phase 7
before 6b avoids a breaking change to generated SDK code.

---

## 1. Shared filter-expression parser

### Grammar

```
expr     := term (AND term)*
term     := agg_term | simple_term
agg_term := AGG_FN '(' field ')' OP value        # e.g. min(genome_size)<2000000000
simple_term := field OP value                     # e.g. genome_size>3000000000
            | field '=' value                     # e.g. assembly_level=Chromosome
AGG_FN   := 'min' | 'max' | 'mean' | 'median'
OP       := '>' | '>=' | '<' | '<='
```

`AND` is case-insensitive. Whitespace around operators is ignored. A single term
with no `AND` is valid. `OR` and parentheses are deferred.

### ES mapping

All attribute fields live in a `nested` path (`attributes[]`). Every compiled term
becomes a `nested` query:

```json
{
  "nested": {
    "path": "attributes",
    "query": {
      "bool": {
        "must": [
          { "term": { "attributes.key": "<field>" } },
          <value_clause>
        ]
      }
    }
  }
}
```

`<value_clause>` depends on the term type:

| Term                 | `<value_clause>`                                                                                                                                                                                     |
| -------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `field>val`          | `{"bool":{"should":[{"range":{"attributes.long_value":{"gt":val}}},{"range":{"attributes.float_value":{"gt":val}}},{"range":{"attributes.half_float_value":{"gt":val}}}],"minimum_should_match":1}}` |
| `field=val` (string) | `{"term":{"attributes.keyword_value":val}}`                                                                                                                                                          |
| `min(field)<val`     | `{"range":{"attributes.min":{"lt":val}}}`                                                                                                                                                            |
| `max(field)>val`     | `{"range":{"attributes.max":{"gt":val}}}`                                                                                                                                                            |
| `median(field)<val`  | `{"range":{"attributes.median":{"lt":val}}}` (uses the `median` sub-field stored on each attribute entry)                                                                                            |

Multiple `AND` terms are wrapped in `{"bool":{"must":[...]}}` together with
`base_query`.

### New file

`crates/genomehubs-api/src/report/filter_expr.rs`

```rust
/// Parsed representation of a single filter term.
pub enum FilterTerm {
    /// `field op value` — applies to numeric sub-fields (long_value, float_value, half_float_value)
    NumericRange { field: String, op: &'static str, value: f64 },
    /// `agg(field) op value` — applies to the named summary sub-field (min / max / median / mean)
    AggRange { field: String, agg: String, op: &'static str, value: f64 },
    /// `field=value` — applies to keyword_value
    KeywordMatch { field: String, value: String },
}

/// Parse a filter expression string into a list of terms (AND-separated).
pub fn parse_filter_string(expr: &str) -> Result<Vec<FilterTerm>, String> { ... }

/// Compile a list of filter terms into an ES nested-attributes clause.
pub fn build_nested_attribute_query(terms: &[FilterTerm]) -> serde_json::Value { ... }

/// Convenience: parse + compile in one step, ANDed with `base_query`.
pub fn filter_expr_to_es_query(expr: &str, base_query: &serde_json::Value)
    -> Result<serde_json::Value, String> { ... }
```

### Migration

- `build_status_filter_clause` in `report_types.rs` is replaced by
  `filter_expr_to_es_query(filter_str, base_query)`.
- `build_term_filter` in `arc.rs` is replaced by the same helper.

---

## 2. Tree field axes

### Config model

The `axes` array is the canonical form for tree field configuration. It keeps field
name, scale/bounds opts, and summary together per field, and extends naturally to
multiple fields each with independent opts:

```yaml
report: tree
axes:
  - position: y
    field: genome_size
    summary: min
    scale: log10
  - position: y
    field: c_value
    summary: median
```

Each entry is deserialized as `AxisInput`, which already carries all AxisOpts fields
(`scale`, `min`, `max`, `bin_count`, `show_other`, `sort`, `interval`) plus `summary`
alongside the field name. `AxisInput.into_spec()` produces a full `AxisSpec` with
both the correct `AxisOpts` and `AxisSummary`.

**Flat shorthand** — `y:` + `y_opts:` — remains valid as a convenience alias for the
single-field case. It goes through the legacy flat-key branch in `resolve_axis_spec`,
which produces `AxisSummary::Value` (no summary selection) and parses `y_opts:` as a
standard `AxisOpts` string (`min;max;size;scale;sort;interval`). Equivalent to:

```yaml
axes:
  - position: y
    field: <value of y>
    scale: <from y_opts>
    min: <from y_opts>
    max: <from y_opts>
    # summary: Value (implicit)
```

**`y_opts` is only for the flat shorthand.** The `axes` form replaces it when
per-field opts or summary selection is needed. There is no summary slot in the
`AxisOpts` string; `summary:` is a separate structured key on the axis entry.

### Implementation

`extract_tree_field` gains a `summary: AxisSummary` parameter. `run_tree_report`
collects all `y`-role axis specs via `resolve_axis_spec` (multi-entry `axes` array,
or single-entry flat `y:` + `y_opts:` fallback), then calls
`extract_tree_field(src, field, summary)` for each.

Mapping from `AxisSummary` variant to ES sub-field for `display_value`:

| `AxisSummary` | sub-field used                                                     |
| ------------- | ------------------------------------------------------------------ |
| `Value`       | `long_value` → `float_value` → `half_float_value` (first non-null) |
| `Min`         | `min`                                                              |
| `Max`         | `max`                                                              |
| `Median`      | `median`                                                           |
| `Mean`        | `mean`                                                             |
| `Count`       | `count`                                                            |
| `Length`      | `length`                                                           |

The existing `value`, `min`, `max` keys on each field entry are always populated
(matching v2 `wantedSummaries = ["value", "min", "max"]`). `display_value` is the
additionally emitted key set to the selected summary's sub-field value.

Note: `resolve_axis_spec` currently returns `Option<AxisSpec>` (single spec per
role). For multi-field tree support it must be extended to return `Vec<AxisSpec>` for
the `Y` role, or a new `resolve_y_specs()` helper introduced. The flat `y:` shorthand
produces a single-element vec.

### Node field response shape

Each field entry in `fields` gains a `display_value` key set to the selected summary,
keeping the full `{ value, min, max, source }` shape intact for other consumers:

```json
"genome_size": {
  "value": 2.5e9,       // median (always)
  "display_value": 2.1e9, // min (selected by y_opts)
  "min": 1.8e9,
  "max": 3.2e9,
  "source": "descendant"
}
```

### SDK method signature revision

The SDK `tree()` method exposes the simple single-field case. Callers needing
per-field opts or summary control pass a raw `report_yaml` string directly to
`_run_report`, which accepts the full `axes` array form. There is no `y_opts`
parameter in the SDK method — opts are embedded in the `axes` entry alongside
the field name when per-field control is needed:

```python
def tree(
    self,
    y: str | list[str] | None = None,
    status_filter: str | None = None,
    cat: str | None = None,
    cat_opts: str = "",
    cat_rank: str | None = None,
    collapse_monotypic: bool = False,
    preserve_rank: str | None = None,
    rank: str = "phylum",
    *,
    api_base: str = "https://goat.genomehubs.org/api",
    api_version: str = "v3",
) -> list[dict[str, Any]]:
```

`y` is converted to a `fields:` sequence in the report YAML. A string becomes
`fields: [y]`; a list becomes `fields: [...]`. Both use `AxisSummary::Value` via
the flat-key path in `resolve_axis_spec`. For summary selection or per-field
scale, callers use the `axes` array via raw YAML.

---

## 3. Arc report

Unchanged from the original Phase 7 spec except `build_term_filter` is replaced by
`filter_expr_to_es_query` from `filter_expr.rs`.

### `report_yaml` format

```yaml
report: arc
x: "country=BR"
y: "genome_size>1000000"
z: "gc_percent>45" # optional
```

### Arc response shape

```json
{
  "status": { "success": true, "hits": 5432, "took": 18 },
  "report": {
    "type": "arc",
    "arc": 120,
    "arc2": 85,
    "x": 1500,
    "y": 400,
    "z": 200,
    "xTerm": "country=BR",
    "yTerm": "genome_size>1000000",
    "zTerm": "gc_percent>45",
    "queryString": "country=BR AND genome_size>1000000"
  }
}
```

---

## Files to Create / Modify

| File                                               | Change                                                                                                                                                   |
| -------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `crates/genomehubs-api/src/report/filter_expr.rs`  | New: shared filter-expression parser                                                                                                                     |
| `crates/genomehubs-api/src/report/arc.rs`          | New: arc report logic (uses filter_expr)                                                                                                                 |
| `crates/genomehubs-api/src/report/mod.rs`          | Add `pub mod filter_expr; pub mod arc;`                                                                                                                  |
| `crates/genomehubs-api/src/report/report_types.rs` | Replace `build_status_filter_clause` stub with `filter_expr_to_es_query`; add `summary: AxisSummary` param to `extract_tree_field`; emit `display_value` |
| `crates/genomehubs-api/src/routes/report.rs`       | Add `"arc"` dispatch branch                                                                                                                              |
| `python/cli_generator/query.py`                    | Revise `tree()` signature (`y` replaces `fields`, no `y_opts`); add `arc()`                                                                              |
| `templates/python/query.py.tera`                   | Mirror same changes                                                                                                                                      |
| `templates/js/query.js`                            | `tree()` + `arc()` methods                                                                                                                               |
| `templates/r/query.R`                              | `tree()` + `arc()` methods                                                                                                                               |

---

## Arc report implementation

### `crates/genomehubs-api/src/report/arc.rs`

Replace the old `build_term_filter` stub with `filter_expr::filter_expr_to_es_query`.
All other arc logic is unchanged.

/// Combine two query strings with AND, returning a merged query string.
///
/// The combined string is used to filter ES documents matching both conditions.
/// Simple concatenation with a space separator; the query parser in
/// `genomehubs-query` treats space as AND.
pub fn combine_queries(a: &str, b: &str) -> String {
if a.is_empty() {
return b.to_string();
}
if b.is_empty() {
return a.to_string();
}
format!("{a} AND {b}")
}

/// Count documents matching a query against an ES index.
async fn count_matching(
client: &Client,
es_base: &str,
index: &str,
query: &Value,
) -> Result<u64, String> {
let url = format!("{es_base}/{index}/\_count");
let body = json!({ "query": query });
let resp = client
.post(&url)
.json(&body)
.send()
.await
.map_err(|e| format!("count request failed: {e}"))?;
let data: Value = resp.json().await.map_err(|e| format!("parse error: {e}"))?;
Ok(data.get("count").and_then(|v| v.as_u64()).unwrap_or(0))
}

/// Arc report config parsed from `report_yaml`.
pub struct ArcConfig {
pub x_term: String,
pub y_term: String,
pub z_term: Option<String>,
}

impl ArcConfig {
pub fn from_yaml(config: &serde_yaml::Value) -> Result<Self, String> {
let x = config.get("x").and_then(|v| v.as_str())
.ok_or("arc report requires 'x' query string")?
.to_string();
let y = config.get("y").and_then(|v| v.as_str())
.ok_or("arc report requires 'y' query string")?
.to_string();
let z = config.get("z").and_then(|v| v.as_str()).map(|s| s.to_string());
Ok(Self { x_term: x, y_term: y, z_term: z })
}
}

/// Run an arc report: issue 3 (or 5) parallel count queries.
///
/// Returns `(total_hits, took_ms, report_data)`.
pub async fn run_arc_report(
client: &Client,
es_base: &str,
index: &str,
base_query: &Value,
config: &ArcConfig,
) -> Result<(u64, u64, Value), String> {
// Build filter queries for each term
// Terms are plain query strings; parse them into ES filter clauses
let x_filter = build_term_filter(&config.x_term);
let y_filter = build_term_filter(&config.y_term);

    // Compose AND queries
    let xy_filter = json!({ "bool": { "must": [&base_query, &x_filter, &y_filter] } });
    let x_full_filter = json!({ "bool": { "must": [&base_query, &x_filter] } });
    let y_full_filter = json!({ "bool": { "must": [&base_query, &y_filter] } });

    // Issue X, Y, and XY counts in parallel
    let (x_count, y_count, xy_count) = tokio::try_join!(
        count_matching(client, es_base, index, &x_full_filter),
        count_matching(client, es_base, index, &y_full_filter),
        count_matching(client, es_base, index, &xy_filter),
    )?;

    let took = 0u64; // individual count queries don't return combined took

    if let Some(z_term) = &config.z_term {
        // Z provided: also count XZ and total Z
        let z_filter = build_term_filter(z_term);
        let xz_filter = json!({ "bool": { "must": [&base_query, &x_filter, &z_filter] } });
        let z_full_filter = json!({ "bool": { "must": [&base_query, &z_filter] } });

        let (z_count, xz_count) = tokio::try_join!(
            count_matching(client, es_base, index, &z_full_filter),
            count_matching(client, es_base, index, &xz_filter),
        )?;

        let report_data = json!({
            "type": "arc",
            "arc": xy_count,
            "arc2": xz_count,
            "x": x_count,
            "y": y_count,
            "z": z_count,
            "xTerm": &config.x_term,
            "yTerm": &config.y_term,
            "zTerm": z_term,
            "queryString": combine_queries(&config.x_term, &config.y_term)
        });

        Ok((xy_count.max(xz_count), took, report_data))
    } else {
        let report_data = json!({
            "type": "arc",
            "arc": xy_count,
            "x": x_count,
            "y": y_count,
            "xTerm": &config.x_term,
            "yTerm": &config.y_term,
            "queryString": combine_queries(&config.x_term, &config.y_term)
        });

        Ok((xy_count, took, report_data))
    }

}

/// Build an ES filter clause from a plain query string term.
///
/// Arc query strings use the same compact syntax as the URL API:
/// `"genome_size>1000000"` → `{ "range": { "genome_size": { "gt": 1000000 } } }`
/// `"country=BR"` → `{ "term": { "country.keyword": "BR" } }`
///
/// **Implementation note:** This is a simplified parser covering the most common
/// cases. Full query string parsing should be delegated to the
/// `genomehubs_query::query` module once it supports this format.
/// For now, handle the patterns seen in production arc reports.
fn build_term_filter(term: &str) -> Value {
let term = term.trim();

    // Range operators: >, <, >=, <=
    if let Some((field, op, raw_val)) = parse_range_term(term) {
        let es_op = match op {
            ">" => "gt",
            ">=" => "gte",
            "<" => "lt",
            "<=" => "lte",
            _ => "gt",
        };
        if let Ok(n) = raw_val.parse::<f64>() {
            return json!({ "range": { field: { es_op: n } } });
        }
        return json!({ "range": { field: { es_op: raw_val } } });
    }

    // Equality: field=value
    if let Some((field, value)) = term.split_once('=') {
        let field = field.trim();
        let value = value.trim();
        // Use keyword sub-field for string values
        let es_field = format!("{field}.keyword");
        return json!({ "term": { es_field: value } });
    }

    // Fallback: treat as a query_string expression
    json!({ "query_string": { "query": term } })

}

/// Parse a range term like `"genome_size>1000000"` into (field, op, value).
fn parse_range_term(term: &str) -> Option<(&str, &str, &str)> {
for op in &[">=", "<=", ">", "<"] {
if let Some(pos) = term.find(op) {
let field = term[..pos].trim();
let value = term[pos + op.len()..].trim();
return Some((field, op, value));
}
}
None
}

#[cfg(test)]
mod tests {
use super::\*;

    #[test]
    fn combine_queries_with_both_terms() {
        assert_eq!(combine_queries("a=1", "b>2"), "a=1 AND b>2");
    }

    #[test]
    fn combine_queries_with_empty_first() {
        assert_eq!(combine_queries("", "b>2"), "b>2");
    }

    #[test]
    fn build_range_filter_gt() {
        let filter = build_term_filter("genome_size>1000000");
        assert_eq!(filter["range"]["genome_size"]["gt"], 1000000.0);
    }

    #[test]
    fn build_equality_filter() {
        let filter = build_term_filter("country=BR");
        assert_eq!(filter["term"]["country.keyword"], "BR");
    }

    #[test]
    fn parse_range_term_detects_gte() {
        let (field, op, val) = parse_range_term("assembly_span>=1000").unwrap();
        assert_eq!(field, "assembly_span");
        assert_eq!(op, ">=");
        assert_eq!(val, "1000");
    }

}

````

---

## `routes/report.rs` dispatch addition

Add to the `match report_type` block (Phase 6):

```rust
"arc" => {
    let arc_config = match report::arc::ArcConfig::from_yaml(&report_config) {
        Ok(c) => c,
        Err(e) => return Json(ReportResponse { status: ApiStatus::error(e), report: Value::Null }),
    };
    report::arc::run_arc_report(
        &state.client, &state.es_base, &idx, &base_query, &arc_config,
    ).await
}
````

---

## SDK Methods

### Python — `python/cli_generator/query.py` and `templates/python/query.py.tera`

```python
def arc(
    self,
    x: str,
    y: str,
    z: str | None = None,
    *,
    api_base: str = "https://goat.genomehubs.org/api",
    api_version: str = "v3",
) -> dict[str, Any]:
    """Run an arc report counting document overlap between query conditions.

    Args:
        x: First query string condition, e.g. ``"country=BR"``.
        y: Second query string condition, e.g. ``"genome_size>1000000"``.
        z: Optional third condition. If omitted, defaults to Y in response.
        api_base: API base URL.
        api_version: API version string.

    Returns:
        Arc report dict with keys ``arc``, ``x``, ``y``, and optionally
        ``arc2``, ``z`` (if Z provided).
    """
    import json, urllib.request

    report_yaml = f"report: arc\nx: \"{x}\"\ny: \"{y}\"\n"
    if z:
        report_yaml += f"z: \"{z}\"\n"

    url = f"{api_base}/{api_version}/report"
    payload = json.dumps({
        "query_yaml": self.to_query_yaml(),
        "params_yaml": self.to_params_yaml(),
        "report_yaml": report_yaml,
    }).encode()
    req = urllib.request.Request(url, data=payload,
                                 headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req) as resp:
        data = json.loads(resp.read().decode())
    return data.get("report", {})
```

### JavaScript — `templates/js/query.js`

```javascript
async arc(x, y, z = null, { apiBase = this._apiBase, apiVersion = "v3" } = {}) {
    let reportYaml = `report: arc\nx: "${x}"\ny: "${y}"\n`;
    if (z) reportYaml += `z: "${z}"\n`;
    const url = `${apiBase}/${apiVersion}/report`;
    const resp = await fetch(url, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
            query_yaml: this.toQueryYaml(),
            params_yaml: this.toParamsYaml(),
            report_yaml: reportYaml,
        }),
    });
    const data = await resp.json();
    return data.report || {};
}
```

### R — `templates/r/query.R`

```r
#' Run an arc report
#'
#' @param x First query string condition.
#' @param y Second query string condition.
#' @param z Optional third condition.
#' @return Arc report list with overlap counts.
#' @export
arc = function(x, y, z = NULL, api_base = private$.api_base, api_version = "v3") {
  report_yaml <- sprintf("report: arc\nx: \"%s\"\ny: \"%s\"\n", x, y)
  if (!is.null(z)) {
    report_yaml <- paste0(report_yaml, sprintf("z: \"%s\"\n", z))
  }
  url <- sprintf("%s/%s/report", api_base, api_version)
  payload <- jsonlite::toJSON(list(
    query_yaml = self$to_query_yaml(),
    params_yaml = self$to_params_yaml(),
    report_yaml = report_yaml
  ), auto_unbox = TRUE)
  resp <- httr::POST(url, httr::content_type_json(), body = payload)
  data <- httr::content(resp, as = "parsed")
  data$report %||% list()
},
```

---

## Verification

```bash
cargo test -p genomehubs-api report::arc

# Smoke test (requires live API)
curl -s -X POST http://localhost:3000/api/v3/report \
  -H 'Content-Type: application/json' \
  -d '{
    "query_yaml": "index: taxon\n",
    "params_yaml": "taxonomy: ncbi\n",
    "report_yaml": "report: arc\nx: \"assembly_level=chromosome\"\ny: \"genome_size>100000000\"\n"
  }' | jq '{type: .report.type, arc: .report.arc, x: .report.x, y: .report.y}'
```

---

## Completion Checklist

- [ ] `filter_expr.rs` created: `parse_filter_string`, `build_nested_attribute_query`, `filter_expr_to_es_query`
- [ ] Unit tests: numeric range, agg-function range, keyword match, compound AND
- [ ] `build_status_filter_clause` stub in `report_types.rs` replaced by `filter_expr_to_es_query`
- [ ] `extract_tree_field` gains `summary: AxisSummary` parameter; `display_value` set from correct sub-field per summary variant
- [ ] `tree()` SDK method revised: `y` replaces `fields`, no `y_opts`; three-language parity
- [ ] `crates/genomehubs-api/src/report/arc.rs` created; uses `filter_expr_to_es_query`
- [ ] `"arc"` dispatch branch in `routes/report.rs`
- [ ] `arc()` method in Python, JS, R
- [ ] `cargo test -p genomehubs-api` passes
- [ ] Smoke tests: `min(genome_size)<2e9`, `genome_size>3e9 AND assembly_level=Chromosome`, arc `x`/`y` counts

---

# Phase 7b: V2 Report Response Parity

**Depends on:** Phase 6 + Phase 7 (all report types implemented)
**Blocks:** Phase 8 (sign-off gates user testing)
**Estimated scope:** 1 collection script, 1 translation module, 1 parity test file, fixture JSON

---

## Goal

Verify that the v3 report API produces responses that contain all information
required to render the current GoaT site report suite, validated against real
v2 fixtures from the live production instance.

"Parity" does not mean byte-for-byte identical responses. The v3 API intentionally
diverges in field names, request format, and some edge-case behaviour. Parity
means: the v3 response contains all data a UI component needs to render, counts
are plausible, and the overall structure is credible for user testing.

---

## Step 1 — Extract real v2 API calls from GoaT server logs

The GoaT nginx access log (or application container log) records every
`/api/v2/report` request. Filter for unique `(report_type, params)` combinations
that are actually made by the live site:

```bash
# From nginx access log
grep "GET /api/v2/report" /var/log/nginx/access.log \
  | awk '{print $7}' | sort -u > /tmp/v2-report-calls.txt

# From Kubernetes container log
kubectl logs <goat-api-pod> --since=7d 2>&1 \
  | grep "GET /api/v2/report" \
  | awk -F'"' '{print $2}' | sort -u > /tmp/v2-report-calls.txt
```

If access logs are unavailable, extract calls from the GoaT site config
(dashboard panels, report definitions) and hand-craft the URL list. Keep one
entry per unique `report` type + `x`/`cat` combination — avoid duplicate
parameterisations.

---

## Step 2 — Collect v2 fixture responses

Script: `scripts/collect_parity_fixtures.py`

- Reads the URL list from Step 1
- Fetches each against `https://goat.genomehubs.org/api/v2/`
- Saves the JSON response to `tests/fixtures/parity/v2/{report_type}/{slug}.json`
- Saves the request URL alongside as `tests/fixtures/parity/v2/{report_type}/{slug}.url`
- Skips already-saved fixtures (idempotent reruns)

```
tests/fixtures/parity/
├── v2/
│   ├── histogram/
│   │   ├── genome_size_mammalia.json
│   │   ├── genome_size_mammalia.url
│   │   └── ...
│   ├── scatter/
│   ├── arc/
│   ├── tree/
│   └── ...
├── screenshots/
│   ├── genome_size_mammalia.png
│   └── ...
└── README.md   ← known divergences + sign-off log
```

At minimum, collect at least one fixture per implemented report type
(histogram, scatter, arc, tree, map, xPerRank, sources), and at least one
fixture with a `cat` breakdown.

---

## Step 3 — Capture UI screenshots for visual reference

The GoaT UI URL for each API call is the API URL with `/api/v2` stripped:

```
API: https://goat.genomehubs.org/api/v2/report?report=histogram&x=genome_size&...
UI:  https://goat.genomehubs.org/report?report=histogram&x=genome_size&...
```

Screenshots can be captured manually (paste URL into browser, screenshot) or
automatically via Playwright:

```python
# scripts/capture_parity_screenshots.py (optional automation)
from playwright.sync_api import sync_playwright

def capture(api_url: str, out_path: str) -> None:
    ui_url = api_url.replace("https://goat.genomehubs.org/api/v2/", "https://goat.genomehubs.org/")
    with sync_playwright() as p:
        browser = p.chromium.launch()
        page = browser.new_page(viewport={"width": 1280, "height": 900})
        page.goto(ui_url, wait_until="networkidle")
        page.screenshot(path=out_path)
        browser.close()
```

Screenshots are stored in `tests/fixtures/parity/screenshots/` and used for
human sign-off only — they are not asserted programmatically.

---

## Step 4 — V2 → V3 request translation

v2 uses GET query strings; v3 uses a JSON POST body. The translation module
`tests/parity/translate.py` maps them:

| v2 GET param        | v3 JSON location           | Notes                    |
| ------------------- | -------------------------- | ------------------------ |
| `report`            | `report.report`            | Direct map               |
| `x`, `y`, `z`       | `report.x/y/z`             | Direct map               |
| `x_opts`, `y_opts`  | `report.x_opts/y_opts`     | Direct map               |
| `cat`, `cat_opts`   | `report.cat/cat_opts`      | Direct map               |
| `result`            | `query.index`              | `"taxon"` / `"assembly"` |
| `rank`              | `query.rank`               | Direct map               |
| `includeEstimates`  | `params.include_estimates` | snake_case rename        |
| `taxonomy`          | `params.taxonomy`          | Direct map               |
| `query` (taxa/attr) | `query.taxa`               | Wrap in array            |
| `fields`            | `query.fields`             | Direct map               |

v2 also passes `report`, `size`, `offset`, and some rendering hints that v3 does
not need — these are dropped during translation.

---

## Step 5 — Parity test harness

`tests/parity/test_report_parity.py` — parametrised over all v2 fixtures:

```python
@pytest.mark.parametrize("fixture_path", collect_v2_fixture_paths())
def test_report_parity(fixture_path: Path, local_v3_base: str) -> None:
    v2_response = json.loads(fixture_path.read_text())
    request_url = fixture_path.with_suffix(".url").read_text().strip()
    v3_body = translate_v2_url_to_v3_body(request_url)

    v3_response = httpx.post(f"{local_v3_base}/api/v3/report", json=v3_body).json()

    assert_structural_parity(
        v2_report=v2_response.get("report", {}),
        v3_report=v3_response.get("report", {}),
    )
```

`assert_structural_parity` checks (defined in `tests/parity/assertions.py`):

1. `type` field matches between v2 and v3 response.
2. All required rendering keys for that type are present and non-null in v3.
3. Where v2 has a non-empty array, v3 has a non-empty array of the same structure.
4. Where v2 returns a count > 0, v3 returns a count > 0 (values need not match —
   the live dataset grows over time).
5. `by_cat` present in v3 whenever `cat` was in the v2 request.

Required keys per report type:

| Type        | Required v3 keys                                                              |
| ----------- | ----------------------------------------------------------------------------- |
| `histogram` | `type`, `x.field`, `x.domain`, `buckets`, `allValues`                         |
| `scatter`   | `type`, `x`, `y`, `buckets`, `allValues`, `yBuckets`, `allYValues`, `zDomain` |
| `arc`       | `type`, `arc`, `x`, `y`, `xTerm`, `yTerm`                                     |
| `tree`      | `type`, `tree` (Newick string or node array)                                  |
| `map`       | `type`, `map` or equivalent location array                                    |
| `xPerRank`  | `type`, `ranks` or equivalent rank-keyed object                               |
| `sources`   | `type`, `sources`                                                             |

When `cat` is present, additionally assert `by_cat` and `cats`.

---

## Allowed divergences

Document each known divergence in `tests/fixtures/parity/README.md` and mark
the corresponding test with `@pytest.mark.xfail(reason="…")` rather than
leaving it as a hard failure.

| Divergence class         | Detail                                                     |
| ------------------------ | ---------------------------------------------------------- |
| Status envelope shape    | v2 uses `status.success`; v3 uses `status.hits`/`took`     |
| Field renames            | Document case-by-case                                      |
| Numeric precision        | Bounds/interval values may differ ±epsilon                 |
| Unimplemented type in v3 | Mark fixture as `xfail`; do not fail the suite             |
| Count growth             | v2 fixture counts are historical; v3 live counts will be ≥ |
| Extra v3 fields          | `rawData` on scatter, `allValues` on histogram — permitted |

---

## Sign-off criteria

The phase is complete — and report work is cleared for user testing — when:

- [ ] At least one fixture per implemented report type collected
- [ ] At least one categorised histogram fixture (`cat` + `cat_opts`) collected
- [ ] UI screenshots reviewed: rendered output looks visually equivalent to the current GoaT site
- [ ] All non-xfail parity tests pass against local v3 server
- [ ] All xfail divergences documented with rationale in `tests/fixtures/parity/README.md`
- [ ] Developer sign-off statement added to `tests/fixtures/parity/README.md`

---

## Files to Create

| File                                    | Purpose                                    |
| --------------------------------------- | ------------------------------------------ |
| `scripts/collect_parity_fixtures.py`    | Fetch v2 fixtures from live GoaT API       |
| `scripts/capture_parity_screenshots.py` | Optional: Playwright screenshot automation |
| `tests/parity/__init__.py`              | Package marker                             |
| `tests/parity/translate.py`             | v2 GET params → v3 JSON body               |
| `tests/parity/assertions.py`            | Per-type structural assertion helpers      |
| `tests/parity/test_report_parity.py`    | Parametrised parity tests                  |
| `tests/fixtures/parity/README.md`       | Known divergences + sign-off log           |

---

## Completion Checklist

- [ ] `scripts/collect_parity_fixtures.py` fetches and saves v2 fixtures idempotently
- [ ] At least one fixture per implemented report type
- [ ] At least one fixture with `cat` breakdown
- [ ] UI screenshots captured for all fixtures
- [ ] `tests/parity/translate.py` handles all v2 param combinations in collected fixtures
- [ ] `tests/parity/test_report_parity.py` runs with `pytest tests/parity/ -v`
- [ ] All non-xfail tests pass against local v3 server
- [ ] All xfail divergences documented with rationale
- [ ] Developer sign-off in `tests/fixtures/parity/README.md`
