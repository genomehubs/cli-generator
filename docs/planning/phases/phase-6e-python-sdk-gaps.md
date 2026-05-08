# Phase 6e: Python SDK Remaining Gaps

**Depends on:** Phase 6b (core v3 migration — Python done), Phase 6c (ReportBuilder)
**Blocks:** Phase 6h (test parity)
**Scope:** `python/cli_generator/query.py`, `templates/python/query.py.tera`, `python/cli_generator.pyi`

---

## Motivation

Phase 6b completed the main Python SDK v3 migration (search, count, search_all, report). Several gaps remain:

1. `from_v2_url()` class method — reconstruct a builder from a v2 GET URL
2. `probe_api_capability()` helper — detect v2 vs v3 at runtime
3. `search_df()` / `search_polars()` — return results as a DataFrame (convenience wrappers)
4. `ReportBuilder` integration (phase 6c is the primary deliverable; this phase wires it)
5. `to_tidy_records()` — verify v3 output compatibility
6. v2 fallback paths for `record`, `lookup`, `summary` (spec says all GET-capable methods should accept `api_version="v2"`)

---

## Work Items

### 1. `from_v2_url()` class method

**Files:** `python/cli_generator/query.py`, `templates/python/query.py.tera`, `crates/genomehubs-query/src/query/mod.rs`, `src/lib.rs`

Parses a v2 API GET URL back into a fully populated `QueryBuilder`. Useful for migrating bookmarks, v2 scripts, and UI-copied URLs to v3-compatible POST bodies.

**Rust core:** `query_yaml_from_url_params(url: &str) -> Result<(String, String), String>` in `crates/genomehubs-query/src/query/mod.rs`. Parses the URL query string into `SearchQuery` and `QueryParams`, then serialises both to YAML. This reverses the logic in `build_query_url`.

Fields to parse from the query string (covering the full v2 parameter set):

| URL param          | Maps to                                           |
| ------------------ | ------------------------------------------------- |
| `tax_name`         | `SearchQuery.taxa[].names`                        |
| `tax_lineage`      | `SearchQuery.taxa[].names` (filter_type=ancestor) |
| `tax_rank`         | `SearchQuery.rank`                                |
| `fields`           | `SearchQuery.fields`                              |
| `query`            | parsed into attribute filters                     |
| `size`             | `QueryParams.size`                                |
| `offset`           | `QueryParams.page` (converted: `offset / size`)   |
| `sortBy`           | `QueryParams.sort.field`                          |
| `sortOrder`        | `QueryParams.sort.order`                          |
| `includeEstimates` | `QueryParams.include_estimates`                   |
| `taxonomy`         | `QueryParams.taxonomy`                            |
| `result`           | `SearchQuery.index`                               |

**PyO3 exposure:**

```rust
#[pyfunction]
fn query_yaml_from_url_params(url: &str) -> PyResult<(String, String)> { ... }
```

**Python method:**

```python
@classmethod
def from_v2_url(cls, url: str) -> "QueryBuilder":
    """Reconstruct a QueryBuilder from a v2 API GET URL.

    Useful for migrating v2 bookmarks and UI-copied URLs to the v3 POST body pattern.

    Args:
        url: A full v2 API URL, e.g.
            ``https://goat.genomehubs.org/api/v2/search?tax_name=Primates&fields=genome_size``

    Returns:
        A populated QueryBuilder ready for v3 POST calls.
    """
    from . import query_yaml_from_url_params as _parse_url
    query_yaml, params_yaml = _parse_url(url)
    qb = cls.__new__(cls)
    qb._query_yaml = query_yaml
    qb._params_yaml = params_yaml
    return qb
```

Template version: same, using `_ext.query_yaml_from_url_params`.

---

### 2. `probe_api_capability()` module function

**Files:** `python/cli_generator/query.py`, `templates/python/query.py.tera`

Lightweight detection function for callers that need to target both v2 and v3 instances.

```python
def probe_api_capability(api_base: str) -> str:
    """Probe an API base URL and return its capability level.

    Calls ``{api_base}/v3/status``. If the response includes ``/search`` in
    the ``supported`` list, returns ``"v3"``. Falls back to ``"v2"`` on any
    error or missing endpoint.

    Returns:
        ``"v3"`` or ``"v2"``.
    """
    import json, urllib.request
    try:
        with urllib.request.urlopen(f"{api_base}/v3/status", timeout=5) as resp:
            data = json.loads(resp.read().decode())
        supported = data.get("supported", [])
        if "/search" in supported:
            return "v3"
    except Exception:
        pass
    return "v2"
```

This is a module-level function, not a `QueryBuilder` method. Callers can pass the result to `search(api_version=...)`, `count(api_version=...)`, etc.

The template version is identical (no site-specific changes needed).

---

### 3. v2 fallback paths for `record`, `lookup`, `summary`

**Files:** `python/cli_generator/query.py`, `templates/python/query.py.tera`

The phase-6b plan specifies consistent `api_version="v2"` opt-in across all GET-capable methods. Currently `record`, `lookup`, `summary` always target v3. Add `api_version` parameter with `"v3"` default and v2 GET fallback:

```python
def record(
    self,
    record_id: str,
    result: str | None = None,
    api_base: str = "https://goat.genomehubs.org/api",
    api_version: str = "v3",
) -> Any:
    if api_version == "v2":
        # build v2 GET URL: /api/v2/record?recordId=...&result=...
        ...
    # existing v3 GET implementation
    ...
```

Same for `lookup()` and `summary()`.

> These methods already target v3 GET (their endpoint path did not change between v2 and v3), so the "v2 fallback" is just using `/api/v2/` instead of `/api/v3/` in the URL. The implementation change is small.

---

### 4. `search_df()` and `search_polars()` correctness

**Files:** `python/cli_generator/query.py`, `templates/python/query.py.tera`

These convenience wrappers call `search()` and convert the result to a pandas/polars DataFrame. With the v3 migration, `search()` now returns a dict with a `results` key rather than raw TSV. Verify these wrappers handle the new response shape.

Current implementation (approximate):

```python
def search_df(self, ...) -> "pd.DataFrame":
    import pandas as pd
    raw = self.search(format="tsv", ...)
    return pd.read_csv(io.StringIO(raw), sep="\t")
```

With v3, TSV format requires the v2 GET path (the v3 POST endpoint returns JSON only). The `search()` method already handles this: when `format != "json"` it falls back to `to_v2_url()` + GET. Verify this path produces valid TSV that the DataFrame constructor can parse. Add a regression test.

---

### 5. `to_tidy_records()` v3 compatibility

**File:** `python/cli_generator/query.py`

`to_tidy_records()` calls `parse_search_json` on the raw search result. With v3, `search()` returns a dict (parsed JSON), not a raw string. The wrapper must serialise back to JSON before passing to `parse_search_json`:

```python
def to_tidy_records(self, records: list[dict] | str | None = None) -> list[dict]:
    import json
    if records is None:
        raw = self.search(format="json")
        data_str = json.dumps(raw) if isinstance(raw, dict) else raw
    elif isinstance(records, list):
        data_str = json.dumps({"results": records})
    else:
        data_str = records
    return json.loads(_ext.parse_search_json(data_str))
```

Verify and update if needed.

---

### 6. `.pyi` stub completeness

**File:** `python/cli_generator.pyi`

After phase 6c adds `validate_report_yaml`, after this phase adds `query_yaml_from_url_params`, ensure both are present in the stub. Run `pyright python/ tests/python/` to confirm zero errors.

Stub additions:

```python
def validate_report_yaml(report_yaml: str, field_meta_json: str) -> str: ...
def query_yaml_from_url_params(url: str) -> tuple[str, str]: ...
```

---

## Verification

```bash
maturin develop --features extension-module
pyright python/ tests/python/
pytest tests/python/ -v
```

Expected: 0 pyright errors, all existing tests pass, new tests pass.

---

## Tests to Add

| Test                                                             | File                                     |
| ---------------------------------------------------------------- | ---------------------------------------- |
| `test_from_v2_url_basic`                                         | `tests/python/test_core.py`              |
| `test_from_v2_url_with_filters`                                  | `tests/python/test_core.py`              |
| `test_from_v2_url_roundtrip` (URL → builder → URL is equivalent) | `tests/python/test_core.py`              |
| `test_probe_api_capability_returns_v2_on_error`                  | `tests/python/test_core.py`              |
| `test_probe_api_capability_integration` (skip without API)       | `tests/python/test_batch_integration.py` |
| `test_search_df_v3_tsv_path`                                     | `tests/python/test_core.py`              |
| `test_to_tidy_records_v3_result`                                 | `tests/python/test_core.py`              |
| `test_record_api_version_v2_fallback`                            | `tests/python/test_core.py`              |

---

## Ordering

1. Rust `query_yaml_from_url_params` in `query/mod.rs` + unit tests
2. PyO3 exposure + `.pyi` stub
3. `from_v2_url()` class method in `query.py` + `query.py.tera`
4. `probe_api_capability()` module function (pure Python, no Rust needed)
5. v2 fallback params on `record`, `lookup`, `summary`
6. `search_df` / `search_polars` correctness check and fix
7. `to_tidy_records` v3 compatibility
8. Tests + pyright check
