# SDK integration: `/lookup` and `/lookup/batch`

This document lists the touchpoints and recommended changes to add robust
`lookup` and `lookup_batch` support across the generated SDKs (Python, JavaScript,
and R). It focuses on preserving input order, respecting the server batch size
limit (100 items), and producing a consistent client-side API across languages.

Summary

- Server constraint: `POST /api/v3/lookup/batch` accepts at most 100 items per
  request (see `MAX_BATCH_SIZE = 100` in `crates/genomehubs-api/src/routes/lookup_batch.rs`).
- Current templates already expose `lookup` and `lookup_batch` (single-call POST),
  but they do not chunk inputs larger than 100.
- Recommendation: update SDK templates to chunk requests >100 items and to return
  a single combined response that preserves input order.

Canonical request/response shapes

Request (one batch POST):

```json
POST /v3/lookup/batch
Content-Type: application/json

{
  "lookups": [
    { "search_term": "Canis lupus", "result": "taxon", "size": 10 },
    { "search_term": "Homo sapiens", "size": 5 }
  ]
}
```

Successful response (per server):

```json
{
  "status": { /* ApiStatus object */ },
  "results": [
    { "status": { /* per-item status */ }, "results": [ {"id":"...","name":"...","rank":null,"reason":"wildcard"}, ... ] },
    { "status": { /* per-item status */ }, "results": [ ... ] }
  ]
}
```

Key requirements for SDK behaviour

- Accept the same input formats as templates currently do: list of strings or
  list of dicts/objects with `search_term`, optional `result`, optional `size`.
- Chunk input into groups of <=100 items and POST each chunk to
  `/v3/lookup/batch`.
- Merge the per-chunk `results` arrays in input order into a single combined
  LookupBatchResponse-like object and return that to callers. Compute the
  aggregated `status` (e.g., sum of hits) or return a best-effort top-level
  status with `results` concatenated.
- Do not change the per-item `results` structure returned by the server; SDKs
  should return raw JSON objects (or language-native dict/list) and allow users
  to call `parse_lookup_json` (or language-specific parsing helpers) where
  available.

Per-language touchpoints and example snippets

Python

- Template to edit: `templates/python/query.py.tera`
- Current behaviour: `lookup_batch` builds a single payload and POSTs it.
- Change: add chunking (<=100) and merge responses.

Suggested implementation (pseudocode to paste into `lookup_batch`):

```python
def lookup_batch(self, lookups, result=None, size=10):
    if not lookups:
        raise ValueError("lookup_batch() requires a non-empty lookups list")
    import json, urllib.request

    default_result = result or self._index or "taxon"
    def normalise(item):
        if isinstance(item, str):
            return {"search_term": item, "result": default_result, "size": size}
        return {
            "search_term": item["search_term"],
            "result": item.get("result", default_result),
            "size": item.get("size", size),
        }

    combined_results = []
    total_hits = 0
    url = f"{API_BASE}/v3/lookup/batch"
    for i in range(0, len(lookups), 100):
        chunk = lookups[i:i+100]
        payload = json.dumps({"lookups": [normalise(x) for x in chunk]}).encode("utf-8")
        req = urllib.request.Request(url, data=payload, headers={"Content-Type": "application/json"})
        with urllib.request.urlopen(req) as resp:
            body_text = resp.read().decode("utf-8")
        resp_obj = json.loads(body_text)
        for item in resp_obj.get("results", []):
            combined_results.append(item)
            total_hits += len(item.get("results", []))

    return {"status": {"ok": True, "hits": total_hits}, "results": combined_results}
```

Notes:

- Be explicit about chunk size `100` to match server `MAX_BATCH_SIZE`.
- Consider surfacing a `max_batch_size` constant in the SDK (module-level).

JavaScript (Node + Browser)

- Templates: `templates/js/query.js`, `templates/js/query.browser.js.tera`.
- Current behaviour: `lookupBatch` posts the whole array in a single HTTP POST.
- Change: add chunking and merging as in the Python example.

Suggested implementation (pseudocode):

```js
async lookupBatch(lookups, result=null, size=10) {
  if (!lookups || lookups.length === 0) throw new Error(...)
  const normalise = (item) => ({ /* as existing */ });
  const url = `${API_BASE}/v3/lookup/batch`;
  const combined = [];
  let totalHits = 0;
  for (let i = 0; i < lookups.length; i += 100) {
    const chunk = lookups.slice(i, i + 100).map(normalise);
    const resp = await fetch(url, { method: 'POST', headers: {...}, body: JSON.stringify({ lookups: chunk }) });
    const json = await resp.json();
    json.results.forEach(r => { combined.push(r); totalHits += r.results.length; });
  }
  return { status: { ok: true, hits: totalHits }, results: combined };
}
```

R

- Template: `templates/r/query.R` (and wrapper `templates/r/extendr-wrappers.R.tera` for bindings where applicable).
- Current behaviour: `lookup_batch` POSTs the full `lookups` array in one request.
- Change: add chunking in R using `split`/`seq` and `httr::POST` for each chunk, then merge `results`.

Suggested implementation (pseudocode):

```r
lookup_batch <- function(lookups, result = NULL, size = 10) {
  default_result <- if (is.null(result)) private$index_name else result
  normalise_item <- function(item) { ... }
  normalised <- lapply(lookups, normalise_item)
  combined <- list()
  total_hits <- 0
  url <- paste0(private$api_base_url, "/", private$api_version, "/lookup/batch")
  for (i in seq(1, length(normalised), by=100)) {
    chunk <- normalised[i:min(i+99, length(normalised))]
    payload <- jsonlite::toJSON(list(lookups = chunk), auto_unbox = TRUE)
    resp <- httr::POST(url, httr::content_type_json(), body = payload, encode = "raw")
    httr::stop_for_status(resp)
    body_text <- httr::content(resp, as = "text", encoding = "UTF-8")
    obj <- jsonlite::fromJSON(body_text, simplifyVector = FALSE)
    combined <- c(combined, obj$results)
    total_hits <- total_hits + sum(vapply(obj$results, function(x) length(x$results), integer(1)))
  }
  list(status = list(ok = TRUE, hits = total_hits), results = combined)
}
```

Docs and tests to update

- Documentation
  - `templates/docs/reference/query-builder.qmd.tera`: update the `lookup_batch` section to mention the server limit (100), that SDKs will chunk automatically and return a merged response, and show example usage.
  - `templates/shared/GETTING_STARTED.md.tera`: add CLI examples for `lookup --term` and `lookup --file` mapping to the batch endpoint.
- Tests
  - Python: add `tests/python/test_lookup_batch_chunking.py` to assert chunking behaviour and correct merging (use `responses` or a local HTTP test server to mock `/v3/lookup/batch`).
  - JS: add a Jest test using `nock` to mock HTTP POSTs and assert the client posts in chunks when given >100 inputs.
  - R: add tests using `httptest` to assert correct chunking/merging behaviour.

Generator / templates notes

- Update the templates in `templates/python/query.py.tera`, `templates/js/query.js`, `templates/js/query.browser.js.tera`, and `templates/r/query.R`.
- If you add a shared helper (e.g., `chunk_and_post`), place it consistently in templates where helper functions live so all generated SDKs use the same pattern.
- When changing templates, re-run generation (`cli-generator new` / `update`) and run the relevant SDK tests.

Verification

- After template changes, regenerate SDKs for a test site in `workdir/` and run the SDK unit tests.

Commands to run locally (examples):

```bash
# regenerate a test site (example) and run Python tests
maturin develop --features extension-module  # for Python extension where needed
pytest tests/python/test_lookup_batch_chunking.py -q

# JS tests
npm test  # or the repo's js test runner after regenerating package

# R tests
Rscript -e 'devtools::test()'  # run package tests
```

Open questions / choices

- Top-level status aggregation: implement a simple sum of per-item `results` length and return a best-effort `status` object. Alternately, return only `{"results": [...]}` and let callers inspect per-item statuses.
- Chunk size constant: expose it as an SDK-level `MAX_LOOKUP_BATCH_SIZE` constant so callers can tune or be warned.

If you'd like, I can:

- Patch the three templates to add chunking and merging code and run the unit tests, OR
- Open a PR containing only the docs changes for review.
