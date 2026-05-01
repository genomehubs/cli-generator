# Phase 3: SDK Coverage for New Endpoints

**Depends on:** Phases 1–2 (API endpoints must exist before SDK methods are wired)
**Blocks:** nothing downstream — SDK methods are standalone
**Estimated scope:** ~3 template files, 2 parse functions, FFI exports (6-touchpoint checklist ×2)

---

## Goal

Add `record()`, `lookup()`, `summary()`, `msearch()` methods to all three SDK languages
(Python, JavaScript, R) following the same pattern as `count()` and `search()`.

Add `parse_record_json()` and `parse_lookup_json()` to the `genomehubs-query` parse module
and expose them via PyO3, WASM, and extendr.

Document and implement the v2/v3 API version transition strategy.

---

## 6-Touchpoint Checklist (from AGENTS.md)

For each new function (`parse_record_json`, `parse_lookup_json`):

| #   | File                                   | Action                                                                |
| --- | -------------------------------------- | --------------------------------------------------------------------- |
| 1   | `crates/genomehubs-query/src/parse.rs` | Implement the function                                                |
| 2   | `src/lib.rs`                           | Add `#[pyfunction]` + register in `#[pymodule]`                       |
| 3   | `crates/genomehubs-query/src/lib.rs`   | Add `#[cfg_attr(feature = "wasm", wasm_bindgen)]` export              |
| 4   | `templates/r/lib.rs.tera`              | Add `#[extendr]` function + register in `extendr_module!`             |
| 5   | `templates/r/extendr-wrappers.R.tera`  | Add R wrapper function                                                |
| 6   | `templates/python/query.py.tera`       | Call via binding; mirror signature in `python/cli_generator/query.py` |

Also update the `__init__.py` import list in `src/commands/new.rs`:`patch_python_init()`.

---

## Files to Modify

| File                                   | Change                                                   |
| -------------------------------------- | -------------------------------------------------------- |
| `crates/genomehubs-query/src/parse.rs` | Add `parse_record_json`, `parse_lookup_json`             |
| `src/lib.rs`                           | PyO3 exports for both new functions                      |
| `crates/genomehubs-query/src/lib.rs`   | WASM exports for both new functions                      |
| `templates/r/lib.rs.tera`              | extendr exports for both new functions                   |
| `templates/r/extendr-wrappers.R.tera`  | R wrapper stubs                                          |
| `python/cli_generator/query.py`        | `record()`, `lookup()`, `summary()`, `msearch()` methods |
| `templates/python/query.py.tera`       | Mirror the same methods                                  |
| `templates/js/query.js`                | `record()`, `lookup()`, `summary()`, `msearch()` methods |
| `templates/r/query.R`                  | `record()`, `lookup()`, `summary()`, `msearch()` methods |
| `src/commands/new.rs`                  | Update `patch_python_init()` with new function names     |

---

## Implementation

### Part A: Parse functions in `crates/genomehubs-query/src/parse.rs`

#### `parse_record_json`

Extracts a flat record dict from the `/record` response envelope.

```rust
/// Parse the `records` array from a raw `/record` API response.
///
/// Returns a JSON array string where each element is a flat dict:
/// `{ "recordId": "...", "result": "taxon", ...all _source fields... }`.
pub fn parse_record_json(raw: &str) -> Result<String, String> {
    let envelope: serde_json::Value =
        serde_json::from_str(raw).map_err(|e| format!("invalid JSON: {e}"))?;

    let records = envelope
        .get("records")
        .and_then(|r| r.as_array())
        .ok_or_else(|| "missing 'records' array in response".to_string())?;

    let flat: Vec<serde_json::Value> = records
        .iter()
        .map(|rec| {
            let mut out = serde_json::Map::new();
            // Top-level envelope fields
            if let Some(id) = rec.get("recordId").and_then(|v| v.as_str()) {
                out.insert("recordId".to_string(), serde_json::Value::String(id.to_string()));
            }
            if let Some(result) = rec.get("result").and_then(|v| v.as_str()) {
                out.insert("result".to_string(), serde_json::Value::String(result.to_string()));
            }
            // Flatten all fields from the nested `record` object
            if let Some(record_obj) = rec.get("record").and_then(|r| r.as_object()) {
                for (k, v) in record_obj {
                    out.insert(k.clone(), v.clone());
                }
            }
            serde_json::Value::Object(out)
        })
        .collect();

    serde_json::to_string(&flat).map_err(|e| format!("serialisation error: {e}"))
}
```

#### `parse_lookup_json`

Normalises the `/lookup` response into a simple list of candidates.

```rust
/// Parse the `results` array from a raw `/lookup` API response.
///
/// Returns a JSON array string: `[{ "id": "...", "name": "...", "rank": "...", "reason": "..." }]`
pub fn parse_lookup_json(raw: &str) -> Result<String, String> {
    let envelope: serde_json::Value =
        serde_json::from_str(raw).map_err(|e| format!("invalid JSON: {e}"))?;

    let results = envelope
        .get("results")
        .and_then(|r| r.as_array())
        .ok_or_else(|| "missing 'results' array in response".to_string())?;

    let candidates: Vec<serde_json::Value> = results
        .iter()
        .map(|item| {
            serde_json::json!({
                "id": item.get("id").or_else(|| item.get("taxon_id"))
                         .and_then(|v| v.as_str()).unwrap_or(""),
                "name": item.get("name").or_else(|| item.get("scientific_name"))
                            .and_then(|v| v.as_str()).unwrap_or(""),
                "rank": item.get("rank").or_else(|| item.get("taxon_rank"))
                            .and_then(|v| v.as_str()),
                "reason": item.get("reason").and_then(|v| v.as_str()).unwrap_or("match"),
            })
        })
        .collect();

    serde_json::to_string(&candidates).map_err(|e| format!("serialisation error: {e}"))
}
```

Add tests in `parse.rs`:

```rust
#[test]
fn parse_record_json_extracts_fields() {
    let raw = r#"{"status":{"success":true},"records":[{"recordId":"2759","result":"taxon","record":{"taxon_id":"2759","scientific_name":"Eukaryota"}}]}"#;
    let out = parse_record_json(raw).unwrap();
    let arr: Vec<serde_json::Value> = serde_json::from_str(&out).unwrap();
    assert_eq!(arr[0]["recordId"], "2759");
    assert_eq!(arr[0]["scientific_name"], "Eukaryota");
}

#[test]
fn parse_lookup_json_normalises_results() {
    let raw = r#"{"status":{"hits":2},"results":[{"id":"9606","name":"Homo sapiens","rank":"species","reason":"exact"}]}"#;
    let out = parse_lookup_json(raw).unwrap();
    let arr: Vec<serde_json::Value> = serde_json::from_str(&out).unwrap();
    assert_eq!(arr[0]["name"], "Homo sapiens");
    assert_eq!(arr[0]["reason"], "exact");
}
```

---

### Part B: PyO3 exports in `src/lib.rs`

Add after the existing parse function exports (e.g. after `parse_search_json`):

```rust
/// Parse the `records` array from a raw `/record` API response.
///
/// Returns a JSON array string of flat record dicts.
/// Raises `ValueError` on parse failure.
#[cfg(feature = "extension-module")]
#[pyfunction]
fn parse_record_json(raw: &str) -> PyResult<String> {
    genomehubs_query::parse::parse_record_json(raw)
        .map_err(|e| PyValueError::new_err(e))
}

/// Parse the `results` array from a raw `/lookup` API response.
///
/// Returns a JSON array string of candidate dicts with id, name, rank, reason.
/// Raises `ValueError` on parse failure.
#[cfg(feature = "extension-module")]
#[pyfunction]
fn parse_lookup_json(raw: &str) -> PyResult<String> {
    genomehubs_query::parse::parse_lookup_json(raw)
        .map_err(|e| PyValueError::new_err(e))
}
```

Register both in the `#[pymodule]` `init` function:

```rust
m.add_function(wrap_pyfunction!(parse_record_json, m)?)?;
m.add_function(wrap_pyfunction!(parse_lookup_json, m)?)?;
```

---

### Part C: WASM exports in `crates/genomehubs-query/src/lib.rs`

```rust
/// Parse the `records` array from a raw `/record` API response.
#[cfg_attr(feature = "wasm", wasm_bindgen)]
pub fn parse_record_json(raw: &str) -> String {
    crate::parse::parse_record_json(raw).unwrap_or_else(|e| format!("error: {e}"))
}

/// Parse the `results` array from a raw `/lookup` API response.
#[cfg_attr(feature = "wasm", wasm_bindgen)]
pub fn parse_lookup_json(raw: &str) -> String {
    crate::parse::parse_lookup_json(raw).unwrap_or_else(|e| format!("error: {e}"))
}
```

---

### Part D: extendr exports in `templates/r/lib.rs.tera`

```rust
/// Parse the records array from a raw /record API response.
#[extendr]
fn parse_record_json(raw: &str) -> String {
    crate::embedded::core::parse::parse_record_json(raw).unwrap_or_else(|e| format!("error: {e}"))
}

/// Parse the results array from a raw /lookup API response.
#[extendr]
fn parse_lookup_json(raw: &str) -> String {
    crate::embedded::core::parse::parse_lookup_json(raw).unwrap_or_else(|e| format!("error: {e}"))
}
```

Register in `extendr_module!`:

```rust
extendr_module! {
    ...
    fn parse_record_json;
    fn parse_lookup_json;
}
```

---

### Part E: `templates/r/extendr-wrappers.R.tera`

Add the two wrapper stubs (follows the exact same pattern as the existing wrappers):

```r
#' @export
parse_record_json <- function(raw) {
  .Call(wrap__parse_record_json, raw)
}

#' @export
parse_lookup_json <- function(raw) {
  .Call(wrap__parse_lookup_json, raw)
}
```

---

### Part F: SDK methods

The pattern for all four new methods is identical across languages: build a URL
(or a JSON body for v3), make an HTTP request, parse the response with the
appropriate parse function.

#### Python — `python/cli_generator/query.py` (and `templates/python/query.py.tera`)

Both files must have identical method signatures. Changes go into both files.

```python
def record(
    self,
    record_id: str | list[str],
    *,
    api_base: str = "https://goat.genomehubs.org/api",
    api_version: str = "v3",
) -> list[dict[str, Any]]:
    """Fetch one or more records by ID.

    Args:
        record_id: Single ID string or list of IDs.
        api_base: API base URL.
        api_version: ``"v2"`` or ``"v3"`` (default ``"v3"``).

    Returns:
        List of flat record dicts from :func:`parse_record_json`.
    """
    import json, urllib.request
    from . import parse_record_json as _parse

    ids = record_id if isinstance(record_id, list) else [record_id]
    ids_str = ",".join(ids)
    result_type = self._index

    if api_version == "v3":
        url = f"{api_base}/v3/record?recordId={ids_str}&result={result_type}"
    else:
        url = f"{api_base}/v2/record?recordId={ids_str}&result={result_type}"

    with urllib.request.urlopen(url) as resp:
        raw = resp.read().decode()
    return json.loads(_parse(raw))


def lookup(
    self,
    search_term: str,
    *,
    api_base: str = "https://goat.genomehubs.org/api",
    api_version: str = "v3",
) -> list[dict[str, Any]]:
    """Resolve a search term to matching records.

    Args:
        search_term: Term to look up (supports prefix matching).
        api_base: API base URL.
        api_version: ``"v2"`` or ``"v3"`` (default ``"v3"``).

    Returns:
        List of candidate dicts from :func:`parse_lookup_json`.
    """
    import json, urllib.request, urllib.parse
    from . import parse_lookup_json as _parse

    encoded = urllib.parse.quote(search_term)
    result_type = self._index

    if api_version == "v3":
        url = f"{api_base}/v3/lookup?searchTerm={encoded}&result={result_type}"
    else:
        url = f"{api_base}/v2/lookup?searchTerm={encoded}&result={result_type}"

    with urllib.request.urlopen(url) as resp:
        raw = resp.read().decode()
    return json.loads(_parse(raw))


def summary(
    self,
    record_id: str,
    fields: list[str],
    summary_types: list[str] | None = None,
    *,
    api_base: str = "https://goat.genomehubs.org/api",
    api_version: str = "v3",
) -> list[dict[str, Any]]:
    """Fetch field summaries for a record.

    Args:
        record_id: Record ID to summarise.
        fields: Field names to aggregate.
        summary_types: Aggregation types, e.g. ``["min", "max"]``.
        api_base: API base URL.
        api_version: ``"v2"`` or ``"v3"`` (default ``"v3"``).

    Returns:
        Raw summaries list from the API response.
    """
    import json, urllib.request

    fields_str = ",".join(fields)
    result_type = self._index
    summary_str = ",".join(summary_types) if summary_types else "min,max,mean"

    if api_version == "v3":
        url = (f"{api_base}/v3/summary?recordId={record_id}"
               f"&result={result_type}&fields={fields_str}&summary={summary_str}")
    else:
        url = (f"{api_base}/v2/summary?recordId={record_id}"
               f"&result={result_type}&fields={fields_str}&summary={summary_str}")

    with urllib.request.urlopen(url) as resp:
        raw = resp.read().decode()
    data = json.loads(raw)
    return data.get("summaries", [])


def msearch(
    self,
    queries: list["QueryBuilder"],
    *,
    api_base: str = "https://goat.genomehubs.org/api",
    api_version: str = "v3",
) -> list[list[dict[str, Any]]]:
    """Execute multiple queries in a single batch request.

    Args:
        queries: List of ``QueryBuilder`` instances to execute.
        api_base: API base URL.
        api_version: ``"v2"`` or ``"v3"`` (default ``"v3"``).

    Returns:
        List of result lists, one per query, each from :func:`parse_search_json`.
    """
    import json, urllib.request
    from . import parse_search_json as _parse

    if api_version == "v3":
        url = f"{api_base}/v3/msearch"
        payload = json.dumps({
            "searches": [
                {"query_yaml": q.to_query_yaml(), "params_yaml": q.to_params_yaml()}
                for q in queries
            ]
        }).encode()
        req = urllib.request.Request(url, data=payload,
                                     headers={"Content-Type": "application/json"})
        with urllib.request.urlopen(req) as resp:
            data = json.loads(resp.read().decode())
        return [
            json.loads(_parse(json.dumps(result)))
            for result in data.get("results", [])
        ]
    else:
        # v2: run searches sequentially (no native batch endpoint in v2 for SDK)
        return [q.search() for q in queries]
```

#### JavaScript — `templates/js/query.js`

Follow the same pattern as `count()` and `search()` already in the file.

```javascript
async record(recordId, { apiBase = this._apiBase, apiVersion = "v3" } = {}) {
    const ids = Array.isArray(recordId) ? recordId.join(",") : recordId;
    const url = `${apiBase}/${apiVersion}/record?recordId=${encodeURIComponent(ids)}&result=${this._index}`;
    const resp = await fetch(url);
    const raw = await resp.text();
    const data = JSON.parse(raw);
    return (data.records || []).map(r => ({ ...r.record, recordId: r.recordId, result: r.result }));
}

async lookup(searchTerm, { apiBase = this._apiBase, apiVersion = "v3" } = {}) {
    const url = `${apiBase}/${apiVersion}/lookup?searchTerm=${encodeURIComponent(searchTerm)}&result=${this._index}`;
    const resp = await fetch(url);
    const raw = await resp.text();
    // Use WASM parse_lookup_json if available, else parse inline
    if (typeof parse_lookup_json === "function") {
        return JSON.parse(parse_lookup_json(raw));
    }
    const data = JSON.parse(raw);
    return data.results || [];
}

async summary(recordId, fields, summaryTypes = ["min", "max", "mean"],
              { apiBase = this._apiBase, apiVersion = "v3" } = {}) {
    const fieldsStr = fields.join(",");
    const summaryStr = summaryTypes.join(",");
    const url = `${apiBase}/${apiVersion}/summary?recordId=${encodeURIComponent(recordId)}&result=${this._index}&fields=${fieldsStr}&summary=${summaryStr}`;
    const resp = await fetch(url);
    const data = await resp.json();
    return data.summaries || [];
}

async msearch(queryBuilders, { apiBase = this._apiBase, apiVersion = "v3" } = {}) {
    if (apiVersion !== "v3") {
        // v2 fallback: sequential
        return Promise.all(queryBuilders.map(q => q.search()));
    }
    const url = `${apiBase}/v3/msearch`;
    const payload = {
        searches: queryBuilders.map(q => ({
            query_yaml: q.toQueryYaml(),
            params_yaml: q.toParamsYaml(),
        }))
    };
    const resp = await fetch(url, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload),
    });
    const data = await resp.json();
    return (data.results || []).map(r => r.hits || []);
}
```

#### R — `templates/r/query.R`

```r
#' Fetch one or more records by ID
#'
#' @param record_id Character scalar or vector of record IDs.
#' @param api_base API base URL.
#' @param api_version API version string, "v2" or "v3".
#' @return A list of record lists.
#' @export
record = function(record_id, api_base = private$.api_base, api_version = "v3") {
  ids_str <- paste(record_id, collapse = ",")
  url <- sprintf("%s/%s/record?recordId=%s&result=%s",
                 api_base, api_version,
                 URLencode(ids_str, reserved = TRUE), private$.index)
  raw <- readLines(url, warn = FALSE)
  raw_str <- paste(raw, collapse = "\n")
  parsed <- parse_record_json(raw_str)
  jsonlite::fromJSON(parsed, simplifyVector = FALSE)
},

#' Resolve a search term to matching records
#'
#' @param search_term Character string to look up.
#' @param api_base API base URL.
#' @param api_version API version string.
#' @return A list of candidate lists.
#' @export
lookup = function(search_term, api_base = private$.api_base, api_version = "v3") {
  url <- sprintf("%s/%s/lookup?searchTerm=%s&result=%s",
                 api_base, api_version,
                 URLencode(search_term, reserved = TRUE), private$.index)
  raw <- readLines(url, warn = FALSE)
  raw_str <- paste(raw, collapse = "\n")
  parsed <- parse_lookup_json(raw_str)
  jsonlite::fromJSON(parsed, simplifyVector = FALSE)
},

#' Fetch field summaries for a record
#'
#' @param record_id Record ID string.
#' @param fields Character vector of field names.
#' @param summary_types Character vector of aggregation types (default min, max, mean).
#' @return A list of summary items.
#' @export
summary = function(record_id, fields, summary_types = c("min", "max", "mean"),
                   api_base = private$.api_base, api_version = "v3") {
  url <- sprintf("%s/%s/summary?recordId=%s&result=%s&fields=%s&summary=%s",
                 api_base, api_version,
                 URLencode(record_id, reserved = TRUE), private$.index,
                 paste(fields, collapse = ","),
                 paste(summary_types, collapse = ","))
  data <- jsonlite::fromJSON(url, simplifyVector = FALSE)
  data$summaries %||% list()
},
```

---

### Part G: v2/v3 API Version Transition

**Strategy:**

1. At SDK build time (or on first use), probe `GET {api_base}/v3/status`
2. If `200` → read `supported: [...]` list; store as `self._v3_supported`
3. If `404` → set `self._v3_supported = []` (all v2)
4. Each HTTP method checks: `if "/search" in self._v3_supported: use v3 else: use v2`

Add to `QueryBuilder.__init__`:

```python
self._v3_supported: list[str] | None = None  # None = not yet probed
```

Add `_probe_api_version()`:

```python
def _probe_api_version(self, api_base: str) -> list[str]:
    """Probe the API once and cache which v3 endpoints are supported."""
    if self._v3_supported is not None:
        return self._v3_supported
    import urllib.request, json
    try:
        url = f"{api_base}/v3/status"
        with urllib.request.urlopen(url, timeout=5) as resp:
            data = json.loads(resp.read().decode())
            self._v3_supported = data.get("supported", [])
    except Exception:
        self._v3_supported = []
    return self._v3_supported
```

In each HTTP method, replace hardcoded `api_version` default with:

```python
supported = self._probe_api_version(api_base)
api_version = "v3" if "/search" in supported else "v2"
```

**Fallback:** If the probe/mixed-mode logic creates complexity in generated SDK code,
use a `api_version` field in `SiteConfig` set at generation time instead.

---

## Verification

```bash
# Rebuild Python extension
maturin develop --features extension-module

# Python
python -c "
from cli_generator import parse_record_json, parse_lookup_json
print(parse_record_json('{\"records\":[{\"recordId\":\"1\",\"result\":\"taxon\",\"record\":{\"taxon_id\":\"1\"}}]}'))
print(parse_lookup_json('{\"results\":[{\"id\":\"9606\",\"name\":\"Homo sapiens\",\"reason\":\"exact\"}]}'))
"

# Full pytest suite
pytest tests/python/ -v

# SDK method test (requires live API)
python -c "
from cli_generator import QueryBuilder
qb = QueryBuilder('taxon')
recs = qb.record('2759', api_base='https://goat.genomehubs.org/api')
print(recs[0].get('scientific_name'))
"

# dev site end-to-end
bash scripts/dev_site.sh --python goat
```

---

## Completion Checklist

- [ ] `parse_record_json` implemented + unit tests in `parse.rs`
- [ ] `parse_lookup_json` implemented + unit tests in `parse.rs`
- [ ] PyO3 exports in `src/lib.rs` (registered in `#[pymodule]`)
- [ ] WASM exports in `crates/genomehubs-query/src/lib.rs`
- [ ] extendr exports in `templates/r/lib.rs.tera`
- [ ] R wrapper stubs in `templates/r/extendr-wrappers.R.tera`
- [ ] `src/commands/new.rs` `patch_python_init()` updated
- [ ] `record()`, `lookup()`, `summary()`, `msearch()` in `python/cli_generator/query.py`
- [ ] Same methods in `templates/python/query.py.tera` (identical signatures)
- [ ] Same methods in `templates/js/query.js`
- [ ] Same methods in `templates/r/query.R`
- [ ] `_probe_api_version()` added (or build-time config variable as fallback)
- [ ] `maturin develop` succeeds
- [ ] `pytest tests/python/ -v` passes
- [ ] `bash scripts/dev_site.sh --python goat` passes
