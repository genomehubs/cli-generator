# Phase XX: Test Coverage Improvement

**Status:** Design
**Depends on:** None (standalone quality work)
**Blocks:** Nothing downstream; but raising `fail_under` unblocks stricter CI enforcement
**Estimated scope:** ~200–300 lines of new tests across 4 files; no production code changes

---

## 1. Motivation

Current Python unit-test coverage is **53.5 %**, against a floor of **50 %**.
The gap is not caused by poor testing of core logic — the query builder, URL
encoding, fixtures, and parity tests all have good coverage. The gap comes from
three clusters of code that are structurally hard to reach with the existing
test harness:

| Cluster | Module                                                                          | Uncovered lines | Nature                                        |
| ------- | ------------------------------------------------------------------------------- | --------------- | --------------------------------------------- |
| A       | `query.py` – API call methods (`search`, `count`, `record`, …)                  | ~120            | Require HTTP; covered by CI integration tests |
| B       | `query.py` – Vega-Lite chart spec builders (`vl_histogram`, `vl_scatter`, etc.) | ~90             | Complex dict builders; no fixtures yet        |
| C       | `query.py` / `multi_query_builder.py` – hybrid positional, batch paths          | ~80             | Require FFI function or fixture JSON          |
| D       | `multi_query_builder.py` – named-query builder paths                            | ~80             | Builder methods; no dedicated tests           |

Cluster A (network) is excluded from the `fail_under` check via
`exclude_lines` patterns in `pyproject.toml` — those paths are adequately
covered by CI integration tests. Clusters B, C, and D are genuine gaps worth
closing with unit tests.

**Target:** raise `fail_under` from 50 → 65 by the end of this phase. That
number represents well-tested pure logic; it is deliberately not 80+ because
the network-call methods are integration-tested not unit-tested.

---

## 2. Scope

### 2.1 Cluster B — Vega-Lite spec builders (unit tests)

`query.py` lines 2711–2792 are the chart-spec helpers:
`_vl_histogram`, `_vl_scatter`, `_vl_map`, `_vl_tree`, `_vl_arc`.
These are pure `dict`-returning functions — no FFI, no HTTP.

**Test approach:** snapshot tests using `pytest` fixtures.

For each report type:

1. Construct a minimal `PlotSpec`-like dict (as returned by `parse_histogram_json`
   / `parse_report_json` etc.).
2. Call the builder function directly.
3. Assert structure: `$schema` present, `mark` is correct, top-level `encoding`
   has the expected keys, axis labels match.

Example:

```python
def test_vl_histogram_structure():
    spec = {
        "report": "histogram",
        "x": {"key": "genome_size", "step": 1e9, "min": 1e8, "max": 1e11},
        "data": {"buckets": [{"key": 1e8, "doc_count": 10}]},
    }
    vl = QueryBuilder._vl_histogram(spec)
    assert vl["$schema"].startswith("https://vega.github.io/schema")
    assert vl["mark"]["type"] == "bar"
    assert "x" in vl["encoding"]
    assert "y" in vl["encoding"]
```

Files: `tests/python/test_chart_specs.py` (new)

### 2.2 Cluster C — `parse_*` round-trips (unit tests)

The `parse_*` Rust functions are already tested via `tests/python/test_core.py`,
but the Python wrapper code that calls them (`query.py` lines 700–800) is not
fully exercised. The gaps are the `to_flat_records`, `to_tidy_records`, and
`annotated_values` paths.

**Test approach:** use the existing fixture JSON responses to drive the Python
wrappers.

```python
# tests/python/test_parse_paths.py
def test_to_flat_records_from_fixture(search_response_json):
    qb = QueryBuilder("taxon")
    records = qb.to_flat_records(search_response_json)
    assert isinstance(records, list)
    assert all(isinstance(r, dict) for r in records)
```

The `search_response_json` fixture is already defined in
`tests/python/conftest.py` (or can be added by reusing an existing
`tests/fixtures/*.json` file).

Files: `tests/python/test_parse_paths.py` (new)

### 2.3 Cluster D — `MultiQueryBuilder` named-query paths

`multi_query_builder.py` lines 292–351 and 392–409 cover:

- `add_named_query` / `remove_named_query`
- `to_msearch_body` with a named-query map
- `to_url_map`
- `chain_query` branching

**Test approach:** pure builder tests (no HTTP), similar to `test_sdk_fixtures.py`.

```python
from cli_generator import MultiQueryBuilder, QueryBuilder

def test_named_query_round_trip():
    mqb = MultiQueryBuilder()
    qb = QueryBuilder("taxon").set_taxa(["Mammalia"], filter_type="tree")
    mqb.add_named_query("mammals", qb)
    body = mqb.to_msearch_body()
    assert any(b.get("index") == "taxon" for b in body if isinstance(b, dict))

def test_url_map_keys():
    mqb = MultiQueryBuilder()
    mqb.add_named_query("q1", QueryBuilder("taxon"))
    mqb.add_named_query("q2", QueryBuilder("assembly"))
    url_map = mqb.to_url_map()
    assert set(url_map.keys()) == {"q1", "q2"}
```

Files: `tests/python/test_multi_query_builder.py` (new)

### 2.4 Cluster B (partial) — hybrid positional Python wrapper

`query.py` lines 1642–1730 contain the `positional()` method and its
`_build_positional_doc` helper. The doc-building logic is pure Python; it can
be tested without calling the Rust FFI positional renderer.

**Test approach:** test `_build_positional_doc` (or equivalent internal) directly
with mock assembly data.

Files: `tests/python/test_parse_paths.py` (extend)

---

## 3. Acceptance criteria

- [ ] `coverage report` shows ≥ 65 % total (with network methods excluded)
- [ ] `fail_under` raised from 50 → 65 in `pyproject.toml`
- [ ] All new tests pass in `pytest tests/python/ -v` with no `xfail`
- [ ] No new `pragma: no cover` annotations added (existing exclusions are already
      principled — network I/O, `__repr__`, TYPE_CHECKING)
- [ ] `bash scripts/verify_code.sh` exits 0

---

## 4. Out of scope

- R and JavaScript test coverage (tracked separately in `phase-6h-test-parity.md`)
- Rust coverage (tarpaulin runs in CI; no threshold enforced locally)
- Testing the Vega-Lite _visual output_ (pixel/render tests are not warranted)
- Mock-HTTP tests for `search()`, `count()`, etc. (integration tests cover these;
  adding mocks would duplicate coverage without adding confidence)

---

## 5. Implementation checklist

- [ ] Add `tests/python/test_chart_specs.py` — Vega-Lite spec builder unit tests
- [ ] Add `tests/python/test_parse_paths.py` — `to_flat_records`, `to_tidy_records`,
      `annotated_values`, `_build_positional_doc`
- [ ] Add `tests/python/test_multi_query_builder.py` — named-query builder paths
- [ ] Verify coverage ≥ 65 % with `coverage run -m pytest tests/python/ && coverage report`
- [ ] Raise `fail_under = 65` in `pyproject.toml`
- [ ] Update CI comment in `ci.yml` to reflect new threshold
