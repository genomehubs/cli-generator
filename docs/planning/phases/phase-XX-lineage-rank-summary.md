# Phase XX: Lineage Rank Summary Aggregation

**Status:** Design capture — implementation ready
**Rationale:** Returns per-genus and per-family assembly distribution alongside species-level results in a single API request, eliminating the need for a second query and client-side join when planning genome projects
**Priority:** High-value for conservation genomics workflows (ERGA, VGP, etc.)

---

## Overview

### The problem

Species prioritisation for genome projects requires knowing the assembly coverage
at every phylogenetic level — species, genus, family. Today this requires:

1. A primary search for species in a clade (filtered to `tax_rank=species`).
2. A secondary search for genus/family documents whose `aggregation_source=descendant`
   fields carry rollup assembly quality data.
3. A client-side join on `genus_taxon_id` / `family_taxon_id`.

The join is unworkable at scale: a query returning 50,000 species may belong to
15,000 genera and 4,000 families. `chainQueries` (Phase 15) does not solve this
because it is a filter injector, not a per-row enricher.

### The solution

A `lineage_rank_summary` parameter in the search body instructs the API to run
nested ES aggregations **alongside** the main search in the same request. Each
spec names a taxonomic rank and one or more attribute fields. The aggregation
groups the matching species by the `taxon_id` of their ancestor at the requested
rank, then sub-aggregates on each attribute field. The result is returned as a
top-level `lineage_summary` object keyed by `{rank}.{ancestor_taxon_id}.{field}`.

### Two implementation strategies (design decision)

**Option A — ES nested aggregation** (chosen for v1 of this feature):

Runs one extra `nested(lineage)` aggregation per rank in the same ES request.
Computes the full distribution of values across all matching species — e.g.
how many species in a genus have each assembly level. Correct by construction:
only counts species that actually appear in the main query (not ancestor-
inherited values).

**Option B — `_mget` on ancestor documents** (simpler, different semantics):

Extract unique `genus_taxon_id` values from the result set, fetch those genus
documents via ES `_mget`, return the pre-computed rollup value from each. Much
simpler to implement. Returns the single best value per genus (the one stored
on the genus document), not a species count distribution. BUT has a subtle
correctness issue: GoaT's genus document stores `aggregation_source=ancestor`
for a field when NO descendant species has a value — the value was inherited
downward from the family. In this case the genus's stored `assembly_level` is
not reflecting its own species and must be discarded. Filtering requires an
extra `_mget` response field (`aggregation_source`) and conditional logic.

**Verdict:** Option A (ES agg) is chosen. It returns the correct distribution
without the `aggregation_source` ambiguity, handles nulls naturally (ancestor
buckets simply absent), and adds only one extra nested agg per rank to the
existing query — not a full second round-trip. Option B remains viable as a
future optimisation for single-best-value lookups.

### Multiple fields per rank

A spec covers **one rank and one or more fields**. Multiple fields at the same
rank are batched inside a single outer `nested(lineage)` aggregation, which is
more efficient than one outer agg per field:

```json
"lineage_rank_summary": [
  { "rank": "genus",  "fields": ["assembly_level", "ebp_standard_date"] },
  { "rank": "family", "fields": ["assembly_level"] }
]
```

This collapses two earlier-draft specs
`{ "rank": "genus", "field": "assembly_level" }` and
`{ "rank": "genus", "field": "ebp_standard_date" }` into one agg pass.

### Field type handling

Each field's inner sub-aggregation is selected based on its ES type, resolved
from the metadata cache (`get_attribute_value_field` / `processed_summary`):

| ES type                               | Aggregation                                                                                    | Value subfield                 | Result shape             |
| ------------------------------------- | ---------------------------------------------------------------------------------------------- | ------------------------------ | ------------------------ |
| `keyword` / `ordered_keyword`         | `terms`                                                                                        | `attributes.keyword_value.raw` | `{value: count, …}`      |
| `date`                                | `date_histogram` (calendar_interval: year) or `terms` on `keyword_value.raw` for sparse fields | `attributes.date_value`        | `{year: count, …}`       |
| `long` / `integer` / `short` / `byte` | `stats`                                                                                        | `attributes.long_value`        | `{min, max, avg, count}` |
| `float` / `half_float` / `double`     | `stats`                                                                                        | `attributes.half_float_value`  | `{min, max, avg, count}` |

For v1, implement keyword and numeric (`stats`) only. Date support can follow
(a `terms` on `keyword_value.raw` for dates gives ISO year-month strings, which
is sufficient for `ebp_standard_date`).

### Null / no-value handling

If no species in a genus has a value for a requested field:

- The `back_to_root → by_attribute → {field} → by_value` path returns an empty
  `buckets` array (for keyword) or `doc_count: 0` (for stats).
- The extractor produces `{}` for that genus+field combination.
- Genera with **zero** matching species in the main query simply do not appear
  in the `by_ancestor` buckets at all — they are fully absent from
  `lineage_summary`, which is correct.

This means `lineage_summary.genus["7090"]` being absent (genus has no matching
species in the query) is semantically distinct from
`lineage_summary.genus["7090"].assembly_level` being `{}` (genus has matching
species but none with an assembly).

### Example use case

"For each species in Lepidoptera, tell me the assembly level distribution and
earliest EBP date across its genus and family."

Request body (`POST /api/v3/search`):

```json
{
  "query_yaml": "index: taxon\ntaxa:\n  - Lepidoptera\ntaxon_filter_type: tree\nattributes:\n  - name: tax_rank\n    operator: eq\n    value: species\nfields:\n  - assembly_level\n  - genome_size\n",
  "params_yaml": "size: 50\npage: 1\n",
  "lineage_rank_summary": [
    { "rank": "genus", "fields": ["assembly_level", "ebp_standard_date"] },
    { "rank": "family", "fields": ["assembly_level"] }
  ]
}
```

Response envelope:

```json
{
  "status": { "hits": 183421, "ok": true },
  "url": "https://goat.genomehubs.org/api/v3/taxon?…",
  "results": [
    {
      "taxon_id": "7091",
      "scientific_name": "Bombyx mori",
      "assembly_level": "chromosome",
      "genome_size": 485000000,
      "lineage": { "genus_taxon_id": "7090", "family_taxon_id": "72019" }
    }
  ],
  "lineage_summary": {
    "genus": {
      "7090": {
        "assembly_level": { "chromosome": 1, "scaffold": 3, "contig": 12 },
        "ebp_standard_date": { "2024": 1, "2022": 2, "2019": 1 }
      },
      "7200": {
        "assembly_level": {}
      }
    },
    "family": {
      "72019": {
        "assembly_level": { "chromosome": 2, "scaffold": 14, "contig": 65 }
      }
    }
  }
}
```

Note genus `7200` has matching species in the query but none with an assembly
(`assembly_level: {}`). A genus with no matching species at all would be absent.

### SDK YAML interface

```yaml
# query_yaml fragment
lineage_rank_summary:
  - rank: genus
    fields:
      - assembly_level
      - ebp_standard_date
  - rank: family
    fields:
      - assembly_level
```

Because `lineage_rank_summary` scopes to the query rather than execution
parameters, it belongs in `query_yaml`, not `params_yaml`.

---

## Implementation

### 1. Data model — new struct

Add to `crates/genomehubs-query/src/query/mod.rs`:

```rust
/// Specification for a single lineage-rank aggregation requested alongside
/// the main search results.
///
/// `rank` names a taxonomic rank (e.g. `"genus"`, `"family"`, `"order"`).
/// `fields` names one or more attributes to aggregate within each ancestor bucket.
/// Multiple fields per rank are batched inside a single outer nested agg pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageRankSummarySpec {
    /// Taxonomic rank to group by (e.g. `"genus"`, `"family"`).
    pub rank: String,
    /// Attribute fields whose distributions to aggregate per ancestor.        // NEW plural
    /// e.g. `["assembly_level", "ebp_standard_date"]`
    pub fields: Vec<String>,
}
```

Add to `SearchQuery`:

```rust
pub struct SearchQuery {
    // … existing fields …

    /// Per-rank ancestor aggregations to compute alongside search results.    // NEW
    ///                                                                        // NEW
    /// Produces `lineage_summary` in the response envelope, keyed by         // NEW
    /// `{rank}.{ancestor_taxon_id}.{field}`.                                  // NEW
    #[serde(default, skip_serializing_if = "Option::is_none")]                // NEW
    pub lineage_rank_summary: Option<Vec<LineageRankSummarySpec>>,            // NEW
}
```

`SearchQuery::default()` gains `lineage_rank_summary: None` — fully backward-compatible.

---

### 2. ES aggregation design

One outer aggregation per spec (i.e. per distinct rank). All requested fields
are nested inside the same `by_ancestor` buckets, so `genus` with two fields
costs one outer agg, not two.

**Agg naming convention:** `lineage_{rank}` — e.g. `lineage_genus`.

Full ES body for the example request:

```json
{
  "query": { "…main_query…" },
  "size": 50,
  "aggs": {
    "lineage_genus": {
      "nested": { "path": "lineage" },
      "aggs": {
        "by_rank": {
          "filter": { "term": { "lineage.taxon_rank": "genus" } },
          "aggs": {
            "by_ancestor": {
              "terms": { "field": "lineage.taxon_id", "size": 50000 },
              "aggs": {
                "back_to_root": {
                  "reverse_nested": {},
                  "aggs": {
                    "by_attribute": {
                      "nested": { "path": "attributes" },
                      "aggs": {
                        "assembly_level": {
                          "filter": { "term": { "attributes.key": "assembly_level" } },
                          "aggs": {
                            "by_value": {
                              "terms": {
                                "field": "attributes.keyword_value.raw",
                                "size": 20
                              }
                            }
                          }
                        },
                        "ebp_standard_date": {
                          "filter": { "term": { "attributes.key": "ebp_standard_date" } },
                          "aggs": {
                            "by_value": {
                              "terms": {
                                "field": "attributes.keyword_value.raw",
                                "size": 30
                              }
                            }
                          }
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    },
    "lineage_family": {
      "nested": { "path": "lineage" },
      "aggs": {
        "by_rank": {
          "filter": { "term": { "lineage.taxon_rank": "family" } },
          "aggs": {
            "by_ancestor": {
              "terms": { "field": "lineage.taxon_id", "size": 10000 },
              "aggs": {
                "back_to_root": {
                  "reverse_nested": {},
                  "aggs": {
                    "by_attribute": {
                      "nested": { "path": "attributes" },
                      "aggs": {
                        "assembly_level": {
                          "filter": { "term": { "attributes.key": "assembly_level" } },
                          "aggs": {
                            "by_value": {
                              "terms": {
                                "field": "attributes.keyword_value.raw",
                                "size": 20
                              }
                            }
                          }
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
  }
}
```

**Key structural choices:**

| Choice                                                       | Rationale                                                           |
| ------------------------------------------------------------ | ------------------------------------------------------------------- |
| One outer agg per rank, all fields inside                    | Avoids re-traversing the lineage nested path once per field         |
| `nested(lineage)` → `filter(taxon_rank)` → `terms(taxon_id)` | Buckets documents by ancestor without a client-side join            |
| `reverse_nested` → `nested(attributes)`                      | Escapes lineage context to reach the document's attribute values    |
| `attributes.keyword_value.raw`                               | Matches the `.raw` suffix used in `bounds.rs` and existing agg code |
| `size: 50000` for genus, `size: 10000` for family            | Calibrated to maximum realistic clade sizes                         |

**Field-type dispatch** (resolved from metadata cache):

| Processed type                    | Aggregation      | ES subfield                                       |
| --------------------------------- | ---------------- | ------------------------------------------------- |
| `keyword` / `ordered_keyword`     | `terms(size=20)` | `attributes.keyword_value.raw`                    |
| `integer` / `long` / `short`      | `stats`          | `attributes.long_value`                           |
| `float` / `half_float` / `double` | `stats`          | `attributes.half_float_value`                     |
| `date` (v1 fallback)              | `terms(size=30)` | `attributes.keyword_value.raw` (ISO date strings) |

For v1, use `keyword_value.raw` for any field where the processed type is not
numeric. This is correct for `assembly_level` (ordered keyword), `long_list`
(keyword list), and `ebp_standard_date` (stored as ISO string in `keyword_value`
alongside the typed `date_value`). Numeric stats follow in v1.1.

**Null / no-value handling:**

- If no species in a genus has a value for field F, the `by_value.buckets` array
  is empty. The extractor emits `{}` for that genus+field.
- If a genus has zero species in the main query, it does not appear in
  `by_ancestor.buckets` at all. It is absent from `lineage_summary`.
- These two cases (`{}` vs absent) are semantically distinct and intentional.

---

### 3. Builder function — `build_lineage_rank_summary_agg`

Add to `crates/genomehubs-api/src/routes/` as a new file `lineage_agg.rs`:

```rust
use serde_json::{json, Value};
use crate::es_metadata::MetadataCache;
use genomehubs_query::query::LineageRankSummarySpec;
use std::sync::Arc;

/// Build one outer `nested(lineage)` aggregation for a `LineageRankSummarySpec`.
///
/// Covers all `spec.fields` in a single pass over `by_ancestor` buckets.
/// Returns `(agg_name, agg_body)` where `agg_name` is `lineage_{rank}`.
pub fn build_lineage_rank_summary_agg(
    spec: &LineageRankSummarySpec,
    ancestor_bucket_size: usize,
    cache: &Option<Arc<tokio::sync::RwLock<MetadataCache>>>,
) -> Result<(String, Value), String> {
    let agg_name = format!("lineage_{}", spec.rank);

    // Build one inner field sub-agg per requested field
    let mut field_aggs = serde_json::Map::new();
    for field in &spec.fields {
        let inner = build_field_sub_agg(field, cache)?;
        field_aggs.insert(field.clone(), inner);
    }

    let agg_body = json!({
        "nested": { "path": "lineage" },
        "aggs": {
            "by_rank": {
                "filter": { "term": { "lineage.taxon_rank": spec.rank } },
                "aggs": {
                    "by_ancestor": {
                        "terms": {
                            "field": "lineage.taxon_id",
                            "size": ancestor_bucket_size
                        },
                        "aggs": {
                            "back_to_root": {
                                "reverse_nested": {},
                                "aggs": {
                                    "by_attribute": {
                                        "nested": { "path": "attributes" },
                                        "aggs": Value::Object(field_aggs)
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    Ok((agg_name, agg_body))
}

/// Build the inner aggregation for a single attribute field.
///
/// Uses `stats` for numeric types (resolves via metadata cache);
/// falls back to `terms` on `keyword_value.raw` for all other types.
fn build_field_sub_agg(
    field: &str,
    cache: &Option<Arc<tokio::sync::RwLock<MetadataCache>>>,
) -> Result<Value, String> {
    let value_field = resolve_value_subfield(field, cache);

    let inner_agg = match &value_field {
        f if f.ends_with("long_value") || f.ends_with("half_float_value") => {
            json!({ "stats": { "field": f } })
        }
        _ => json!({
            "terms": {
                "field": "attributes.keyword_value.raw",
                "size": 20
            }
        }),
    };

    Ok(json!({
        "filter": { "term": { "attributes.key": field } },
        "aggs": { "by_value": inner_agg }
    }))
}

/// Resolve the correct ES value subfield for a field from the metadata cache.
///
/// Returns `attributes.keyword_value.raw` as a safe default when the field
/// is not in the cache or has an unrecognised type.
fn resolve_value_subfield(
    field: &str,
    cache: &Option<Arc<tokio::sync::RwLock<MetadataCache>>>,
) -> String {
    use serde_json::Value;

    let Some(cache_arc) = cache else {
        return "attributes.keyword_value.raw".to_string();
    };
    let Ok(guard) = cache_arc.try_read() else {
        return "attributes.keyword_value.raw".to_string();
    };
    let Value::Object(groups) = &guard.attr_types else {
        return "attributes.keyword_value.raw".to_string();
    };

    for group in groups.values() {
        let Value::Object(fields) = group else { continue };
        let Some(Value::Object(meta)) = fields.get(field) else { continue };
        let processed_type = meta.get("processed_type")
            .and_then(|v| v.as_str())
            .unwrap_or("keyword");

        return match processed_type {
            "integer" => "attributes.long_value".to_string(),
            "float"   => "attributes.half_float_value".to_string(),
            _         => "attributes.keyword_value.raw".to_string(),
        };
    }

    "attributes.keyword_value.raw".to_string()
}
```

---

### 4. Route handler integration — `post_search`

In `crates/genomehubs-api/src/routes/search.rs`, after building the main ES body:

```rust
// Inject lineage rank summary aggregations if requested               // NEW
if let Some(specs) = &query.lineage_rank_summary {                    // NEW
    if specs.len() > 5 {                                               // NEW
        bail!("lineage_rank_summary: maximum 5 specs");               // NEW
    }                                                                  // NEW
    let aggs = es_body                                                 // NEW
        .as_object_mut()                                               // NEW
        .unwrap()                                                      // NEW
        .entry("aggs")                                                 // NEW
        .or_insert_with(|| json!({}));                                 // NEW
    for spec in specs {                                                // NEW
        let size = ancestor_bucket_size_for_rank(&spec.rank);          // NEW
        let (name, body) = build_lineage_rank_summary_agg(            // NEW
            spec, size, &state.cache,                                  // NEW
        ).map_err(|e| bail!(e))?;                                      // NEW
        aggs[name] = body;                                             // NEW
    }                                                                  // NEW
}                                                                      // NEW
```

Helper to tune bucket sizes by rank:

```rust
fn ancestor_bucket_size_for_rank(rank: &str) -> usize {
    match rank {
        "genus"     => 50_000,
        "family"    => 10_000,
        "order"     => 2_000,
        "class"     => 500,
        _           => 10_000,
    }
}
```

`post_count` does not receive `lineage_rank_summary` — count responses have no
result set to join against.

---

### 5. Response extraction — `extract_lineage_summary`

```rust
/// Extract per-rank lineage summary from the ES aggregation response.
///
/// Returns a map: `rank → ancestor_taxon_id → {field → distribution}`.
/// Distribution shape depends on field type:
/// - keyword: `{"chromosome": 3, "scaffold": 2, …}`
/// - numeric: `{"min": 0.5, "max": 99.1, "avg": 72.3, "count": 40}`
pub fn extract_lineage_summary(
    es_resp: &Value,
    specs: &[LineageRankSummarySpec],
) -> Value {
    let mut summary = serde_json::Map::new();

    for spec in specs {
        let agg_name = format!("lineage_{}", spec.rank);
        let mut rank_map: serde_json::Map<String, Value> = serde_json::Map::new();

        let ancestor_buckets = es_resp
            .pointer(&format!("/aggregations/{agg_name}/by_rank/by_ancestor/buckets"))
            .and_then(|b| b.as_array())
            .cloned()
            .unwrap_or_default();

        for bucket in &ancestor_buckets {
            let ancestor_id = match bucket
                .get("key")
                .and_then(|k| k.as_str().map(str::to_string)
                    .or_else(|| k.as_u64().map(|n| n.to_string())))
            {
                Some(id) => id,
                None => continue,
            };

            let mut field_map = serde_json::Map::new();
            for field in &spec.fields {
                let path = format!("/back_to_root/by_attribute/{field}/by_value");
                let by_value = bucket.pointer(&path);

                let distribution = match by_value {
                    // Keyword: by_value is a terms agg with buckets array
                    Some(v) if v.get("buckets").is_some() => {
                        let mut counts = serde_json::Map::new();
                        for vb in v["buckets"].as_array().cloned().unwrap_or_default() {
                            let key = vb.get("key_as_string")
                                .or_else(|| vb.get("key"))
                                .and_then(|k| k.as_str())
                                .unwrap_or("unknown");
                            let count = vb["doc_count"].as_u64().unwrap_or(0);
                            counts.insert(key.to_string(), json!(count));
                        }
                        Value::Object(counts)
                    }
                    // Numeric: by_value is a stats agg object
                    Some(v) if v.get("count").is_some() => v.clone(),
                    // No data for this field in this ancestor
                    _ => json!({}),
                };

                field_map.insert(field.clone(), distribution);
            }

            rank_map.insert(ancestor_id, Value::Object(field_map));
        }

        summary.insert(spec.rank.clone(), Value::Object(rank_map));
    }

    Value::Object(summary)
}
```

---

### 6. Updated `SearchResponse` struct

```rust
#[derive(Serialize)]
pub struct SearchResponse {
    pub status: ApiStatus,
    pub url: String,
    pub results: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_after: Option<Value>,
    /// Per-rank ancestor aggregation results.                                 // NEW
    ///                                                                        // NEW
    /// Shape: `{rank: {ancestor_taxon_id: {field: distribution}}}`.          // NEW
    /// Only present when `lineage_rank_summary` was requested.               // NEW
    #[serde(skip_serializing_if = "Option::is_none")]                        // NEW
    pub lineage_summary: Option<Value>,                                       // NEW
}
```

---

### 7. SDK exposure — Python

Add `set_lineage_rank_summary` to `QueryBuilder` in
`python/cli_generator/query.py` and `templates/python/query.py.tera`:

```python
def set_lineage_rank_summary(
    self,
    specs: list[dict[str, str | list[str]]],
) -> "QueryBuilder":
    """Request per-ancestor aggregations alongside search results.

    Each spec is a dict with ``rank`` and ``fields`` keys:

    .. code-block:: python

        qb.set_lineage_rank_summary([
            {"rank": "genus",  "fields": ["assembly_level", "ebp_standard_date"]},
            {"rank": "family", "fields": ["assembly_level"]},
        ])

    The response ``lineage_summary`` map contains value distributions keyed by
    ``rank → ancestor_taxon_id → field``.
    """
    self._query["lineage_rank_summary"] = specs
    return self
```

Usage:

```python
from cli_generator import QueryBuilder

results = (
    QueryBuilder("taxon")
    .set_taxa(["Rodentia"], filter_type="tree")
    .set_rank("species")
    .set_fields(["assembly_level"])
    .set_lineage_rank_summary([
        {"rank": "genus",  "fields": ["assembly_level", "ebp_standard_date"]},
        {"rank": "family", "fields": ["assembly_level"]},
    ])
    .run()
)

for hit in results["results"]:
    gid = hit.get("lineage", {}).get("genus_taxon_id")
    genus_levels = results["lineage_summary"]["genus"].get(gid, {})
    print(hit["scientific_name"], genus_levels.get("assembly_level", {}))
```

### SDK exposure — JavaScript

```javascript
setLineageRankSummary(specs) {
    /**
     * @param {Array<{rank: string, fields: string[]}>} specs
     */
    this.query.lineage_rank_summary = specs;
    return this;
}
```

### SDK exposure — R

```r
set_lineage_rank_summary = function(specs) {
    #' Request per-ancestor aggregations alongside search results.
    #' @param specs A list of named lists, each with `rank` and `fields` keys.
    #' @return The builder (invisibly) for chaining.
    private$query[["lineage_rank_summary"]] <- specs
    invisible(self)
},
```

---

## Worked example — full response data

Query: species in Rodentia, genus assembly level + EBP date, family assembly level.

```json
{
  "status": { "hits": 2654, "ok": true },
  "results": [
    {
      "taxon_id": "10090",
      "scientific_name": "Mus musculus",
      "assembly_level": "chromosome",
      "lineage": { "genus_taxon_id": "10088", "family_taxon_id": "337687" }
    },
    {
      "taxon_id": "10116",
      "scientific_name": "Rattus norvegicus",
      "assembly_level": "chromosome",
      "lineage": { "genus_taxon_id": "10114", "family_taxon_id": "337687" }
    }
  ],
  "lineage_summary": {
    "genus": {
      "10088": {
        "assembly_level": { "chromosome": 3, "scaffold": 2, "contig": 18 },
        "ebp_standard_date": { "2022-01-15": 1, "2024-03-10": 2 }
      },
      "10114": {
        "assembly_level": { "chromosome": 2, "scaffold": 5, "contig": 9 },
        "ebp_standard_date": {}
      }
    },
    "family": {
      "337687": {
        "assembly_level": { "chromosome": 12, "scaffold": 31, "contig": 84 }
      }
    }
  }
}
```

Genus `10114` (_Rattus_) has no EBP date on any of its species → `ebp_standard_date: {}`.

Client-side join: `results["lineage_summary"]["genus"][hit.lineage.genus_taxon_id]`.

---

## Precedent in v2 codebase

- `aggregateRanks.js` — per-rank ES aggs over the lineage nested field; same
  `nested(lineage)` → `filter(taxon_rank)` pattern used here.
- `matchRanks.js` — uses `inner_hits` on the lineage nested field.
- The `reverse_nested` → `nested(attributes)` → `filter(key)` pattern is already
  established in `agg.rs` for report aggregations.

---

## Test coverage

| Test                                                                                | Location                                                    |
| ----------------------------------------------------------------------------------- | ----------------------------------------------------------- |
| `test_build_lineage_rank_summary_agg_keyword` — well-formed agg with keyword field  | `crates/genomehubs-api/src/routes/lineage_agg.rs`           |
| `test_build_lineage_rank_summary_agg_numeric` — `stats` agg for long field          | Same                                                        |
| `test_build_lineage_rank_summary_agg_multi_field` — two fields in one rank agg      | Same                                                        |
| `test_extract_lineage_summary_keyword` — correct map from mock ES keyword response  | Same                                                        |
| `test_extract_lineage_summary_numeric` — correct stats map from mock ES response    | Same                                                        |
| `test_extract_lineage_summary_empty_field` — `{}` when field has no values          | Same                                                        |
| `test_extract_lineage_summary_absent_genus` — genus absent when no matching species | Same                                                        |
| `test_search_query_lineage_rank_summary_serde` — round-trip YAML parse              | `crates/genomehubs-query/src/query/mod.rs`                  |
| `test_lineage_rank_summary_python_builder` — `set_lineage_rank_summary` YAML output | `tests/python/test_core.py`                                 |
| Integration: POST search with `lineage_rank_summary` against live ES                | `tests/python/test_batch_integration.py` (skip without API) |

---

## Performance considerations

- One outer `nested(lineage)` agg per distinct rank; additional fields inside a
  rank add only inner filter aggs (cheap). Two ranks with three fields total ≈
  two outer aggs, not five.
- `size: 50000` on the genus ancestor `terms` agg adds ~5–20 ms for large clades
  (Lepidoptera, Coleoptera). Family is cheaper (`size: 10000`).
- Cap at 5 rank specs to prevent abuse; return an error for larger requests.
- ES `terms` agg accuracy degrades for `size > 10000`; `shard_size` defaults to
  `size * 1.5`. For exact counts on very large clades, document that the
  distribution is approximate for ranks with >50,000 genera.

---

## Backward compatibility

- `lineage_rank_summary` is `Option<Vec<…>>`, defaulting to `None`.
- Existing clients send no `lineage_rank_summary` → no extra agg → no
  `lineage_summary` key in the response.
- `SearchResponse.lineage_summary` is `skip_serializing_if = "Option::is_none"`.

---

## Future enhancements

1. **`_mget` mode (Option B):** For use cases that only need the single best
   pre-computed genus value, expose an alternative `lineage_rank_values: bool`
   flag that fetches genus/family documents via `_mget` instead. Faster but
   requires `aggregation_source` filtering by the caller.
2. **`max_ancestors` override:** Per-spec `size` field to override rank defaults.
3. **Date histogram:** Replace `terms` for date fields with a proper
   `date_histogram(calendar_interval: year)` agg for cleaner bucketing.
4. **Count endpoint:** Optionally support `lineage_rank_summary` on `/count`
   as a "count by ancestor" aggregation without retrieving hit records.
5. **Modifiers:** An optional `modifier` per field (`"min"`, `"max"`, `"count"`)
   to select a specific statistic for numeric fields rather than the full stats
   object. Low priority — the full stats object is already compact.
