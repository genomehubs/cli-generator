# Phase 6b: V3 API — SDK & CLI Migration

**Supersedes:** the original Phase 6b stub referenced in phase-6-report-types.md
**Depends on:** Phase 6 (all report types implemented), Phase 7 (arc + filter-expression parser)
**Blocks:** Phase 3c test parity against v3; Phase 15 cross-query reports

---

## Scope

The v3 API is now a substantially complete REST implementation (POST-body JSON/YAML
throughout) rather than the URL-parameter-based v2 API. The SDK and CLI currently
target v2 exclusively (except for `search_batch`, `count_batch`, `record`, `lookup`,
and `summary` which already use v3). This phase migrates everything to v3 and adds
minimal v2 fallback for parity checking during the transition period.

---

## Current State Audit

### V3 API — implemented endpoints (as of 2026-05-08)

| Endpoint          | Method | Notes                                                                            |
| ----------------- | ------ | -------------------------------------------------------------------------------- |
| `/status`         | GET    | Returns `supported` list — **needs updating** (see §3)                           |
| `/resultFields`   | GET    | —                                                                                |
| `/taxonomies`     | GET    | —                                                                                |
| `/taxonomicRanks` | GET    | —                                                                                |
| `/indices`        | GET    | —                                                                                |
| `/count`          | POST   | JSON/YAML body                                                                   |
| `/countBatch`     | POST   | JSON/YAML body                                                                   |
| `/search`         | POST   | JSON/YAML body; returns `{status, url, results, search_after}`                   |
| `/searchBatch`    | POST   | JSON/YAML body                                                                   |
| `/record`         | GET    | —                                                                                |
| `/lookup`         | GET    | —                                                                                |
| `/summary`        | POST   | JSON/YAML body                                                                   |
| `/report`         | POST   | JSON/YAML body; dispatches histogram, scatter, xPerRank, sources, tree, map, arc |

`/searchPaginated` does **not** exist in v3; pagination is cursor-based via
`search_after` in the `/search` response.

### V3 `/search` response envelope

```json
{
  "status": { "success": true, "hits": 126, "took": 12 },
  "url": "/api/v3/search",
  "results": [
    {
      "index": "taxon",
      "id": "9608",
      "score": null,
      "result": {
        "taxon_id": "9608",
        "scientific_name": "Canis lupus",
        "taxon_rank": "species",
        "parent": "9612",
        "fields": {
          "genome_size": {
            "value": 2400000000,
            "min": 2100000000,
            "aggregation_source": "direct"
          },
          "c_value": { "value": 2.8, "aggregation_source": "descendant" }
        }
      }
    }
  ],
  "search_after": [9608]
}
```

Key differences from v2:

- `results[]` structure is `{index, id, score, result}` — same shape, but
  `result.fields` values come from `inner_hits` docvalue extraction, not
  v2's full `_source.attributes` array.
- `search_after` cursor replaces v2's page-number offset pagination.
- No `/searchPaginated` endpoint; use repeated `/search` POST with `search_after`.

### V3 `/report` response envelope

```json
{
  "status": { "success": true, "hits": 126, "took": 42 },
  "report": { ... report-type-specific data ... }
}
```

### SDK/CLI current state

| Method           | Current API     | Transport                                       |
| ---------------- | --------------- | ----------------------------------------------- |
| `to_url()`       | v2              | GET — URL-param query string                    |
| `count()`        | v2              | GET → `to_url()`                                |
| `search()`       | v2              | GET → `to_url()`                                |
| `search_all()`   | v2              | GET → `/searchPaginated` (does not exist in v3) |
| `search_batch()` | v3              | POST                                            |
| `count_batch()`  | v3              | POST                                            |
| `record()`       | v3              | POST                                            |
| `lookup()`       | v3              | POST                                            |
| `summary()`      | v3              | POST                                            |
| `report()`       | not implemented | —                                               |

`parse_search_json` already handles the v3 `results[]` envelope shape (the
`{index, id, score, result}` wrapper is what it consumes). The parse path for
the body is compatible — **no changes to `parse.rs` are needed for search**.

---

## Work Items

### 1. Update `/status` `supported` list

The `SUPPORTED_ENDPOINTS` constant in `status.rs` is stale. Update it to reflect
the full v3 surface. The list is the canonical signal the CLI/SDK uses to detect
v3 capability:

```rust
const SUPPORTED_ENDPOINTS: &[&str] = &[
    "/status",
    "/resultFields",
    "/taxonomies",
    "/taxonomicRanks",
    "/indices",
    "/count",
    "/countBatch",
    "/search",
    "/searchBatch",
    "/record",
    "/lookup",
    "/summary",
    "/report",
];
```

**Files:** `crates/genomehubs-api/src/routes/status.rs`

---

### 2. Version negotiation helper

Add a single async helper that the SDK/CLI calls once per session to determine
which API version an instance supports. It hits `/status` and inspects
`supported`:

**Location:** `crates/genomehubs-query/src/api_version.rs` (new file)

```rust
/// Detected API capability of a remote genomehubs instance.
#[derive(Debug, Clone, PartialEq)]
pub enum ApiCapability {
    /// Full v3 POST-body API with all endpoints.
    V3,
    /// Legacy v2 GET URL-parameter API.
    V2,
}

/// Probe an API base URL and return its capability.
///
/// Calls `{api_base}/v3/status`. If the response is valid JSON and
/// `supported` contains `"/search"`, returns `V3`.
/// Falls back to `V2` on any error or missing endpoint.
pub fn probe_api_capability(api_base: &str) -> ApiCapability { ... }
```

This is a blocking call (matches the existing SDK transport pattern of
`urllib.request`). It can be cached in the Python `QueryBuilder` instance.

**Files:** `crates/genomehubs-query/src/api_version.rs`, `src/lib.rs` (PyO3 export),
`python/cli_generator/query.py` (call in `search()`, `count()`, etc.)

---

### 3. Migrate `count()` to v3 POST + improve CLI count support

**Current:** `GET {api_base}/v2/search?...` then reads `status.hits`. The CLI
`count` subcommand is less capable than the API: it lacks attribute filter support,
rank filtering, and any output beyond a bare integer.

**New:** `POST {api_base}/v3/count` with `{query_yaml, params_yaml}` body.

The v3 `/count` endpoint returns `{status: {hits, success, took}}` — the same
`parse_response_status` path works unchanged.

```python
def count(
    self,
    api_base: str = "https://goat.genomehubs.org/api",
    api_version: str = "v3",
) -> int:
    url = f"{api_base}/{api_version}/count"
    payload = {"query_yaml": self.to_query_yaml(), "params_yaml": self.to_params_yaml()}
    # POST, parse status.hits
```

V2 fallback: if `api_version == "v2"`, use the existing GET → `to_url()` path.

**CLI improvements needed alongside this migration:**

- Attribute filter flags (`--filter`, `--exclude-ancestral`, etc.) should be
  wired to the CLI `count` subcommand to match what `search` already accepts.
- Output should optionally include `took` and the query YAML used, for debugging.
- A `--count-batch` mode (accepting a list of queries) should delegate to
  `count_batch()` and print one count per line.

**Files:** `python/cli_generator/query.py`, `templates/python/query.py.tera`,
`templates/js/query.js`, `templates/r/query.R`, `src/main.rs` (CLI flags)

---

### 4. Migrate `search()` to v3 POST

**Current:** `GET {api_base}/v2/search?...`
**New:** `POST {api_base}/v3/search` with `{query_yaml, params_yaml}` body.

The v3 response `results[]` array is already in the shape that `parse_search_json`
consumes. No parse changes needed.

```python
def search(
    self,
    format: str = "json",
    api_base: str = "https://goat.genomehubs.org/api",
    api_version: str = "v3",
) -> Any:
    if api_version != "v3":
        # v2 fallback: GET path (existing implementation)
        return self._search_v2(format, api_base, api_version)
    url = f"{api_base}/{api_version}/search"
    payload = {"query_yaml": self.to_query_yaml(), "params_yaml": self.to_params_yaml()}
    # POST, return raw JSON or parse
```

**Files:** `python/cli_generator/query.py`, `templates/python/query.py.tera`,
`templates/js/query.js`, `templates/r/query.R`

---

### 5. Migrate `search_all()` to v3 cursor pagination

**Decision: handle in SDK — do not add `/searchPaginated` to v3.**

The v3 `/search` response already carries `search_after` when there are more
pages. A separate endpoint would be a thin wrapper doing exactly what the SDK
loop does, adding API surface to maintain. The UI already handles cursor-based
pagination client-side. The only cost of not having the endpoint is that
non-SDK HTTP clients must loop manually — acceptable given the v3 audience is
mostly SDK users.

**Current:** `GET /searchPaginated?searchAfter=...` (endpoint does not exist in v3).
**New:** Repeated `POST /search` with `search_after` cursor from each response.

```python
def search_all(
    self,
    max_records: int | None = None,
    api_base: str = "https://goat.genomehubs.org/api",
    api_version: str = "v3",
) -> list[dict[str, Any]]:
    all_records: list[dict[str, Any]] = []
    search_after: Any = None
    while True:
        payload = {"query_yaml": self.to_query_yaml(), "params_yaml": self.to_params_yaml()}
        if search_after is not None:
            payload["search_after"] = search_after
        resp = self._post_json(f"{api_base}/{api_version}/search", payload)
        records = json.loads(parse_search_json(json.dumps(resp)))
        all_records.extend(records)
        search_after = resp.get("search_after")
        total = resp.get("status", {}).get("hits", 0)
        if not search_after or len(all_records) >= (max_records or total):
            break
    return all_records[:max_records]
```

V2 fallback: if `api_version == "v2"`, use the existing `searchPaginated` GET path.

Note: the `params_yaml` must include `size` so each page is fetched at the
configured chunk size. Default page size for `search_all` remains 1 000.

**Files:** `python/cli_generator/query.py`, `templates/python/query.py.tera`,
`templates/js/query.js`, `templates/r/query.R`

---

### 6. Add `report()` method to `QueryBuilder`

New method covering all report types. The report configuration is passed as a
`dict` / named object and serialised to YAML internally.

```python
def report(
    self,
    report_type: str,
    *,
    x: str | None = None,
    x_opts: str = "",
    y: str | list[str] | None = None,
    y_opts: str = "",
    cat: str | None = None,
    cat_opts: str = "",
    rank: str | None = None,
    fields: list[str] | None = None,
    status_filter: str | None = None,
    cat_rank: str | None = None,
    collapse_monotypic: bool = False,
    preserve_rank: str | None = None,
    count_rank: str | None = None,
    location_field: str = "sample_location",
    hex_resolution: int = 3,
    map_threshold: int = 2000,
    scatter_threshold: int = 100,
    api_base: str = "https://goat.genomehubs.org/api",
    api_version: str = "v3",
) -> Any:
    """Run a report and return the raw report data dict.

    Args:
        report_type: One of ``histogram``, ``scatter``, ``xPerRank``,
            ``sources``, ``tree``, ``map``, ``arc``.
        x: X-axis field name.
        ...
    """
```

Internally constructs `report_yaml` and POSTs to `/report`. Returns the raw
`report` dict from the response (callers can parse further with dedicated
helpers — see §7).

No v2 fallback: `/report` did not exist in v2 with this interface.

**Files:** `python/cli_generator/query.py`, `templates/python/query.py.tera`,
`templates/js/query.js`, `templates/r/query.R`

---

### 7. Parse functions for report types

Add to `crates/genomehubs-query/src/parse.rs` and expose via PyO3/WASM/extendr.

#### `parse_histogram_json(raw: &str) -> String`

Extracts `report.buckets` from a `/report` response. Returns a compact JSON
array of bucket objects. `by_cat` is preserved on each bucket when present.

#### `parse_tree_json(raw: &str) -> String`

Flattens `report.treeNodes` into a JSON array with one object per node:

```json
[
  {
    "taxon_id": "9608",
    "scientific_name": "Canidae",
    "taxon_rank": "family",
    "count": 96,
    "descendant_count": 63,
    "status": 0,
    "cat": null,
    "children": ["9611"],
    "fields": {
      "genome_size": { "value": 2.4e9, "aggregation_source": "direct" }
    }
  }
]
```

**Files:** `crates/genomehubs-query/src/parse.rs`, `src/lib.rs` (PyO3),
`crates/genomehubs-query/src/lib.rs` (WASM),
`templates/r/lib.rs.tera` + `extendr-wrappers.R.tera`,
`python/cli_generator/query.py` (convenience wrappers),
`python/cli_generator.pyi` (stubs)

---

### 8. Rename `to_url()` → `to_v2_url()` and add `from_v2_url()`

`to_url()` builds a GET URL that is only meaningful for the v2 query-string
API. After this migration:

- Rename to `to_v2_url()`. Keep `to_url()` as a deprecated alias emitting a
  `DeprecationWarning`.
- In `count()`, `search()`, `search_all()`, the URL-building path is only
  used when the caller explicitly passes `api_version="v2"`.

**Add `from_v2_url(url: str) -> QueryBuilder` (class method):**

Parses a v2 GET URL back into a fully populated `QueryBuilder`. This lets users
migrate existing bookmarks, scripts, and UI-copied URLs by converting them to
v3-compatible POST bodies:

```python
@classmethod
def from_v2_url(cls, url: str) -> "QueryBuilder":
    """Reconstruct a QueryBuilder from a v2 API GET URL.

    Parses the query-string parameters into the equivalent
    query_yaml / params_yaml state. Useful for migrating
    v2 bookmarks or UI-copied URLs to v3 POST body queries.
    """
```

The implementation parses the URL query string into `SearchQuery` and
`QueryParams` via `query_yaml_from_url_params` (new Rust function in
`crates/genomehubs-query/src/query/mod.rs`). This reverses the logic in
`build_query_url`.

**Relationship to Phase 9 (URL query strings):**

Phase 9 targets GET query strings as an _alternative input format_ for the
v3 server (i.e. the server parses GET params and converts them to POST body
internally). `from_v2_url()` is the _client-side_ complement of that work:
it converts a v2 URL to a builder without any server involvement. The two
approaches solve the same problem from different directions. Full server-side
GET parameter support (Phase 9) is deferred until after 6b; reproducible URLs
for the UI are covered by `to_ui_url()` which is unaffected.

**Files:** `python/cli_generator/query.py`, `templates/python/query.py.tera`,
`crates/genomehubs-query/src/query/mod.rs` (new `query_yaml_from_url_params`),
`src/lib.rs` (PyO3), `python/cli_generator.pyi`

---

### 9. Transport helper

The multiple methods that now POST JSON share the same boilerplate. Extract a
private `_post_json` helper:

```python
def _post_json(self, url: str, payload: dict[str, Any]) -> Any:
    import json
    import urllib.request
    req = urllib.request.Request(
        url,
        data=json.dumps(payload).encode("utf-8"),
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req) as resp:
        return json.loads(resp.read().decode("utf-8"))
```

**Files:** `python/cli_generator/query.py`, `templates/python/query.py.tera`

---

### 10. Default version constants

Change SDK method defaults from `"v2"` to `"v3"` everywhere. Keep `"v2"` as an
accepted value and implement fallback paths for:

- `count()` → GET `/v2/search`
- `search()` → GET `/v2/search`
- `search_all()` → GET `/v2/searchPaginated`

No v2 fallback is needed for `record`, `lookup`, `summary`, `search_batch`,
`count_batch` (they already target v3 and these endpoints have direct equivalents).

---

### 11. CLI update

The CLI entry points (`goat-cli` binary via `src/main.rs`) use the same
`QueryBuilder` methods. No CLI-specific changes are needed beyond what the SDK
migration covers, _except_ for the `--url` / `--ui-url` commands which use
`to_url()` for their output. These should:

- Default to v2 URL output for now (the v3 POST body cannot be expressed as a
  shareable URL)
- Add a `--v3-body` flag that prints the POST body JSON instead of a URL

---

## Ordering

| Step                             | Depends on | Can parallelise with |
| -------------------------------- | ---------- | -------------------- |
| 1. Update `supported` list       | —          | all others           |
| 2. `probe_api_capability` helper | 1          | —                    |
| 3. Migrate `count()`             | 2          | 4, 5                 |
| 4. Migrate `search()`            | 2          | 3, 5                 |
| 5. Migrate `search_all()`        | 4          | —                    |
| 6. Add `report()` method         | 4          | 7                    |
| 7. Parse functions               | —          | 6                    |
| 8. Rename `to_url()`             | 3, 4, 5    | —                    |
| 9. Transport helper `_post_json` | —          | 3, 4, 5, 6           |
| 10. Default version constants    | 3, 4, 5    | —                    |
| 11. CLI `--v3-body` flag         | 8          | —                    |

Recommended sequence: 9 → 1 → (3, 4, 7 in parallel) → 5 → 6 → 2 → 10 → 8 → 11.

---

## V2 Fallback Policy

V2 fallback is **not** the default path. It is an explicit opt-in via
`api_version="v2"`. SDK defaults always target v3. The `probe_api_capability`
helper is available for automated detection when the instance version is unknown.

### Fallback coverage

| Method           | V2 equivalent                           | Fallback transport  |
| ---------------- | --------------------------------------- | ------------------- |
| `count()`        | `GET /v2/search?size=0` → `status.hits` | GET                 |
| `search()`       | `GET /v2/search?...`                    | GET                 |
| `search_all()`   | `GET /v2/searchPaginated?...`           | GET (loop)          |
| `record()`       | `GET /v2/record?...`                    | GET                 |
| `lookup()`       | `GET /v2/lookup?...`                    | GET                 |
| `summary()`      | `GET /v2/summary?...`                   | GET                 |
| `search_batch()` | No v2 equivalent                        | raises `ValueError` |
| `count_batch()`  | No v2 equivalent                        | raises `ValueError` |
| `report()`       | No v2 equivalent                        | raises `ValueError` |

Note: `record`, `lookup`, and `summary` currently default to v3; their v2
fallback paths need to be added alongside the `count` / `search` migration
so that `api_version="v2"` is consistently handled across all GET-capable methods.

---

## Tests to Add / Update

| Test file                           | What to add                                                           |
| ----------------------------------- | --------------------------------------------------------------------- |
| `tests/python/test_sdk_fixtures.py` | Add fixture entries for `search()` and `count()` against local v3 API |
| `tests/python/test_sdk_parity.py`   | Parity tests: same query via v2 URL vs v3 POST returns same hit count |
| `tests/python/test_core.py`         | Tests for `parse_histogram_json`, `parse_tree_json`                   |
| `tests/python/test_sdk_fixtures.py` | Add `report()` fixture (histogram over Canidae)                       |

SDK parity tests require both the local v3 API (`genomehubs-api`) and a reachable
v2 instance. Mark them with `@pytest.mark.integration` and skip when either is
unavailable.
