# Phase 6f: R and JavaScript SDK Gaps

**Depends on:** Phase 6c (ReportBuilder design established in Python), Phase 6e (Python patterns confirmed)
**Blocks:** Phase 6g (Quarto docs), Phase 6h (test parity)
**Scope:** `templates/r/query.R`, `templates/js/query.js`, `templates/r/lib.rs.tera`, `templates/r/extendr-wrappers.R.tera`

---

## Motivation

The Python SDK (library + template) is now the reference implementation for v3. Both R and JS templates still use v2 transports for `count`, `search`, and `search_all`, and neither has `report()` or `ReportBuilder`. This phase brings them to full parity with Python.

The approach is: apply the same changes Python has, adapted to each language's idioms. Since all logic lives in Rust (`parse_*`, `validate_*`, `build_url`, `_ext.*`), there is no logic to re-implement — only wiring.

---

## R Template (`templates/r/query.R`)

### R.1 — Migrate `count()` to v3 POST

**Current:** `httr::GET(counter$to_url("search"), ...)` → parses `status.hits`.

**New:**

```r
count = function() {
  payload <- list(
    query_yaml = self$to_query_yaml(),
    params_yaml = self$to_params_yaml()
  )
  resp <- httr::POST(
    paste0(private$.api_base, "/", private$.api_version, "/count"),
    body = jsonlite::toJSON(payload, auto_unbox = TRUE),
    httr::content_type_json(),
    httr::accept_json()
  )
  httr::stop_for_status(resp)
  raw_text <- httr::content(resp, as = "text", encoding = "UTF-8")
  status <- tryCatch(
    jsonlite::fromJSON(parse_response_status(raw_text)),
    error = function(e) list(hits = 0)
  )
  as.integer(as.numeric(status[["hits"]] %||% 0))
},
```

---

### R.2 — Migrate `search()` to v3 POST (JSON path)

**Current:** `httr::GET(self$to_url("search"), ...)` for all formats.

**New:** For JSON format, POST to `/v3/search`. For TSV/CSV, keep the v2 GET path (v3 POST only returns JSON):

```r
search = function(format = "tsv") {
  if (format %in% c("tsv", "csv")) {
    url <- self$to_v2_url("search")
    sep <- if (format == "tsv") "\t" else ","
    accept_type <- if (format == "tsv") "text/tab-separated-values" else "text/csv"
    response <- httr::GET(url, httr::accept(accept_type))
    httr::stop_for_status(response)
    text <- httr::content(response, as = "text", encoding = "UTF-8")
    return(utils::read.table(text = text, header = TRUE, sep = sep,
                             stringsAsFactors = FALSE, quote = "\""))
  }
  # JSON: POST to v3
  payload <- list(
    query_yaml = self$to_query_yaml(),
    params_yaml = self$to_params_yaml()
  )
  resp <- httr::POST(
    paste0(private$.api_base, "/", private$.api_version, "/search"),
    body = jsonlite::toJSON(payload, auto_unbox = TRUE),
    httr::content_type_json(),
    httr::accept_json()
  )
  httr::stop_for_status(resp)
  httr::content(resp, as = "text", encoding = "UTF-8")
},
```

---

### R.3 — Add `search_all()` with v3 cursor pagination

Currently missing from the R template. Add:

```r
#' @description Fetch all matching records using v3 cursor-based pagination.
#' @param max_records Maximum total records (NULL = no limit).
#' @return A list of record lists.
search_all = function(max_records = NULL) {
  CHUNK_SIZE <- 1000L
  cap <- if (is.null(max_records)) Inf else as.numeric(max_records)
  all_records <- list()
  search_after <- NULL
  orig_size <- private$.size
  self$set_size(CHUNK_SIZE)
  on.exit(self$set_size(orig_size), add = TRUE)

  repeat {
    payload <- list(
      query_yaml = self$to_query_yaml(),
      params_yaml = self$to_params_yaml()
    )
    if (!is.null(search_after)) {
      payload[["search_after"]] <- search_after
    }
    resp <- httr::POST(
      paste0(private$.api_base, "/", private$.api_version, "/search"),
      body = jsonlite::toJSON(payload, auto_unbox = TRUE),
      httr::content_type_json()
    )
    httr::stop_for_status(resp)
    raw_text <- httr::content(resp, as = "text", encoding = "UTF-8")
    resp_data <- jsonlite::fromJSON(raw_text, simplifyVector = FALSE)

    records <- jsonlite::fromJSON(
      parse_search_json(raw_text), simplifyVector = FALSE
    )
    remaining <- cap - length(all_records)
    all_records <- c(all_records, head(records, ceiling(remaining)))

    search_after <- resp_data[["search_after"]]
    total <- resp_data[["status"]][["hits"]] %||% 0
    if (is.null(search_after) || length(all_records) >= min(cap, total)) break
  }

  if (!is.null(max_records)) head(all_records, max_records) else all_records
},
```

---

### R.4 — Rename `to_url()` → `to_v2_url()` with deprecated alias

```r
to_v2_url = function(endpoint = "search") {
  as.character(build_url(self$to_query_yaml(), self$to_params_yaml(), endpoint))
},

to_url = function(endpoint = "search") {
  .Deprecated("to_v2_url")
  self$to_v2_url(endpoint)
},
```

---

### R.5 — Add `report()` accepting `ReportBuilder` or type string

```r
report = function(report, ...) {
  if (inherits(report, "ReportBuilder")) {
    report_yaml <- report$to_report_yaml()
  } else {
    rb <- ReportBuilder$new(report)
    # apply any named args as setters
    args <- list(...)
    if (!is.null(args$x)) rb$set_x(args$x, args$x_opts %||% "")
    if (!is.null(args$y)) rb$set_y(args$y, args$y_opts %||% "")
    if (!is.null(args$cat)) rb$set_cat(args$cat, args$cat_opts %||% "")
    if (!is.null(args$rank)) rb$set_rank(args$rank)
    # ... etc.
    report_yaml <- rb$to_report_yaml()
  }
  payload <- list(
    query_yaml = self$to_query_yaml(),
    params_yaml = self$to_params_yaml(),
    report_yaml = report_yaml
  )
  resp <- httr::POST(
    paste0(private$.api_base, "/", private$.api_version, "/report"),
    body = jsonlite::toJSON(payload, auto_unbox = TRUE),
    httr::content_type_json()
  )
  httr::stop_for_status(resp)
  result <- jsonlite::fromJSON(httr::content(resp, as = "text", encoding = "UTF-8"),
                               simplifyVector = FALSE)
  result[["report"]] %||% result
},
```

---

### R.6 — Add `ReportBuilder` R6 class

See [phase-6c-report-builder-validation.md](phase-6c-report-builder-validation.md) §7 for the full class definition. Add after `QueryBuilder` in `query.R`.

---

### R.7 — `search_batch()` and `count_batch()` internal POST helper

Currently `search_batch` and `count_batch` use inline `httr::POST` with duplicated boilerplate. Extract a private `..post_json()` method:

```r
private = list(
  .post_json = function(url, payload) {
    resp <- httr::POST(url,
      body = jsonlite::toJSON(payload, auto_unbox = TRUE),
      httr::content_type_json()
    )
    httr::stop_for_status(resp)
    httr::content(resp, as = "text", encoding = "UTF-8")
  }
)
```

Then `search_batch`, `count_batch`, and `report` all call `private$.post_json()`.

---

## JS Template (`templates/js/query.js`)

### JS.1 — Migrate `count()` to v3 POST

**Current:** `fetch(counter.toUrl(apiBase))` (GET).

**New:**

```js
async count(apiBase = API_BASE) {
  const resp = await fetch(`${apiBase}/v3/count`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      query_yaml: this.toQueryYaml(),
      params_yaml: this.toParamsYaml(),
    }),
  });
  if (!resp.ok) throw new Error(`API request failed: ${resp.status} ${resp.statusText}`);
  const data = await resp.json();
  const statusJson = _parseResponseStatus(JSON.stringify(data));
  return JSON.parse(statusJson).hits ?? 0;
},
```

---

### JS.2 — Migrate `search()` to v3 POST (JSON path)

**Current:** `fetch(this.toUrl(apiBase))` (GET).

**New:** POST for JSON; keep GET for non-JSON formats:

```js
async search(format = "json", apiBase = API_BASE) {
  if (format !== "json") {
    const url = this.toUrl(apiBase);
    const resp = await fetch(url, { headers: { Accept: _formatMime(format) } });
    if (!resp.ok) throw new Error(`API request failed: ${resp.status}`);
    return resp.text();
  }
  const resp = await fetch(`${apiBase}/v3/search`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      query_yaml: this.toQueryYaml(),
      params_yaml: this.toParamsYaml(),
    }),
  });
  if (!resp.ok) throw new Error(`API request failed: ${resp.status}`);
  return resp.json();
},
```

---

### JS.3 — Migrate `searchAll()` to v3 cursor pagination

**Current:** GET `/searchPaginated` loop (endpoint does not exist in v3).

**New:** POST `/search` loop with `search_after` cursor:

```js
async searchAll(maxRecords = Infinity, apiBase = API_BASE) {
  const CHUNK_SIZE = 1000;
  const allRecords = [];
  let searchAfter = null;
  const origSize = this._size;
  this.setSize(CHUNK_SIZE);
  try {
    while (true) {
      const payload = {
        query_yaml: this.toQueryYaml(),
        params_yaml: this.toParamsYaml(),
      };
      if (searchAfter !== null) payload.search_after = searchAfter;
      const resp = await fetch(`${apiBase}/v3/search`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload),
      });
      if (!resp.ok) throw new Error(`API request failed: ${resp.status}`);
      const data = await resp.json();
      const records = JSON.parse(_parseSearchJson(JSON.stringify(data)));
      const remaining = maxRecords - allRecords.length;
      allRecords.push(...records.slice(0, remaining));
      searchAfter = data.search_after ?? null;
      const total = data.status?.hits ?? 0;
      if (!searchAfter || allRecords.length >= Math.min(maxRecords, total)) break;
    }
  } finally {
    this.setSize(origSize);
  }
  return allRecords.slice(0, maxRecords);
},
```

---

### JS.4 — Rename `toUrl()` → `toV2Url()` with deprecated alias

```js
toV2Url(apiBase = API_BASE, apiVersion = API_VERSION, endpoint = "search") {
  // existing toUrl() implementation
  ...
}

/** @deprecated Use toV2Url() instead. */
toUrl(apiBase = API_BASE, apiVersion = API_VERSION, endpoint = "search") {
  console.warn("toUrl() is deprecated; use toV2Url() instead.");
  return this.toV2Url(apiBase, apiVersion, endpoint);
}
```

---

### JS.5 — Add `report()` accepting `ReportBuilder` or type string

```js
async report(report, { x, xOpts = "", y, yOpts = "", cat, catOpts = "",
    rank, fields, statusFilter, catRank, collapseMonotypic = false,
    preserveRank, countRank, locationField = "sample_location",
    hexResolution = 3, mapThreshold = 2000, scatterThreshold = 100,
    apiBase = API_BASE } = {}) {
  let reportYaml;
  if (report instanceof ReportBuilder) {
    reportYaml = report.toReportYaml();
  } else {
    const rb = new ReportBuilder(report);
    if (x !== undefined) rb.setX(x, xOpts);
    if (y !== undefined) rb.setY(y, yOpts);
    if (cat !== undefined) rb.setCat(cat, catOpts);
    if (rank !== undefined) rb.setRank(rank);
    // ... etc.
    reportYaml = rb.toReportYaml();
  }
  const resp = await fetch(`${apiBase}/v3/report`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      query_yaml: this.toQueryYaml(),
      params_yaml: this.toParamsYaml(),
      report_yaml: reportYaml,
    }),
  });
  if (!resp.ok) throw new Error(`API request failed: ${resp.status}`);
  const data = await resp.json();
  return data.report ?? data;
},
```

---

### JS.6 — Add `ReportBuilder` JS class

See [phase-6c-report-builder-validation.md](phase-6c-report-builder-validation.md) §8 for the full class definition. Add before the `QueryBuilder` class (or after, with `export`).

---

### JS.7 — Extract `_postJson()` helper

```js
async _postJson(url, payload) {
  const resp = await fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
  if (!resp.ok) throw new Error(`POST ${url} failed: ${resp.status}`);
  return resp.json();
}
```

Then `count`, `search` (JSON path), `searchAll`, `report`, `searchBatch`, `countBatch` all delegate to `_postJson`.

---

## Tera context for `query.js`

The `query.js` template is rendered by a separate `tera::Tera::one_off()` call in `create_js_package()` in `src/commands/new.rs`. Any new Tera variable used in the template must be added to that context. The only currently-needed addition is confirming that `api_version` and `api_base_url` are already in the context (they are, per the existing template). No new context variables are required for this phase.

---

## Verification

```bash
# After changes:
bash scripts/dev_site.sh --no-rebuild-wasm goat

# Smoke-test R
Rscript -e "
  library(goat.sdk)
  qb <- QueryBuilder\$new('taxon')
  qb\$set_taxa('Primates', 'ancestor')
  cat(qb\$count(), '\n')
"

# Smoke-test JS
node -e "
  const { QueryBuilder } = require('./workdir/goat-test-cli/js/index.js');
  const qb = new QueryBuilder('taxon');
  qb.setTaxa(['Primates'], 'ancestor');
  qb.count().then(n => console.log(n));
"
```

---

## Tests to Add

These belong in `tests/python/test_sdk_parity.py` (cross-language) and R/JS test files:

| Test                                                | File                                     |
| --------------------------------------------------- | ---------------------------------------- |
| `test_r_count_transport_is_v3_post`                 | `tests/r/test_sdk_fixtures.R`            |
| `test_r_search_json_transport_is_v3_post`           | `tests/r/test_sdk_fixtures.R`            |
| `test_r_search_all_returns_list`                    | `tests/r/test_sdk_fixtures.R`            |
| `test_r_report_histogram_returns_data`              | `tests/r/test_sdk_fixtures.R`            |
| `test_r_report_builder_chainable`                   | `tests/r/test_sdk_fixtures.R`            |
| `test_js_count_transport_is_v3_post`                | `tests/javascript/test_sdk_fixtures.mjs` |
| `test_js_search_all_cursor_loop`                    | `tests/javascript/test_sdk_fixtures.mjs` |
| `test_js_report_histogram_returns_data`             | `tests/javascript/test_sdk_fixtures.mjs` |
| `test_js_report_builder_chainable`                  | `tests/javascript/test_sdk_fixtures.mjs` |
| SDK parity: `count()` same result Python/R/JS       | `tests/python/test_sdk_parity.py`        |
| SDK parity: `report()` same result type Python/R/JS | `tests/python/test_sdk_parity.py`        |

---

## Ordering

1. R template: `to_v2_url()` rename + deprecated `to_url()` alias
2. R template: `_post_json()` private helper
3. R template: migrate `count()` to v3 POST
4. R template: migrate `search()` to v3 POST (JSON path only)
5. R template: add `search_all()` cursor loop
6. R template: add `report()` + `ReportBuilder` R6 class (from 6c)
7. JS template: `toV2Url()` rename + deprecated `toUrl()` alias
8. JS template: `_postJson()` helper
9. JS template: migrate `count()` to v3 POST
10. JS template: migrate `search()` to v3 POST (JSON path)
11. JS template: migrate `searchAll()` to cursor loop
12. JS template: add `report()` + `ReportBuilder` class (from 6c)
13. `dev_site.sh` smoke tests
14. R/JS fixture tests + parity tests
