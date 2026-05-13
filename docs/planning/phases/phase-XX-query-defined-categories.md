# Phase XX — Query-defined categories

## Status: Deferred

Prerequisite infrastructure (`filter_expr`, `NamedQuerySpec`, `resolve_chain_refs`) is
now in place as of Phase 15. Implementation is deferred because it requires new
report-config structure, multi-query ES orchestration, and category-assignment logic
that are orthogonal to the chain-query work already completed.

---

## Feature description

Allow histogram and scatter report categories to be defined by a query expression
rather than by a field value. Instead of binning documents by the distinct values of
a categorical field (e.g. `assembly_level`), each category is defined by an arbitrary
filter expression. Documents matching multiple category queries fall into the first
matching category (ordered list semantics).

**Example use-case:** Compare two genome-size thresholds across taxa:

```yaml
report: histogram
x: genome_size
categories:
  - label: ">3 Gb"
    query: "genome_size>3000000000"
  - label: "1–3 Gb"
    query: "genome_size>=1000000000 AND genome_size<=3000000000"
  - label: "<1 Gb"
    query: "genome_size<1000000000"
```

---

## Config structure (proposed)

Add a top-level `categories` key to the report YAML accepted by the `/v3/report`
endpoint. The existing `cat` / `cat_rank` keys continue to work for field-value
categories and are unaffected.

```yaml
# New: query-defined categories
categories:
  - label: ">3 Gb"
    query: "genome_size>3000000000"
  - label: "1–3 Gb"
    query: "genome_size>=1000000000"
  - label: "<1 Gb" # implicit: all remaining hits
    query: ""
```

Each entry:

| Key     | Type   | Required | Description                                                                 |
| ------- | ------ | -------- | --------------------------------------------------------------------------- |
| `label` | string | yes      | Display label for the category                                              |
| `query` | string | yes      | Filter expression (same syntax as `filter_expr`). Empty string matches all. |
| `color` | string | no       | Optional hex color override                                                 |

---

## Implementation steps

### 1. Rust: parse `categories` in report config

**File:** `crates/genomehubs-query/src/report/report_types.rs`

- Add `categories: Option<Vec<CategorySpec>>` to `ReportConfig`
- `CategorySpec` struct: `label: String`, `query: String`, `color: Option<String>`
- `CategorySpec::to_es_filter()` — reuse `filter_expr_to_es_query()` from `filter_expr.rs`

### 2. Rust: multi-query ES orchestration

**File:** `crates/genomehubs-api/src/report/histogram.rs` (and `scatter.rs`)

When `categories` is non-empty, fan out one ES query per category using `_msearch`,
then merge the per-category hit counts into a single histogram bucket series.

Pattern: same `_msearch` batching used by `run_rings_report()` in `arc.rs`.

### 3. Rust: category-assignment logic

Each ES query returns the count of documents matching that category _within the scope
of the parent query_. No per-document assignment is needed for counts — each
category result is an independent count.

For scatter / raw results, use a `bool.should` filter with `minimum_should_match: 1`
and tag each hit with the first matching category label via a
[`field_collapsing`](https://www.elastic.co/guide/en/elasticsearch/reference/current/collapse-search-results.html)
or runtime-field approach.

### 4. Python SDK: `ReportBuilder.add_category()`

```python
def add_category(self, label: str, query: str, *, color: str | None = None) -> "ReportBuilder":
    """Append a query-defined category."""
    if "categories" not in self._doc:
        self._doc["categories"] = []
    entry: dict[str, str] = {"label": label, "query": query}
    if color is not None:
        entry["color"] = color
    self._doc["categories"].append(entry)
    return self
```

Mirror to Tera template, JS template (`addCategory`), and R template (`add_category`).

Add to `CANONICAL_REPORT_BUILDER_METHODS` in `test_sdk_parity.py`.

### 5. Tests

- Unit test: `ReportBuilder.add_category()` YAML contains `categories` key
- Unit test: multiple categories all appear in YAML
- Fixture test: builder-only fixture (no live API response needed initially)
- Parity test: `add_category` in canonical method list for all three languages

---

## Scope boundary

This feature affects **report rendering only** — it does not change the search
query path, URL encoding, or the `NamedQuerySpec` / chain-query machinery.
Validation (`ReportBuilder.validate()`) should warn if both `cat` and `categories`
are set simultaneously (ambiguous config).

---

## Why deferred

1. The histogram/scatter renderers need separate refactoring to support multi-query
   result merging — currently they issue a single ES query.
2. The scatter case (per-document category tags) requires a more complex ES strategy
   that deserves its own design pass.
3. No immediate user request — the filter_expr infrastructure makes this feasible
   but it is not blocking any current workflow.
