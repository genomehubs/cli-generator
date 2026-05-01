# Phase 7: Arc Reports

**Depends on:** Phase 5 (es_client), Phase 6 (`/report` route pattern established)
**Blocks:** nothing downstream
**Estimated scope:** ~1 new Rust file, 1 small helper, SDK method additions

---

## Goal

Implement arc reports: a report type that counts document overlap between two or three
query string conditions. Arc reports answer questions like "How many taxa have both
a chromosome-level genome AND genome_size > 1 GB?"

Arc is architecturally distinct from other reports:

- X, Y, Z are **query strings** (filter expressions), not field names
- No aggregation — just 3 (or 2) parallel count queries
- The response is scalar counts, not bucket arrays

---

## `report_yaml` format

```yaml
report: arc
x: "country=BR"
y: "genome_size>1000000"
z: "gc_percent>45" # optional; defaults to y if absent
```

`x`, `y`, `z` are query string fragments in the same syntax as the `taxa`/`attributes`
filter string used by the v2 URL API. They are ANDed with the main query from `query_yaml`.

---

## Files to Create

| File                                      | Purpose          |
| ----------------------------------------- | ---------------- |
| `crates/genomehubs-api/src/report/arc.rs` | Arc report logic |

## Files to Modify

| File                                         | Change                           |
| -------------------------------------------- | -------------------------------- |
| `crates/genomehubs-api/src/report/mod.rs`    | `pub mod arc;`                   |
| `crates/genomehubs-api/src/routes/report.rs` | Add `"arc"` dispatch branch      |
| `python/cli_generator/query.py`              | `arc()` method on `QueryBuilder` |
| `templates/python/query.py.tera`             | Mirror `arc()` method            |
| `templates/js/query.js`                      | `arc()` method                   |
| `templates/r/query.R`                        | `arc()` method                   |

---

## Response Shape

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
    "xQuery": "...",
    "yQuery": "...",
    "queryString": "..."
  }
}
```

| Field         | Description                                           |
| ------------- | ----------------------------------------------------- |
| `arc`         | Count matching X AND Y (and Z if provided)            |
| `arc2`        | Count matching X AND Z (only if Z provided and Z ≠ Y) |
| `x`           | Count matching X                                      |
| `y`           | Count matching Y                                      |
| `z`           | Count matching Z (only if Z provided)                 |
| `xTerm`       | Original X query string                               |
| `yTerm`       | Original Y query string                               |
| `zTerm`       | Original Z query string (only if provided)            |
| `xQuery`      | Full URL-encoded query string used for X count        |
| `yQuery`      | Full URL-encoded query string used for Y count        |
| `queryString` | Full combined query string for arc count              |

---

## Implementation

### `crates/genomehubs-api/src/report/arc.rs`

```rust
use reqwest::Client;
use serde_json::{json, Value};

use crate::es_client;

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
    let url = format!("{es_base}/{index}/_count");
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
    use super::*;

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
```

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
```

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

- [ ] `crates/genomehubs-api/src/report/arc.rs` created
- [ ] `combine_queries` + `build_term_filter` + `parse_range_term` implemented
- [ ] `run_arc_report` issues parallel counts via `tokio::try_join!`
- [ ] Unit tests for `combine_queries`, `build_term_filter`, `parse_range_term`
- [ ] `"arc"` branch added to dispatch in `routes/report.rs`
- [ ] `pub mod arc` in `report/mod.rs`
- [ ] `arc()` method in `python/cli_generator/query.py` + `templates/python/query.py.tera`
- [ ] `arc()` method in `templates/js/query.js`
- [ ] `arc()` method in `templates/r/query.R`
- [ ] `cargo test -p genomehubs-api` passes
- [ ] Smoke test returns correct `arc`, `x`, `y` counts
