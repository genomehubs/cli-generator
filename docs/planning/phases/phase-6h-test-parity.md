# Phase 6h: Test Parity

**Depends on:** Phase 6c (ReportBuilder), Phase 6d (CLI), Phase 6e (Python gaps), Phase 6f (R/JS gaps), Phase 6g (Quarto docs)
**Blocks:** nothing downstream (this is the gate check for the whole 6b–6h arc)
**Scope:** `tests/python/test_sdk_parity.py`, `tests/python/test_sdk_fixtures.py`, `tests/r/test_sdk_fixtures.R`, `tests/javascript/test_sdk_fixtures.mjs`

---

## Motivation

The existing parity test suite (`test_sdk_parity.py`) verifies that:

1. Every canonical method exists in Python, R, and JS templates
2. Python has no extra methods beyond the canonical set (plus documented extras)
3. Every method documented in the Quarto reference exists in the canonical set
4. Fixture tests produce the same URL-state for the same query across all three languages

After phases 6c–6g, the canonical set has grown:

- `report` is now canonical (Python has it; R and JS will have it after 6f)
- `to_v2_url` is a renamed canonical (replaces `to_url`)
- `search_all` is canonical for Python and JS; R gains it in 6f
- `from_v2_url` is Python-only (justified: class method, R/JS pattern differs)
- `probe_api_capability` is Python-only module function (not a method; exempt)
- `ReportBuilder` is a new parallel class — needs its own canonical registry

The goal of this phase is to bring the test suite fully into sync with the implemented state, fix any justified divergences, and document the rationale for any Python-only methods.

---

## Work Items

### 1. Update `CANONICAL_METHODS` registry

**File:** `tests/python/test_sdk_parity.py`

Changes to the registry:

| Key          | Change                                                              |
| ------------ | ------------------------------------------------------------------- |
| `to_url`     | Mark as deprecated; keep entry (it still exists as an alias)        |
| `to_v2_url`  | Add new entry (`js_name: "toV2Url"`, `r_name: "to_v2_url"`)         |
| `search_all` | Verify present; add `r_name: "search_all"` (after phase 6f adds it) |
| `report`     | Add new entry (`js_name: "report"`, `r_name: "report"`)             |
| `count`      | Verify present (was already canonical)                              |
| `search`     | Verify present                                                      |

Full new entry for `to_v2_url`:

```python
"to_v2_url": {
    "params": ["endpoint"],
    "python_name": "to_v2_url",
    "js_name": "toV2Url",
    "r_name": "to_v2_url",
},
```

Full new entry for `report`:

```python
"report": {
    "params": ["report"],
    "python_name": "report",
    "js_name": "report",
    "r_name": "report",
},
```

---

### 2. Update Python-only extras allowlist

**File:** `tests/python/test_sdk_parity.py`

The `test_no_extra_methods_in_python` test has an allowlist. Current state (after phase 6b):

```python
["report", "to_v2_url", "_post_json", "search_all", ...]
```

After phases 6e–6f, `report` and `to_v2_url` move from extras to canonical (they exist in all three). Remove them from the allowlist when they are confirmed in R and JS.

`_post_json` remains Python-only (internal transport helper — R uses `httr::POST`, JS uses `fetch`). Justified divergence.

`from_v2_url` is Python-only:

- R: the pattern would be a standalone function `query_builder_from_url()`, not a class method
- JS: `QueryBuilder.fromV2Url(url)` is a valid addition, but deferred to phase-XX (from_v2_url)

The allowlist after this phase:

```python
canonical_python_names.update([
    "__init__",
    "field_modifiers",
    "to_tidy_records",
    "field_names",
    "field_info",
    "combine",
    "search_df",
    "search_polars",
    "search_all",        # until R/JS confirm it; then move to canonical
    "_post_json",        # Python-only: internal transport helper
    "from_v2_url",       # Python-only (phase 6e): R/JS deferred
    "probe_api_capability",  # module function, not a method (exempt)
])
```

---

### 3. `ReportBuilder` canonical registry

Add a separate `CANONICAL_REPORT_BUILDER_METHODS` dict:

```python
CANONICAL_REPORT_BUILDER_METHODS = {
    "set_x":                  {"python_name": "set_x",               "js_name": "setX",              "r_name": "set_x"},
    "set_y":                  {"python_name": "set_y",               "js_name": "setY",              "r_name": "set_y"},
    "set_cat":                {"python_name": "set_cat",             "js_name": "setCat",            "r_name": "set_cat"},
    "set_rank":               {"python_name": "set_rank",            "js_name": "setRank",           "r_name": "set_rank"},
    "set_fields":             {"python_name": "set_fields",          "js_name": "setFields",         "r_name": "set_fields"},
    "set_status_filter":      {"python_name": "set_status_filter",   "js_name": "setStatusFilter",   "r_name": "set_status_filter"},
    "set_cat_rank":           {"python_name": "set_cat_rank",        "js_name": "setCatRank",        "r_name": "set_cat_rank"},
    "set_collapse_monotypic": {"python_name": "set_collapse_monotypic", "js_name": "setCollapseMonotypic", "r_name": "set_collapse_monotypic"},
    "set_preserve_rank":      {"python_name": "set_preserve_rank",   "js_name": "setPreserveRank",   "r_name": "set_preserve_rank"},
    "set_count_rank":         {"python_name": "set_count_rank",      "js_name": "setCountRank",      "r_name": "set_count_rank"},
    "set_location_field":     {"python_name": "set_location_field",  "js_name": "setLocationField",  "r_name": "set_location_field"},
    "set_hex_resolution":     {"python_name": "set_hex_resolution",  "js_name": "setHexResolution",  "r_name": "set_hex_resolution"},
    "set_map_threshold":      {"python_name": "set_map_threshold",   "js_name": "setMapThreshold",   "r_name": "set_map_threshold"},
    "set_scatter_threshold":  {"python_name": "set_scatter_threshold", "js_name": "setScatterThreshold", "r_name": "set_scatter_threshold"},
    "to_report_yaml":         {"python_name": "to_report_yaml",      "js_name": "toReportYaml",      "r_name": "to_report_yaml"},
    "validate":               {"python_name": "validate",            "js_name": "validate",          "r_name": "validate"},
    "run":                    {"python_name": "run",                  "js_name": "run",               "r_name": "run"},
}
```

Add corresponding `TestReportBuilderParity` class:

```python
class TestReportBuilderParity:
    """ReportBuilder methods must be present in all three SDK languages."""

    def test_python_report_builder_methods_present(self):
        python_methods = get_python_report_builder_methods()
        for concept, spec in CANONICAL_REPORT_BUILDER_METHODS.items():
            assert spec["python_name"] in python_methods, \
                f"ReportBuilder missing Python method: {spec['python_name']}"

    def test_javascript_report_builder_methods_present(self): ...

    def test_r_report_builder_methods_present(self): ...
```

Helpers `get_python_report_builder_methods()`, `get_js_report_builder_methods()`, `get_r_report_builder_methods()` parse the respective template files for the `ReportBuilder` class.

---

### 4. Add fixture tests for new methods

#### `tests/python/test_sdk_fixtures.py`

Add entries for:

```python
FIXTURE_TO_BUILDER = {
    # ... existing entries ...
    "report_histogram_primates": (
        lambda: QueryBuilder("taxon")
            .set_taxa(["Primates"], filter_type="ancestor")
            .set_rank("species"),
        "report",
        {"report_type": "histogram", "x": "genome_size", "rank": "species"},
    ),
}

FIXTURE_EXPECTED_URL_PARTS = {
    # ... existing entries ...
    "report_histogram_primates": [
        "result=taxon",
        "tax_lineage",
    ],  # v2 URL parts from to_v2_url() (the report itself has no v2 URL)
}
```

> Note: the `report` fixture is special — it validates `to_report_yaml()` output rather than a URL. A separate assertion structure may be needed:
>
> ```python
> assert "histogram" in qb.report_builder.to_report_yaml()  # if ReportBuilder is stored
> ```
>
> Or simply: validate that `validate_report_yaml()` returns no errors for the fixture.

#### `tests/r/test_sdk_fixtures.R` and `tests/javascript/test_sdk_fixtures.mjs`

Mirror the `report_histogram_primates` fixture entry. Per AGENTS.md convention, matching `FIXTURE_TO_BUILDER` and `FIXTURE_EXPECTED_URL_PARTS` entries are required in all three files simultaneously.

---

### 5. v3 transport parity assertions

The fixture tests currently assert URL shape via `FIXTURE_EXPECTED_URL_PARTS`. For v3 POST methods, the observable contract is the YAML body content, not a URL. Add a parallel assertion mechanism:

```python
# In test_sdk_parity.py
FIXTURE_EXPECTED_YAML_PARTS = {
    "report_histogram_primates": {
        "query_yaml": ["taxa:", "names:", "Primates"],
        "report_yaml": ["report: histogram", "x: genome_size", "rank: species"],
    },
}
```

Tests assert that `to_query_yaml()` and `to_report_yaml()` contain these substrings, giving observable proof of v3 transport correctness without requiring a live API.

---

### 6. Deprecation test for `to_url()`

```python
def test_to_url_emits_deprecation_warning():
    """to_url() must emit DeprecationWarning."""
    from cli_generator.query import QueryBuilder
    import warnings
    qb = QueryBuilder("taxon")
    with warnings.catch_warnings(record=True) as w:
        warnings.simplefilter("always")
        qb.to_url()
    assert any(issubclass(warning.category, DeprecationWarning) for warning in w)
```

---

### 7. `search_all` R/JS parity (added after phase 6f)

Once R and JS have `search_all` / `searchAll`:

```python
def test_r_has_search_all():
    r_methods = get_r_methods()
    assert "search_all" in r_methods

def test_js_has_searchAll():
    js_methods = get_js_methods()
    assert "searchAll" in js_methods
```

And move `"search_all"` from the Python-only extras allowlist to `CANONICAL_METHODS`.

---

### 8. Quarto documentation parity (extended)

The existing `TestDocumentationParity` tests check that canonical method names appear in the Quarto reference. Extend to:

- Check `ReportBuilder` section exists in the Quarto doc
- Check `to_v2_url` appears in the Quarto doc (not just `to_url`)
- Check the deprecation marker appears near `to_url` mentions
- Check API tab section exists for `count`, `search`, `report`

---

## Justified Python-only divergences

The following methods are Python-only and their absence from R/JS is intentional:

| Method            | Justification                                                                                                                                                                                                                                                                                                                                 |
| ----------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `_post_json`      | Internal transport helper. R uses `httr::POST`; JS uses `fetch`. Not part of the public API.                                                                                                                                                                                                                                                  |
| `from_v2_url`     | Implemented as class method in Python. R/JS equivalent would be a standalone function. Deferred to phase-XX.                                                                                                                                                                                                                                  |
| `search_df`       | pandas-specific. No pandas in R or JS.                                                                                                                                                                                                                                                                                                        |
| `search_polars`   | polars-specific.                                                                                                                                                                                                                                                                                                                              |
| `to_tidy_records` | Identical functionality exists in R (`parse_search_json` returns R list); JS has `toTidyRecords` module function. The name diverges because R/JS have it as a standalone rather than a method. If the JS standalone `toTidyRecords` is considered equivalent, update the parity test to check the module export rather than the class method. |

These justifications must be documented in a comment block in `test_sdk_parity.py` so future contributors understand why the allowlist entries exist.

---

## Ordering

1. Update `CANONICAL_METHODS` with `to_v2_url`, `report`, `search_all` (after 6f confirms R/JS)
2. Update Python-only extras allowlist with documented justifications
3. Add `CANONICAL_REPORT_BUILDER_METHODS` and `TestReportBuilderParity` class
4. Add `FIXTURE_EXPECTED_YAML_PARTS` structure and assertions
5. Add fixture entries for `report_histogram_primates` in all three files
6. Add deprecation test for `to_url()`
7. Add `search_all` R/JS parity tests (after 6f)
8. Extend Quarto parity tests to cover new sections
9. Run full suite: `pytest tests/python/ -v` — all pass
