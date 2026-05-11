# Phase XX: ID-Set Filter

**Status:** Design capture (not sequenced into ordered phases yet)
**Rationale:** Allows queries to be filtered to an arbitrary user-supplied set of taxon IDs without requiring those IDs to be indexed as a named project (`long_list`), enabling project planning for non-integrated datasets
**Priority:** High-value for project planning workflows; complements `long_list` and `chainQueries`; can be implemented independently of Phase 15

---

## Overview

### The problem

GoaT supports project-based filtering via the `long_list` field (e.g.
`long_list=ERGA`), but only for projects that have been formally indexed into the
database. For non-integrated projects — a spreadsheet of target species, a
custom list from a collaborator, the output of a prior analysis — there is no
mechanism to restrict a query to exactly those taxa without:

1. Client-side `search_all` + local intersection (slow, transfers all data).
2. `chainQueries` (Phase 15) — only works when the IDs come from a prior API
   query, not from an external list.
3. `multiTerm` with newline-separated names — practical only for ~200–500 species;
   breaks under URL length limits and doesn't accept numeric IDs directly.

### The solution

An `id_set` field in the request body injects a single ES `terms` clause ANDed
with the existing query. The `terms` clause is the canonical way to filter an
ES search to a known set of IDs; it is evaluated against an inverted-index
cache and costs O(1) per document regardless of set size (up to the 65,536
ES hard limit).

### Example use case

"I have a list of 4,000 target taxon IDs from a project planning spreadsheet.
Show me assembly status for those taxa only."

Request body (`POST /api/v3/search`):

```json
{
  "query_yaml": "index: taxon\n",
  "params_yaml": "size: 50\npage: 1\n",
  "id_set": [10090, 10116, 9606, 7955, 3702, 6239, 7227]
}
```

Because `id_set` is an execution modifier (it refines which documents are
returned, not what the query means), it belongs in `params_yaml` for SDK
users, or as a top-level key alongside `query_yaml` / `params_yaml` for
direct API callers. See §SDK interface for the recommended placement.

---

## Implementation

### 1. Data model — `QueryParams` extension

Add to `crates/genomehubs-query/src/query/mod.rs`:

```rust
pub struct QueryParams {
    // … existing fields …

    /// Filter results to exactly this set of taxon IDs.            // NEW
    ///                                                              // NEW
    /// Injected as an ES `terms` clause ANDed with the main query. // NEW
    /// Maximum 65,536 entries (ES hard limit for `terms` filters). // NEW
    /// The SDK emits a structured error above this limit.          // NEW
    #[serde(default, skip_serializing_if = "Option::is_none")]      // NEW
    pub id_set: Option<Vec<u64>>,                                   // NEW
}
```

`QueryParams::default()` gains `id_set: None` — fully backward-compatible.

Add the same field to `QueryParams::default()` and the serde round-trip:

```rust
impl Default for QueryParams {
    fn default() -> Self {
        Self {
            // … existing fields …
            id_set: None,  // NEW
        }
    }
}
```

---

### 2. Validation — size limits

Add to the `from_yaml` parsing path or to a new `QueryParams::validate` method:

```rust
/// Hard ES limit for a `terms` filter clause.
const ES_TERMS_LIMIT: usize = 65_536;
/// Soft warning threshold — larger sets may impact query latency.
const ID_SET_WARN_THRESHOLD: usize = 10_000;

impl QueryParams {
    /// Validate that `id_set` is within acceptable bounds.
    ///
    /// Returns `Err` if the set exceeds the ES hard limit.
    /// Logs a warning if the set exceeds the soft threshold.
    pub fn validate_id_set(&self) -> Result<(), String> {
        if let Some(ids) = &self.id_set {
            if ids.len() > ES_TERMS_LIMIT {
                return Err(format!(
                    "id_set contains {} IDs, which exceeds the ES terms clause \
                     limit of {ES_TERMS_LIMIT}",
                    ids.len()
                ));
            }
            if ids.len() > ID_SET_WARN_THRESHOLD {
                // Replace with tracing::warn! in the API crate
                eprintln!(
                    "WARNING: id_set contains {} IDs (>{ID_SET_WARN_THRESHOLD}); \
                     large term sets may increase query latency",
                    ids.len()
                );
            }
        }
        Ok(())
    }
}
```

---

### 3. ES query injection

Add a helper to `crates/genomehubs-query/src/query/` or inline in the route
handler:

```rust
/// Inject an `id_set` filter into an existing ES query body.
///
/// The `terms` clause is ANDed with the existing `bool.must` context.
/// If the existing query is `match_all`, it is wrapped in a `bool.must`.
pub fn inject_id_set_filter(es_body: &mut Value, taxon_ids: &[u64]) {
    if taxon_ids.is_empty() {
        return;
    }

    let id_values: Vec<Value> = taxon_ids.iter().map(|id| json!(id)).collect();
    let terms_clause = json!({ "terms": { "taxon_id": id_values } });

    // Navigate to or create the bool.must array
    let query = es_body
        .get_mut("query")
        .unwrap_or_else(|| { es_body["query"] = json!({}); &mut es_body["query"] });

    if let Some(bool_query) = query.get_mut("bool") {
        let must = bool_query
            .as_object_mut()
            .and_then(|obj| obj.get_mut("must"));

        match must {
            Some(Value::Array(arr)) => arr.push(terms_clause),
            Some(existing) => {
                *existing = json!([existing.clone(), terms_clause]);
            }
            None => {
                bool_query["must"] = json!([terms_clause]);
            }
        }
    } else {
        // Wrap the current query in a bool.must alongside the terms filter
        let original_query = query.clone();
        *query = json!({
            "bool": {
                "must": [original_query, terms_clause]
            }
        });
    }
}
```

**Applies to all three search routes:** `post_search`, `post_count`,
`post_search_batch` — the filter is purely additive.

---

### 4. Route handler integration

In `crates/genomehubs-api/src/routes/search.rs`, after building the main ES body:

```rust
// Validate id_set size
if let Err(e) = params.validate_id_set() {
    bail!(e);
}

// Build the main ES query body (existing path)
let mut es_body = build_search_body(…)?;

// Inject id_set terms filter if present                       // NEW
if let Some(ids) = &params.id_set {                           // NEW
    inject_id_set_filter(&mut es_body, ids);                  // NEW
}                                                             // NEW
```

Apply the same three lines to `post_count` (`count.rs`) and
`post_search_batch` (`searchBatch.rs`).

The ES `terms` clause for `taxon_id` looks like:

```json
{
  "bool": {
    "must": [
      { "…main query…" },
      { "terms": { "taxon_id": [10090, 10116, 9606, 7955, 3702, 6239, 7227] } }
    ]
  }
}
```

The `taxon_id` field in the taxon index is a `keyword` (ES). For the assembly
index, the correct field is `taxon_id` too (each assembly document holds the
taxon_id of its source organism).

---

### 5. SDK interface — Python

Add `set_id_set` to `QueryBuilder` in `python/cli_generator/query.py` and
`templates/python/query.py.tera`:

```python
def set_id_set(self, taxon_ids: list[int]) -> "QueryBuilder":
    """Restrict results to exactly the supplied taxon IDs.

    Injected as an ES ``terms`` filter ANDed with the main query.
    Maximum 65,536 IDs (ES hard limit). A structured error is returned
    for larger sets.

    Args:
        taxon_ids: List of integer taxon IDs to include.

    Example::

        qb.set_id_set([10090, 10116, 9606])
    """
    self._params["id_set"] = taxon_ids
    return self
```

Note: `id_set` is placed in `_params` (rendered into `params_yaml`) not `_query`,
because it is an execution filter rather than a semantic query specification.

Full Python usage:

```python
from cli_generator import QueryBuilder

# Load target IDs from any external source
target_ids = [10090, 10116, 9606, 7955, 3702, 6239, 7227]

results = (
    QueryBuilder("taxon")
    .set_fields(["assembly_level", "genome_size", "busco_completeness"])
    .set_id_set(target_ids)
    .run()
)

for hit in results["results"]:
    print(hit["taxon_id"], hit.get("assembly_level"))
```

Combining with a clade filter:

```python
# IDs must also be within Rodentia
results = (
    QueryBuilder("taxon")
    .set_taxa(["Rodentia"], filter_type="tree")
    .set_rank("species")
    .set_fields(["assembly_level"])
    .set_id_set(target_ids)
    .run()
)
```

### SDK interface — JavaScript

Add to `QueryBuilder` in `templates/js/query.js.tera`:

```javascript
setIdSet(taxonIds) {
    /**
     * Restrict results to exactly the supplied taxon IDs.
     *
     * Injected as an ES `terms` filter. Maximum 65,536 IDs.
     *
     * @param {number[]} taxonIds
     */
    this.params.id_set = taxonIds;
    return this;
}
```

Usage:

```javascript
const results = await new QueryBuilder("taxon")
  .setFields(["assembly_level", "genome_size"])
  .setIdSet([10090, 10116, 9606])
  .run();
```

### SDK interface — R

Add to the R6 `QueryBuilder` class in `templates/r/query.R.tera`:

```r
set_id_set = function(taxon_ids) {
    #' Restrict results to exactly the supplied taxon IDs.
    #'
    #' Injected as an ES `terms` filter. Maximum 65,536 IDs.
    #'
    #' @param taxon_ids Integer vector of taxon IDs.
    #' @return The builder (invisibly) for chaining.
    private$params[["id_set"]] <- as.integer(taxon_ids)
    invisible(self)
},
```

Usage:

```r
results <- QueryBuilder$new("taxon")$
    set_fields(c("assembly_level", "genome_size"))$
    set_id_set(c(10090L, 10116L, 9606L))$
    run()
```

---

### 6. WASM / browser SDK

For WASM-based use, `id_set` goes in `params_yaml` (not `query_yaml`), following
the placement convention for `QueryParams` fields:

```yaml
# params_yaml
size: 50
page: 1
id_set:
  - 10090
  - 10116
  - 9606
```

The WASM `build_url` function composes the params YAML into the request, so
`id_set` reaches the API transparently through the existing serialisation path.

---

## Error response

When `id_set` exceeds 65,536 entries:

```json
{
  "status": {
    "ok": false,
    "error": "id_set contains 70000 IDs, which exceeds the ES terms clause limit of 65536"
  },
  "url": "",
  "results": []
}
```

The SDK raises this as a `ValueError` in Python, an `Error` in JavaScript, and
`stop()` in R before the request is sent (client-side validation).

---

## Worked example — full response data

Query: taxon IDs for four well-sequenced model organisms + assembly status.

Request:

```json
{
  "query_yaml": "index: taxon\n",
  "params_yaml": "size: 10\npage: 1\nid_set:\n  - 9606\n  - 10090\n  - 10116\n  - 7955\n"
}
```

Response:

```json
{
  "status": { "hits": 4, "ok": true },
  "url": "…",
  "results": [
    {
      "taxon_id": "9606",
      "scientific_name": "Homo sapiens",
      "assembly_level": "chromosome",
      "genome_size": 3099441038
    },
    {
      "taxon_id": "10090",
      "scientific_name": "Mus musculus",
      "assembly_level": "chromosome",
      "genome_size": 2728222451
    },
    {
      "taxon_id": "10116",
      "scientific_name": "Rattus norvegicus",
      "assembly_level": "chromosome",
      "genome_size": 2870184193
    },
    {
      "taxon_id": "7955",
      "scientific_name": "Danio rerio",
      "assembly_level": "chromosome",
      "genome_size": 1679204697
    }
  ]
}
```

Note `"hits": 4` — the `terms` filter restricts the total result count to the
intersection of the main query and the supplied IDs.

---

## Relationship to other filtering mechanisms

| Mechanism                         | Best use                                                                     |
| --------------------------------- | ---------------------------------------------------------------------------- |
| `long_list=PROJECT`               | Integrated projects with IDs indexed in GoaT (`long_list` field)             |
| `id_set`                          | External lists from spreadsheets, prior analyses, or non-integrated projects |
| `chainQueries` (Phase 15)         | IDs derived from a prior API query in the same request                       |
| `taxa` + `taxon_filter_type=tree` | Entire clade without a specific ID list                                      |
| `multiTerm`                       | Small named-taxon lists (~200–500 names, not numeric IDs)                    |

`id_set` and `long_list` are orthogonal and can be combined:

```yaml
# Find ERGA targets that are also in my custom list
index: taxon
attributes:
  - name: long_list
    operator: eq
    value: ERGA
```

```json
{ "…query_yaml…": "…", "id_set": [11111, 22222, 33333] }
```

This ANDs both filters: the result must be in ERGA **and** in the supplied set.

---

## Test coverage

| Test                                                                        | Location                                                    |
| --------------------------------------------------------------------------- | ----------------------------------------------------------- |
| `test_inject_id_set_filter_into_match_all` — wraps `match_all` correctly    | `crates/genomehubs-query/src/query/mod.rs`                  |
| `test_inject_id_set_filter_into_bool_must_array` — appends to existing must | Same                                                        |
| `test_inject_id_set_filter_empty` — no-op on empty slice                    | Same                                                        |
| `test_validate_id_set_over_limit` — returns `Err` at 65,537 entries         | Same                                                        |
| `test_validate_id_set_under_limit` — returns `Ok` at 65,536 entries         | Same                                                        |
| `test_query_params_id_set_serde` — round-trip YAML parse with `id_set`      | Same                                                        |
| `test_set_id_set_python_builder` — `set_id_set` populates `params_yaml`     | `tests/python/test_core.py`                                 |
| `test_set_id_set_js_builder` — `setIdSet` populates params correctly        | `tests/javascript/test_sdk_fixtures.mjs`                    |
| `test_set_id_set_r_builder` — `set_id_set` populates params correctly       | `tests/r/test_sdk_fixtures.R`                               |
| Integration: POST search with `id_set` returns only matching IDs            | `tests/python/test_batch_integration.py` (skip without API) |

---

## Backward compatibility

- `id_set` is `Option<Vec<u64>>`, defaulting to `None`.
- Existing API clients unaffected — field absent → no `terms` clause injected.
- `deny_unknown_fields` on `QueryParams` must NOT be set (or `id_set` added to the
  allowed fields list before deploying). Check the current serde configuration.

---

## Future enhancements

1. **Assembly-index variant:** Accept `assembly_id` strings (e.g. `GCF_000001405.40`)
   via a parallel `assembly_id_set: Option<Vec<String>>` field on `QueryParams`.
2. **File upload endpoint:** A `POST /api/v3/id-set-upload` endpoint that accepts
   a plain-text file of IDs and returns a short-lived token, allowing `id_set`
   to be replaced by `id_set_token: "abc123"` for very large sets stored server-side.
3. **Streaming intersection:** For sets exceeding 65,536 IDs, automatically split
   into batches of 65,536 and union the results client-side (implemented in the
   Python/JS/R SDK, transparent to the user).
4. **Named ID sets:** A `POST /api/v3/id-sets` management endpoint that lets users
   store and retrieve named sets, referenced in queries as `id_set_name: "my-targets"`.
   Requires authentication and a persistent store — significant scope increase.
