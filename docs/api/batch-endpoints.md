# Batch Endpoints: /searchBatch and /countBatch

**Version:** v3 API
**Status:** Production (Phase 3a.1+)
**Last Updated:** May 5, 2026

---

## Overview

The v3 API provides two batch endpoints for high-performance multi-query operations:

| Endpoint              | Method | Purpose                                  | Returns                      |
| --------------------- | ------ | ---------------------------------------- | ---------------------------- |
| `/api/v3/searchBatch` | POST   | Execute multiple searches in one request | Document hits for each query |
| `/api/v3/countBatch`  | POST   | Get hit counts for multiple queries      | Count results for each query |

Both endpoints support:

- **Up to 100 searches per request** (batch optimization)
- **Multi-query combining** with OR/AND logic (Phase 3a.1+)
- **Lineage resolution** for hierarchical filters
- **Mixed filter types** (name, tree, lineage) per query

---

## Common Request Format

Both endpoints accept identical POST request bodies:

```json
{
  "searches": [
    {
      "query_yaml": "...",
      "params_yaml": "..."
    }
  ]
}
```

### `query_yaml` Field

The YAML-serialized query structure. Supports:

**Single-query mode:**

```yaml
index: taxon
taxa: [Canis lupus]
taxon_filter_type: tree
```

**Multi-query mode (Phase 3a.1+):**

```yaml
index: taxon
combine_with: OR
queries:
  - taxa: [Canis lupus]
    taxon_filter_type: tree
  - taxa: [Felis]
    taxon_filter_type: tree
```

### `params_yaml` Field

Pagination and result options:

```yaml
size: 10 # Results per page (searchBatch) / ignored (countBatch)
from: 0 # Pagination offset (searchBatch only)
sort: "name" # Sort field (searchBatch only)
```

For `countBatch`, set `size: 0` to optimize (hits are counted without fetching results).

### Request Constraints

- **Max 100 searches per request** — API returns error if exceeded
- **Max 10 nested queries per search item** — When using multi-query mode
- **Same index for all nested queries** — Multi-query combining requires single index
- **Non-empty queries list** — At least one nested query required in multi-query mode

---

## Single-Query Requests

### countBatch: Count Results

**Request:**

```bash
curl -X POST http://localhost:3000/api/v3/countBatch \
  -H "Content-Type: application/json" \
  -d '{
    "searches": [
      {
        "query_yaml": "index: taxon\ntaxa: [Mammalia]",
        "params_yaml": "size: 0"
      }
    ]
  }'
```

**Response:**

```json
{
  "status": {
    "success": true,
    "hits": 150000,
    "took": 45
  },
  "results": [
    {
      "status": {
        "success": true,
        "hits": 150000,
        "took": 45
      },
      "count": 150000
    }
  ]
}
```

### searchBatch: Get Document Results

**Request:**

```bash
curl -X POST http://localhost:3000/api/v3/searchBatch \
  -H "Content-Type: application/json" \
  -d '{
    "searches": [
      {
        "query_yaml": "index: taxon\ntaxa: [Canis lupus]",
        "params_yaml": "size: 5"
      }
    ]
  }'
```

**Response:**

```json
{
  "status": {
    "success": true,
    "hits": 26,
    "took": 12
  },
  "results": [
    {
      "status": {
        "success": true,
        "hits": 26,
        "took": 12
      },
      "hits": [
        {
          "taxon_id": 9646,
          "scientific_name": "Canis lupus",
          "rank": "species",
          "lineage": [...]
        },
        ...
      ]
    }
  ]
}
```

---

## Multi-Query Combining (Phase 3a.1+)

### Request Format

Use the `queries` field to combine multiple search conditions:

```yaml
index: taxon
combine_with: OR # or "AND"
queries:
  - taxa: [Canis lupus]
    taxon_filter_type: tree
  - taxa: [Felis]
    taxon_filter_type: tree
```

### OR Combining (Union)

Returns results matching **any** of the sub-queries:

**Request:**

```bash
curl -X POST http://localhost:3000/api/v3/countBatch \
  -H "Content-Type: application/json" \
  -d '{
    "searches": [
      {
        "query_yaml": "index: taxon\ncombine_with: OR\nqueries:\n  - taxa: [\"9612\"]\n    taxon_filter_type: name\n  - taxa: [\"9611\"]\n    taxon_filter_type: name",
        "params_yaml": "size: 0"
      }
    ]
  }'
```

**Response:**

```json
{
  "status": { "success": true, "hits": 2, "took": 8 },
  "results": [
    {
      "status": { "success": true, "hits": 2, "took": 8 },
      "count": 2
    }
  ]
}
```

**Behavior:** Returns sum of individual query hits (no duplicates; Elasticsearch `bool.should` with `minimum_should_match: 1`)

### AND Combining (Intersection)

Returns results matching **all** sub-queries:

```yaml
index: assembly
combine_with: AND
queries:
  - attributes:
      - name: genome_size
        operator: lt
        value: "3G"
  - attributes:
      - name: species_count
        operator: gt
        value: "100"
```

**Behavior:** Returns only records satisfying all conditions (Elasticsearch `bool.must`)

---

## Filter Types Reference

All batch queries support three filter types for hierarchical searches:

### 1. `name` Filter

Match by exact taxon name (case-sensitive):

```yaml
taxa: [Canis lupus]
taxon_filter_type: name
```

**Behavior:** Returns only exact matches

### 2. `tree` Filter

Match taxon and all descendants:

```yaml
taxa: [Mammalia]
taxon_filter_type: tree
```

**Behavior:** Returns the taxon plus all descendants (subtree)

### 3. `lineage` Filter

Match by ancestor, using taxon lineage resolution:

```yaml
taxa: [Chordata]
taxon_filter_type: lineage
```

**Behavior:**

1. Resolve query taxa to taxon IDs
2. For each document, extract its taxon's lineage (ancestor chain)
3. Match if any ancestor matches the query taxa

**Note:** Lineage resolution is performed for each nested query independently when using multi-query combining.

---

## Mixing Multiple Searches in One Batch

Combine many independent queries in a single request:

**Request:**

```bash
curl -X POST http://localhost:3000/api/v3/countBatch \
  -H "Content-Type: application/json" \
  -d '{
    "searches": [
      {
        "query_yaml": "index: taxon\ntaxa: [Mammalia]\ntaxon_filter_type: tree",
        "params_yaml": "size: 0"
      },
      {
        "query_yaml": "index: taxon\ntaxa: [Aves]\ntaxon_filter_type: tree",
        "params_yaml": "size: 0"
      },
      {
        "query_yaml": "index: assembly\nattributes:\n  - name: genome_size\n    operator: lt\n    value: \"3G\"",
        "params_yaml": "size: 0"
      }
    ]
  }'
```

**Response:**

```json
{
  "status": { "success": true, "hits": 999999, "took": 125 },
  "results": [
    {
      "status": { "success": true, "hits": 150000, "took": 45 },
      "count": 150000
    },
    {
      "status": { "success": true, "hits": 120000, "took": 38 },
      "count": 120000
    },
    {
      "status": { "success": true, "hits": 729999, "took": 42 },
      "count": 729999
    }
  ]
}
```

**Note:** Total hits in envelope is sum of all queries; each result shows per-query metrics.

---

## Error Handling

### Common Error Cases

**Too many searches (>100):**

```json
{
  "status": {
    "success": false,
    "error": "maximum 100 searches per request"
  },
  "results": []
}
```

**Invalid YAML in query_yaml:**

```json
{
  "status": {
    "success": false,
    "error": "error deserializing YAML: invalid syntax..."
  },
  "results": []
}
```

**Multi-query with mismatched indices:**

```json
{
  "status": {
    "success": false,
    "error": "multi-query combining requires all queries to use same index"
  },
  "results": []
}
```

**Multi-query with >10 nested queries:**

```json
{
  "status": {
    "success": false,
    "error": "maximum 10 queries per multi-query search"
  },
  "results": []
}
```

### Per-Query Errors

Individual search failures are reported in the results array:

```json
{
  "status": { "success": true, "hits": 150000, "took": 125 },
  "results": [
    {
      "status": { "success": true, "hits": 100000, "took": 45 },
      "count": 100000
    },
    { "status": { "success": false, "error": "unknown index: inventory" } }
  ]
}
```

---

## Migration from V2 API

### V2 `/msearch` → V3 `/countBatch`

**V2:**

```bash
# V2 msearch pattern (if available in old API)
POST /api/v2/msearch
{
  "queries": [
    { "query": "taxa:Mammalia" },
    { "query": "taxa:Aves" }
  ]
}
```

**V3 equivalent:**

```bash
POST /api/v3/countBatch
{
  "searches": [
    { "query_yaml": "index: taxon\ntaxa: [Mammalia]", "params_yaml": "size: 0" },
    { "query_yaml": "index: taxon\ntaxa: [Aves]", "params_yaml": "size: 0" }
  ]
}
```

### V2 String Parsing → V3 YAML Structure

**V2:** Query strings like `tax_name(wolf) OR tax_name(strawberry)`

**V3:** Parsed into structured multi-query (Phase 9 planned to add string parsing to `/search` and `/count`; batch endpoints require structured input):

```yaml
index: taxon
combine_with: OR
queries:
  - taxa: [wolf]
    taxon_filter_type: name
  - taxa: [strawberry]
    taxon_filter_type: name
```

### V2 Lineage Filters → V3 Batch Lineage

**V2:** Implicit lineage resolution in query parsing

**V3:** Explicit `taxon_filter_type: lineage`:

```yaml
index: taxon
taxa: [Chordata]
taxon_filter_type: lineage
```

---

## Performance Considerations

- **Batch size:** Up to 100 searches per request; exceeding returns error
- **Query complexity:** Nested multi-query combining uses Elasticsearch `bool.should/must`; 10 query limit balances performance vs flexibility
- **Lineage resolution:** Two-stage process (taxon ID resolution + lineage extraction) adds ~5-10ms per lineage query
- **Result size:** searchBatch respects `size` parameter; use `size: 0` in countBatch for count-only operations

**Recommended patterns:**

- For high-volume counting → use countBatch with `size: 0`
- For large result sets → use smaller batch sizes (10-20 searches) to avoid timeouts
- For dynamic queries → combine batch with application-level result aggregation

---

## SDK Integration

### Python

```python
from genomehubs.query import QueryBuilder

qb = QueryBuilder(api_base="http://localhost:3000")

# Single query batch count
result = qb.count_batch([
    qb.taxon_tree(["Mammalia"]),
    qb.taxon_tree(["Aves"])
])

# Multi-query OR combining
result = qb.count_batch([
    qb.query_batch_combine(
        queries=[
            {"taxa": ["Canis lupus"], "taxon_filter_type": "name"},
            {"taxa": ["Felis"], "taxon_filter_type": "name"}
        ],
        combine_with="OR"
    )
])
```

### JavaScript

```javascript
const qb = new QueryBuilder({ apiBase: "http://localhost:3000" });

// Batch count
const result = await qb.countBatch([
  qb.taxonTree(["Mammalia"]),
  qb.taxonTree(["Aves"]),
]);

// Multi-query OR combining
const result = await qb.countBatch([
  qb.queryBatchCombine({
    queries: [
      { taxa: ["Canis lupus"], taxon_filter_type: "name" },
      { taxa: ["Felis"], taxon_filter_type: "name" },
    ],
    combine_with: "OR",
  }),
]);
```

### R

```r
library(genomehubs)

qb <- QueryBuilder$new(api_base = "http://localhost:3000")

# Batch count
result <- qb$count_batch(list(
  qb$taxon_tree(c("Mammalia")),
  qb$taxon_tree(c("Aves"))
))

# Multi-query OR combining
result <- qb$count_batch(list(
  qb$query_batch_combine(
    queries = list(
      list(taxa = c("Canis lupus"), taxon_filter_type = "name"),
      list(taxa = c("Felis"), taxon_filter_type = "name")
    ),
    combine_with = "OR"
  )
))
```

---

## Related Resources

- [Phase 3a Implementation](../planning/phases/phase-3-sdk-coverage.md) — Design decisions and scope
- [Examples: Batch Queries](../../examples/QUERY-EXAMPLES.md) — Curl examples
- [API Status Endpoint](../planning/v3-api-parity-plan.md) — Version detection
- [Integration Tests](../../tests/api_endpoints.rs) — Test patterns and validation
