# Phase 15: Cross-Query Reports

**Depends on:** Phase 5 (es_client, AggBuilder), Phase 7 (arc + shared filter-expression parser)
**Precedes:** Phase 6b (SDK / CLI integration) — cross-query support must be available before SDK method signatures are finalised
**Blocks:** nothing downstream beyond Phase 6b
**Estimated scope:** 1 new Rust module, extensions to Phase 7 arc, extensions to Phase 5 query builder

---

## Audit: `query[A-J]` in v2

### What the v2 mechanism actually is

The conversation context described `query[A-J]` as cross-index query support. The audit
of the v2 source code reveals the precise mechanism: a function called `chainQueries()`
in `local-api-copy/src/api/v2/functions/getResults.js`.

#### `chainQueries()` — sequential query expansion

```js
export const chainQueries = async ({
  query,
  result,
  chainThreshold = 500,
  ...params
}) => {
  let matches = query.matchAll(/(query[A-Z]+).(\w+(:?\(\w+\))*)/g);
  for (let match of matches) {
    let parentQuery = params[match[1]]; // e.g. params.queryA
    let [parentResult, str] = parentQuery.split("--");
    if (!str) {
      str = parentResult;
      parentResult = result;
    }
    let [summary, fields] = match[2].split(/[\(\)]/);
    if (!fields) {
      fields = summary;
      summary = "value";
    }
    let res = await getResults({
      ...params,
      query: str,
      size: chainThreshold,
      fields,
      result: parentResult,
    });
    // Collect field values from sub-query results
    let values = res.results.flatMap(
      (obj) => obj.result.fields[fields][summary],
    );
    // Substitute into main query string
    query = query.replace(match[0], values.join(","));
  }
  return query;
};
```

**Key facts from the audit:**

1. **`chainThreshold = 500`** is the hard limit. Sub-queries with > 500 hits throw an
   error. This is a known production limitation — it constrains which queries can use
   the feature.

2. **Cross-index syntax** uses a `--` separator: `queryA=assembly--assembly_span>1e6`
   means "run `assembly_span>1e6` against the assembly index". Without `--`, the
   sub-query uses the same result type as the main query.

3. **Dot-notation references** in the main query: `taxon_id=queryA.taxon_id` — the
   regex `/(query[A-Z]+).(\w+(:?\(\w+\))*)/g` finds these and replaces them with
   comma-separated values extracted from the sub-query.

4. **Summary syntax**: `queryA.mean(genome_size)` extracts the `mean` summary of
   `genome_size` from each result in the sub-query.

5. **Integration with reports**: In `histogram.js` (line 962) and `arc()` in
   `report.js`, the named query params (`queryA`, `queryB`, …) are collected from
   `apiParams` and spread into every `getResults()` / `getResultCount()` call. Since
   `getResults()` always calls `chainQueries()` first, the substitution is transparent.

6. **`arc()` uses `query[A-Z]$`** (single uppercase letter at end); histogram uses
   `query[A-Z]+` (one or more uppercase letters). The patterns are inconsistent in v2.

---

## Genuine Gaps in v3

### Gap 1: `chainQueries` — not implemented

v3 has no equivalent. `SearchQuery` in `crates/genomehubs-query/src/query/mod.rs` has
no `named_queries` map, and the ES query builder has no pre-processing step to expand
`queryA.field` references before building the ES query body.

This is a genuine gap. Any v2 feature that uses `queryA.field` syntax will not work in
v3.

### Gap 2: Multi-ring arc (extended rainbow)

Phase 7 plans a single `x`/`y`/`z` triple per `arc` report. The v2 `arcPerRank` loops
over multiple taxonomic ranks to produce concentric rings, but each ring uses the same
`x`/`y`/`z` queries.

The user's requested extension — N concentric rings with different `x` queries per ring
(e.g., ring 1 = "has C-value", ring 2 = "has genome assembly", ring 3 = "has RNA-seq")
— is not covered by either v2 or Phase 7.

### Gap 3: Query-defined categories

The `cat` parameter in histogram/scatter reports accepts a field name (attribute or
rank). There is no way to define categories by query membership (e.g., "cat A =
assembly_span > 1 Gb, cat B = 100 Mb–1 Gb, cat C = < 100 Mb").

v2 partially covers this via the `queryA.field` substitution, but it is awkward: the
user must pre-compute category membership and embed it as values in the main query. A
first-class `query_categories` concept does not exist in v2 either.

---

## Is Each Feature Practical to Implement?

### `chainQueries` — YES, practical

The algorithm is sequential and deterministic. In Rust:

- Parse the query string for `queryXXX.field(summary)` patterns using a regex.
- For each match, look up the named query string in a `HashMap<String, String>`.
- Execute the sub-query (possibly parallel for multiple names using `_msearch`).
- Substitute comma-separated values into the query string.
- Pass the enriched query string to the standard ES query builder.

The 500-result limit is a practical engineering choice, not a fundamental constraint.
v3 can raise this to 10,000 (the ES `terms` clause limit) and emit a warning when
exceeded rather than hard-failing.

**ES `terms` lookup as workaround for > 10,000 results**: ES supports a
[`terms` lookup](https://www.elastic.co/guide/en/elasticsearch/reference/current/query-dsl-terms-query.html#query-dsl-terms-lookup)
that stores the ID list in ES itself (using an intermediate document) and references
it by ID. This removes the in-memory size limit but adds a write step. It is
implementable but adds complexity. For Phase 15, a configurable threshold with a clear
error message is the right trade-off; `terms` lookup can be added later.

### Cross-index query — YES, practical

The v2 `--` separator is handled only in the backwards-compat parser
(`NamedQuerySpec::from_legacy_string`). In the v3 struct, `index` is a proper
`Option<SearchIndex>` field. Cross-index behaviour (whether the parent's taxon
scope is inherited) is controlled by the explicit `inherit_scope` boolean.

### Multi-ring arc — YES, practical

Extend Phase 7's `ArcSpec` with an optional `rings: Vec<RingSpec>` field. When present,
each ring defines its own `x`/`y` pair (and optional `z`). The server executes all
count queries, optionally using `_msearch` for efficiency.

### Query-defined categories — YES but lower priority

This is a larger surface area change. It touches histogram, scatter, and arc report
types. Deferred to Phase 16 or later.

---

## Implementation Plan

### 15.1 — `chainQueries` pre-processor

#### Design overview

The implementation is split across two crates to respect the WASM-compatibility
constraint of `genomehubs-query`:

| Layer              | Crate              | Responsibility                                                                                  |
| ------------------ | ------------------ | ----------------------------------------------------------------------------------------------- |
| Types & pure logic | `genomehubs-query` | `NamedQuerySpec`, `ChainRef`, `parse_filter_string`, `collect_chain_refs`, `resolve_chain_refs` |
| HTTP execution     | `genomehubs-api`   | Run sub-queries via ES client, pass results to `resolve_chain_refs`                             |

The library crate does no I/O. The API crate passes pre-fetched values in as a
`HashMap<String, Vec<String>>`.

---

#### `NamedQuerySpec` — typed struct replacing the v2 string

**New file**: `crates/genomehubs-query/src/query/chain.rs`

````rust
/// A named sub-query used for chain substitution.
///
/// Defined under `named_queries` in a [`SearchQuery`] YAML block. The
/// sub-query executes before the main query; its results supply values that
/// are substituted into main-query attribute values via dot-notation
/// references like `value: queryA.taxon_id`.
///
/// # YAML example
/// ```yaml
/// named_queries:
///   queryA:
///     index: assembly
///     filters:
///       - name: assembly_span
///         operator: gt
///         value: "1000000000"
///       - name: assembly_level
///         operator: eq
///         value: chromosome
///     limit: 200
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamedQuerySpec {
    /// Target index.  `None` → inherit the parent query's `index` value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<SearchIndex>,

    /// Attribute filter conditions, using the same [`Attribute`] type as
    /// [`SearchQuery`].  All conditions are AND-combined.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<Attribute>,

    /// Whether to scope the sub-query inside the parent query's taxon tree.
    ///
    /// Default (when `None`): `true` if `index` matches the parent's index,
    /// `false` for cross-index queries.  Set explicitly to override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inherit_scope: Option<bool>,

    /// Maximum number of results to fetch.
    ///
    /// Default: 500.  The server enforces a hard ceiling of 10,000.
    /// Requests above 10,000 return a [`ChainError::TooManyHits`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}
````

The key difference from the v2 string format: `index` is a proper `SearchIndex`
enum value, and `filters` is `Vec<Attribute>` — the same typed representation used
throughout `SearchQuery`. No raw query strings inside `NamedQuerySpec`.

---

#### v2 string format — backwards compat parser

For URL parameters arriving as `queryA=assembly--assembly_span>1e9`:

```rust
impl NamedQuerySpec {
    /// Construct from the v2 URL-parameter string format.
    ///
    /// Format: `[index--]filter_expr[ AND filter_expr...]`
    ///
    /// | Input string                            | Result                               |
    /// |-----------------------------------------|--------------------------------------|
    /// | `"assembly--assembly_span>1e9"`         | `index: Assembly, filters: [span>1G]`|
    /// | `"genome_size>0 AND gc_percent<60"`     | `index: None, filters: [two attrs]`  |
    /// | `"taxon--tax_tree(Eukaryota)"`          | `index: Taxon, filters: []` + taxa   |
    ///
    /// # Errors
    /// Returns `None` if the index prefix is unrecognised.
    pub fn from_legacy_string(s: &str) -> Option<Self> {
        let (index_str, filter_str) = match s.split_once("--") {
            Some((idx, rest)) => (Some(idx.trim()), rest.trim()),
            None              => (None, s.trim()),
        };
        let index = match index_str {
            None             => None,
            Some("assembly") => Some(SearchIndex::Assembly),
            Some("sample")   => Some(SearchIndex::Sample),
            Some("taxon")    => Some(SearchIndex::Taxon),
            Some(_)          => return None,    // unknown index → reject
        };
        Some(Self {
            index,
            filters: parse_filter_string(filter_str),
            inherit_scope: None,
            limit: None,
        })
    }
}
```

---

#### `parse_filter_string` — attribute condition parser

Converts a v2-style filter string to `Vec<Attribute>`. This is the only place
where freeform query strings enter the typed representation.

```rust
/// Parse a whitespace-separated, `AND`-joined filter expression into typed
/// [`Attribute`] values.
///
/// Supported patterns per term:
/// - `field=value`, `field!=value`
/// - `field>value`, `field>=value`, `field<value`, `field<=value`
/// - `summary(field)>=value`  (e.g. `mean(genome_size)>1e6`)
/// - bare `field`             (existence test; operator = `Exists`)
///
/// Values are normalised the same way as the YAML deserializer
/// (`normalize_value`), so `"1e9"` and `"1G"` both expand to `"1000000000"`.
///
/// Unrecognised or malformed terms are silently skipped.
fn parse_filter_string(s: &str) -> Vec<Attribute> {
    // Splits on \s+AND\s+ (case-insensitive), then applies regex per term.
    // Regex:  ^(?:(\w+)\()?(\w+)\)?\s*([!=<>]+)\s*(.+)$
    //   group 1: optional summary (e.g. "mean")
    //   group 2: field name (e.g. "genome_size")
    //   group 3: operator string (e.g. ">=")
    //   group 4: value (e.g. "1e9")
    ...
}
```

This function is internal to `chain.rs`; it is not pub. The only public entry
point for legacy strings is `NamedQuerySpec::from_legacy_string`.

---

#### Chain references in attribute values

The v2 dot-notation (`taxon_id=queryA.taxon_id`, `genome_size=queryA.mean(genome_size)`)
is preserved as-is in the YAML format. Chain references are stored as plain
`AttributeValue::Single(String)` values — no new enum variant is needed.

The reference syntax is detected at resolution time:

```rust
/// A parsed chain reference extracted from an [`AttributeValue`].
///
/// Matches strings of the form `key.field` or `key.summary(field)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainRef {
    /// Named query key, e.g. `"queryA"`.
    pub key: String,
    /// Field to extract from results, e.g. `"taxon_id"`.
    pub field: String,
    /// Aggregation to apply.  `"value"` = raw field values (default).
    pub summary: String,
}

impl ChainRef {
    /// Try to parse a string as a chain reference.
    ///
    /// Returns `None` if the string does not match the pattern.
    pub fn parse(s: &str) -> Option<Self> {
        // Regex: ^([a-z][a-zA-Z0-9]*)\.([a-z_]+)(?:\(([a-z_]+)\))?$
        // group 1: key, group 2: summary-or-field, group 3: field (optional)
        ...
    }
}
```

The detection regex requires the key to start with a lowercase letter, so plain
field names like `genome_size` can never be misidentified as chain references.

---

#### Pure resolution functions (WASM-compatible)

Both functions are synchronous and have no I/O dependencies:

```rust
/// Walk all [`Attribute`] values in a [`SearchQuery`] and collect every
/// chain reference found.
///
/// Returns one entry per reference occurrence, not per unique key — the
/// caller deduplicates by key to know which sub-queries to execute.
pub fn collect_chain_refs(query: &SearchQuery) -> Vec<ChainRef> { ... }

/// Substitute resolved values into a [`SearchQuery`] in place.
///
/// For each [`Attribute`] whose `value` is a chain reference, replaces it
/// with an [`AttributeValue::List`] of the pre-fetched strings and sets the
/// operator to [`AttributeOperator::Eq`] if currently `None`.
///
/// # Errors
/// Returns [`ChainError::UndefinedQuery`] if a referenced key is not present
/// in `resolved`, or [`ChainError::TooManyHits`] if the resolved value list
/// exceeds the key's configured `limit`.
pub fn resolve_chain_refs(
    query: &mut SearchQuery,
    resolved: &HashMap<String, Vec<String>>,
) -> Result<(), ChainError> { ... }
```

---

#### Error type

```rust
/// Errors produced during chain-query processing.
#[derive(Debug)]
pub enum ChainError {
    /// A reference like `queryA.field` was found but `queryA` was not
    /// defined in `named_queries`.
    UndefinedQuery { key: String },

    /// A sub-query returned more results than the configured limit.
    TooManyHits { key: String, count: usize, limit: usize },

    /// The sub-query itself failed.
    SubQueryFailed { key: String, message: String },
}
```

---

#### `SearchQuery` — new `named_queries` field

The `deny_unknown_fields` attribute on `SearchQuery` means the new field must be
added to the struct and its serde annotation kept in sync with the `AttributeSet`
and `Identifiers` flatten approach.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchQuery {
    // ... existing fields unchanged ...

    /// Named sub-queries for chain substitution.
    ///
    /// Values in `attributes` may reference these using dot notation:
    /// `value: queryA.source_field` or `value: queryA.mean(source_field)`.
    ///
    /// Keys must match `[a-zA-Z][a-zA-Z0-9]*`; conventional names are
    /// `queryA`, `queryB`, … to match v2 URL parameter names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub named_queries: Option<IndexMap<String, NamedQuerySpec>>,
    //            ^^^ IndexMap preserves insertion order in serialised YAML
}
```

`IndexMap` (from the `indexmap` crate, already a common transitive dependency)
is used instead of `HashMap` to produce deterministic YAML output — important for
test fixtures and documentation examples.

---

#### API-layer execution (`genomehubs-api`)

**New file**: `crates/genomehubs-api/src/routes/chain_executor.rs`

```rust
/// Execute all named sub-queries for a [`SearchQuery`] and return resolved
/// field values, ready for [`resolve_chain_refs`].
///
/// Sub-queries that share the same index are batched into a single
/// `_msearch` request.
pub async fn execute_named_queries(
    query: &SearchQuery,
    state: &AppState,
) -> Result<HashMap<String, Vec<String>>, ChainError> { ... }
```

Called from the route handlers (`search.rs`, `report.rs`) before building the
ES query body:

```rust
// In route handler, after deserializing SearchQuery:
let resolved = chain_executor::execute_named_queries(&query, &state).await
    .map_err(|e| AppError::chain(e))?;
resolve_chain_refs(&mut query, &resolved)
    .map_err(|e| AppError::chain(e))?;
// Now query.attributes has no chain refs; proceed to build ES body.
```

---

#### URL parameter backwards compatibility

In the route handler, before calling `execute_named_queries`, any URL parameters
named `queryA`, `queryB`, …, `queryZ` (or multi-letter variants) are detected and
converted:

```rust
// In parse_url_params() or route handler prologue:
for (key, value) in url_params {
    if key.starts_with("query") && key[5..].chars().all(|c| c.is_ascii_uppercase()) {
        let spec = NamedQuerySpec::from_legacy_string(&value)
            .ok_or_else(|| AppError::bad_request(format!("invalid {key}: {value}")))?;
        query.named_queries
            .get_or_insert_with(IndexMap::new)
            .insert(key.clone(), spec);
    }
}
```

---

#### YAML examples

**Clean v3 format** (preferred):

```yaml
index: taxon
taxa: [Eukaryota]
taxon_filter_type: tree
attributes:
  - name: taxon_id
    operator: eq
    value: queryA.taxon_id # chain reference
named_queries:
  queryA:
    index: assembly
    filters:
      - name: assembly_span
        operator: gt
        value: "1000000000"
      - name: assembly_level
        operator: eq
        value: chromosome
    limit: 200
```

**Multi-reference, cross-index with summary**:

```yaml
index: taxon
taxa: [Mammalia]
taxon_filter_type: tree
attributes:
  - name: genome_size
    operator: gt
    value: queryA.mean(genome_size) # summary reference
named_queries:
  queryA:
    index: taxon # same index, different filter
    filters:
      - name: order
        operator: eq
        value: Rodentia
    inherit_scope: false # don't inherit parent's Mammalia scope
```

**Equivalent v2 URL form (accepted, converted to above)**:

```
?taxa=Mammalia&taxon_filter_type=tree&attributes=genome_size&
 queryA=taxon--tax_tree(Rodentia)&query=genome_size=queryA.mean(genome_size)
```

---

#### Algorithm summary

```
1. Deserialise SearchQuery (YAML or URL params).
2. Convert any legacy queryA= URL params via NamedQuerySpec::from_legacy_string().
3. collect_chain_refs(&query) → list of ChainRef { key, field, summary }.
4. Deduplicate by key; group keys by index.
5. For each group, build ES count+fields query; batch via _msearch.
6. Validate: hits > limit → ChainError::TooManyHits (warn at 5_000, error at 10_000).
7. Extract field values per key → HashMap<String, Vec<String>>.
8. resolve_chain_refs(&mut query, &resolved) → mutates Attribute values in place.
9. Proceed to standard ES query build.
```

Step 5–7 live in `chain_executor.rs` (API crate).
Steps 1–3, 6 validation check, and 8 live in `chain.rs` (query crate, WASM-safe).

---

### 15.2 — Multi-ring arc

**Extend Phase 7** `ArcSpec` in `crates/genomehubs-query/src/report/arc.rs`:

```rust
pub struct ArcSpec {
    // Existing Phase 7 fields (single x/y/z triple)
    pub x: Option<String>,
    pub y: Option<String>,
    pub z: Option<String>,

    // NEW: N-ring extension
    pub rings: Option<Vec<RingSpec>>,
}

pub struct RingSpec {
    /// Filter query (fraction numerator when combined with y).
    pub x: String,
    /// Total population query (denominator). Defaults to outer RingSpec or ArcSpec.y.
    pub y: Option<String>,
    /// Optional second denominator (arc2 / inner fraction).
    pub z: Option<String>,
    /// Display label for this ring.
    pub label: Option<String>,
}
```

**Semantics**: When `rings` is present, each entry is one concentric ring. All rings
share the same `named_queries` map from the enclosing request. Rings are executed as a
single `_msearch` batch for efficiency.

**Response shape**:

```json
{
  "status": { "success": true },
  "report": {
    "arc": [
      { "ring": 0, "label": "has C-value", "arc": 0.41, "x": 8200, "y": 20000 },
      {
        "ring": 1,
        "label": "has assembly",
        "arc": 0.27,
        "x": 5400,
        "y": 20000
      },
      {
        "ring": 2,
        "label": "chromosome-level",
        "arc": 0.09,
        "x": 1800,
        "y": 20000
      }
    ]
  }
}
```

When `rings` is absent, the response is the existing Phase 7 single-arc shape
(backwards compatible).

**`_msearch` batching**: All 2N (or 3N) count queries for N rings are issued as a
single `_msearch` request, then results are zipped back to rings. This matches the
optimisation principle from Phase 5 (`AggBuilder`).

---

### 15.3 — SDK exposure

#### Python `QueryBuilder`

```python
def chain_query(
    self,
    query_key: str,
    query: str,
    result: str | None = None,
) -> "QueryBuilder":
    """Register a named sub-query for chain substitution."""
    ...

def arc(
    self,
    x: str,
    y: str,
    z: str | None = None,
    rings: list[dict] | None = None,
    named_queries: dict[str, str] | None = None,
) -> "QueryBuilder":
    """Arc report with optional multi-ring extension."""
    ...
```

#### `report_yaml` format

Single arc (Phase 7 unchanged):

```yaml
report: arc
x: "country=BR"
y: "genome_size>1000000"
```

Arc with named query:

```yaml
report: arc
x: "taxon_id=queryA.taxon_id"
y: "genome_size>0"
named_queries:
  queryA: "assembly--assembly_span>1e9"
```

Multi-ring arc:

```yaml
report: arc
y: "genome_size>0"
rings:
  - x: "genome_size>0"       label: "has genome size"
  - x: "c_value>0"           label: "has C-value"
  - x: "chromosome_count>0"  label: "has chromosome count"
```

Multi-ring arc with named queries:

```yaml
report: arc
y: "taxon_id=queryA.taxon_id"
named_queries:
  queryA: "taxon--tax_tree(Eukaryota)"
rings:
  - x: "assembly_span>1e9"   label: ">1 Gb assembly"
  - x: "assembly_span>1e8"   label: ">100 Mb assembly"
  - x: "assembly_span>1e7"   label: ">10 Mb assembly"
```

---

## Files to Create

| File                                                 | Purpose                                                                                                       |
| ---------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| `crates/genomehubs-query/src/query/chain.rs`         | `NamedQuerySpec`, `ChainRef`, `ChainError`, `parse_filter_string`, `collect_chain_refs`, `resolve_chain_refs` |
| `crates/genomehubs-api/src/routes/chain_executor.rs` | `execute_named_queries` — HTTP execution of named sub-queries via `_msearch`                                  |

## Files to Modify

| File                                                  | Change                                                                                                                         |
| ----------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| `crates/genomehubs-query/src/query/mod.rs`            | `pub mod chain;`; add `named_queries: Option<IndexMap<String, NamedQuerySpec>>` to `SearchQuery`; add `indexmap` to crate deps |
| `crates/genomehubs-api/src/routes/mod.rs`             | `pub mod chain_executor;`                                                                                                      |
| `crates/genomehubs-api/src/routes/search.rs`          | Convert legacy URL `queryA`/`queryB` params; call `execute_named_queries` + `resolve_chain_refs` before ES build               |
| `crates/genomehubs-api/src/routes/report.rs`          | Same pre-processing before arc/histogram dispatch                                                                              |
| `crates/genomehubs-query/src/report/arc.rs` (Phase 7) | Add `rings: Option<Vec<RingSpec>>`; `_msearch` batch execution                                                                 |
| `python/cli_generator/query.py`                       | `chain_query()`, extended `arc()`                                                                                              |
| `templates/python/query.py.tera`                      | Mirror                                                                                                                         |
| `templates/js/query.js`                               | `chainQuery()`, extended `arc()`                                                                                               |
| `templates/r/query.R`                                 | `chain_query()`, extended `arc()`                                                                                              |

---

## What is Explicitly Out of Scope for Phase 15

- **Query-defined categories** (e.g., `cat: [query: "assembly_span>1e9", label: ">1Gb"]`):
  extends the `cat` system in histogram/scatter. Deferred to Phase 16.
- **ES `terms` lookup** for sub-queries > 10,000 results: deferred. A clear error
  message with the count is sufficient for Phase 15.
- **msearch + histogram/scatter reports**: combining multiple independent histogram
  queries into a single `_msearch` batch is an optimisation deferred to Phase 16.
  The chain queries optimisation (parallel fetch for named queries) in 15.1 is
  sufficient for the immediate use case.

---

## Tests

| Test                                            | Location                             | Verifies                                               |
| ----------------------------------------------- | ------------------------------------ | ------------------------------------------------------ |
| `NamedQuerySpec::from_legacy_string` round-trip | unit test in `chain.rs`              | `assembly--field>val` → struct → back to legacy string |
| `parse_filter_string` single condition          | unit test in `chain.rs`              | `"assembly_span>1e9"` → `[Attribute{gt, 1000000000}]`  |
| `parse_filter_string` AND-joined                | unit test in `chain.rs`              | two conditions parsed correctly                        |
| `ChainRef::parse` match                         | unit test in `chain.rs`              | `"queryA.taxon_id"` → `ChainRef{key,field,summary}`    |
| `ChainRef::parse` non-match                     | unit test in `chain.rs`              | `"genome_size"` → `None`                               |
| `collect_chain_refs` + `resolve_chain_refs`     | unit test in `chain.rs`              | substitution mutates SearchQuery correctly             |
| `resolve_chain_refs` undefined key              | unit test in `chain.rs`              | `ChainError::UndefinedQuery`                           |
| `resolve_chain_refs` over threshold             | unit test in `chain.rs`              | `ChainError::TooManyHits`                              |
| `from_legacy_string` YAML round-trip            | `tests/python/test_core.py`          | parse via PyO3, verify struct fields                   |
| Multi-ring arc response shape                   | `tests/python/test_sdk_fixtures.py`  | rings array structure                                  |
| Multi-ring arc `_msearch` batch                 | unit test in `crates/genomehubs-api` | single HTTP call for N rings                           |

---

## v2 Bugs Fixed by This Implementation

| v2 Issue                                                                                          | v3 Fix                                                                                                                                                                        |
| ------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Named queries defined as opaque strings (`assembly--query_string`); `--` is a fragile string hack | `NamedQuerySpec` struct with typed `index: Option<SearchIndex>` and `filters: Vec<Attribute>`; `--` string only accepted at the URL-params boundary and immediately converted |
| Hard `chainThreshold=500` with opaque error                                                       | Configurable `limit` per `NamedQuerySpec` (default 500); server warn at 5,000, error at 10,000; error response includes `count` and `limit`                                   |
| `query[A-Z]$` vs `query[A-Z]+` regex inconsistency between arc and histogram                      | Uniform `named_queries: IndexMap<String, NamedQuerySpec>` on `SearchQuery`; key detection by `ChainRef::parse` — no regex ambiguity                                           |
| Sequential sub-query execution even when multiple named queries are independent                   | `execute_named_queries` batches per-index groups via `_msearch`                                                                                                               |
| No multi-ring arc (each ring must share the same x/y/z)                                           | `rings: Vec<RingSpec>` in `ArcSpec`                                                                                                                                           |
