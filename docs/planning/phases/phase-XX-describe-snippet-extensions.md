# Phase XX: Describe and Snippet Extensions

**Status:** Design capture (not sequenced into ordered phases yet)
**Rationale:** Extend the describe/snippet system beyond search to cover count, report, and batch workflows
**Priority:** Post-6h; depends on `ReportBuilder` (phase 6c) and `report` CLI subcommand (phase 6d)
**Depends on:** Phase 6c (ReportBuilder), Phase 6d (CLI report subcommand)

---

## Overview

`describe()` and `snippet()` currently operate on `QuerySnapshot` — the serialised state of a `QueryBuilder`. They produce prose and runnable code for the `search` call path only. This phase extends them to cover:

1. **Report configurations** — `ReportBuilder.describe()` and `ReportBuilder.snippet()`
2. **Count calls** — a `call_type` context variable selects `search` vs `count` in snippet templates
3. **Batch workflows** — `MultiQueryBuilder.snippet()` for batch search/count
4. **Combined prose** — `qb.describe()` + `rb.describe()` → single combined sentence

---

## 1. Combined describe: query + report

### Current behaviour

`qb.describe()` → `"taxon genome_size ≥ 1 Gbp, ordered by genome_size, filtered to Primate species"`

### Extended behaviour

When called with an attached `ReportBuilder`:

```python
qb.describe(report=rb)
# → "taxon genome_size ≥ 1 Gbp filtered to Primates,
#    visualised as a histogram of genome_size by species rank"
```

### Implementation

**Rust:** Add `describe_report_yaml(report_yaml: &str) -> String` to `crates/genomehubs-query/src/describe.rs`.

The function parses the report YAML and produces a phrase like:

- `histogram`: `"a histogram of {x} by {rank} rank"` (or `"by category {cat}"` if cat is set)
- `scatter`: `"a scatter plot of {x} vs {y}"` (optionally `"coloured by {cat}"`)
- `map`: `"a geographic distribution map"` (optionally `"using {location_field}"` if non-default)
- `tree`: `"a taxonomic tree at {rank} rank"`
- `xPerRank`: `"values of {x} per rank"`
- `sources`: `"data source summary"`
- `arc`: `"an arc diagram of {x}"`

Combined phrase: `"{query description}, visualised as {report description}"`.

**PyO3:** expose `describe_report_yaml` in `src/lib.rs`.

**Python:** `QueryBuilder.describe(report=None)` gains an optional `ReportBuilder` param; if provided, appends the report description.

**R/JS:** same addition to their `describe()` methods.

---

## 2. Snippet extensions: call_type context variable

### Current snippet templates

All templates (`python_snippet.tera`, `r_snippet.tera`, `js_snippet.tera`, `cli_snippet.tera`) end with a `search` call. There is no mechanism to generate a `count` or `report` snippet from the same `QuerySnapshot`.

### Proposed extension

Add a `call_type` string to `QuerySnapshot` in `crates/genomehubs-query/src/types.rs`:

```rust
pub struct QuerySnapshot {
    // ... existing fields ...
    /// Which SDK call to show in the snippet. One of: "search", "count", "report".
    /// Defaults to "search".
    #[serde(default)]
    pub call_type: String,
}
```

The snippet generator passes `call_type` to the Tera template context. Each template handles all three:

**Python snippet (`call_type = "count"`):**

```python
count = qb.count()
print(f"Total records: {count}")
```

**Python snippet (`call_type = "report"`):**

```python
data = qb.report(rb)
import json
print(json.dumps(data, indent=2))
```

**CLI snippet (`call_type = "count"`):**

```
goat-cli taxon count \
  --taxon "Primates" --taxon-filter ancestor
```

### Report snapshot

For `call_type = "report"`, the `QuerySnapshot` gains an optional `report` field:

```rust
pub struct QuerySnapshot {
    // ...
    pub call_type: String,
    pub report: Option<ReportSnapshot>,
}

pub struct ReportSnapshot {
    pub report_type: String,
    pub x: Option<String>,
    pub y: Option<String>,
    pub cat: Option<String>,
    pub rank: Option<String>,
    // ... other report fields as Option<String>
}
```

The snippet template can then emit the `ReportBuilder` construction code before the `run()` call.

---

## 3. Batch snippets

Batch workflows (search_batch, count_batch) are not well represented by a single `QueryBuilder` state. A dedicated `MultiQueryBuilder` wrapper captures N builders and generates batch snippets.

### Python

```python
class MultiQueryBuilder:
    def __init__(self, queries: list[QueryBuilder]) -> None: ...
    def snippet(self, languages: list[str] = ["python"]) -> dict[str, str]: ...
    def describe(self) -> str: ...  # "N queries across taxon/assembly"
```

The snippet shows:

```python
queries = [qb1, qb2, qb3]
results = qb1.search_batch(queries)
```

### R / JS

Same pattern: `MultiQueryBuilder` R6 class / JS class with `snippet()`.

---

## 4. Files touched

| File                                      | Change                                                                  |
| ----------------------------------------- | ----------------------------------------------------------------------- |
| `crates/genomehubs-query/src/types.rs`    | Add `call_type` and `report: Option<ReportSnapshot>` to `QuerySnapshot` |
| `crates/genomehubs-query/src/describe.rs` | Add `describe_report_yaml()`                                            |
| `crates/genomehubs-query/src/snippet.rs`  | Pass `call_type` and `report` to Tera context                           |
| `templates/snippets/python_snippet.tera`  | Handle `call_type` branches                                             |
| `templates/snippets/r_snippet.tera`       | Handle `call_type` branches                                             |
| `templates/snippets/js_snippet.tera`      | Handle `call_type` branches                                             |
| `templates/snippets/cli_snippet.tera`     | Handle `call_type` branches                                             |
| `src/lib.rs`                              | Expose `describe_report_yaml` via PyO3                                  |
| `python/cli_generator/query.py`           | `describe(report=None)`, `MultiQueryBuilder.snippet()`                  |
| `templates/python/query.py.tera`          | Same                                                                    |
| `templates/r/query.R`                     | `describe(report=NULL)`, `MultiQueryBuilder`                            |
| `templates/js/query.js`                   | `describe({report=null})`, `MultiQueryBuilder`                          |

---

## 5. Tests

| Test                                                              | Location                          |
| ----------------------------------------------------------------- | --------------------------------- |
| `test_describe_report_yaml_histogram`                             | `tests/python/test_core.py`       |
| `test_describe_report_yaml_scatter`                               | `tests/python/test_core.py`       |
| `test_describe_combined_query_and_report`                         | `tests/python/test_core.py`       |
| `test_snippet_call_type_count`                                    | `tests/python/test_core.py`       |
| `test_snippet_call_type_report`                                   | `tests/python/test_core.py`       |
| SDK parity: describe + report produces same string in Python/R/JS | `tests/python/test_sdk_parity.py` |

---

## Ordering

1. `describe_report_yaml()` in Rust — enables combined prose immediately
2. Python `QueryBuilder.describe(report=None)` — uses step 1
3. R/JS `describe()` extension — mirrors step 2
4. `QuerySnapshot.call_type` + `ReportSnapshot` — data model change
5. Snippet template branches for `count` and `report` — needs step 4
6. `MultiQueryBuilder` for batch snippets — standalone addition
7. Tests
