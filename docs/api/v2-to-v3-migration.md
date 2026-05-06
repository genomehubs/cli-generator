# V2 to V3 API Migration Guide

**Date:** May 5, 2026
**Scope:** API endpoint patterns and query structure changes
**Target Audience:** Developers migrating from Goat v2 API to v3 API

---

## Quick Reference: Endpoint Changes

| V2 Endpoint               | V3 Equivalent                   | Notes                                           |
| ------------------------- | ------------------------------- | ----------------------------------------------- |
| `/search`                 | `/search`                       | Similar; v3 requires YAML; v2 supported strings |
| `/count`                  | `/count`                        | Similar; v3 requires YAML                       |
| `/msearch` (if available) | `/countBatch` or `/searchBatch` | Batch operations now split into count/search    |
| `/record`                 | `/record`                       | Unchanged behavior; v3 YAML format              |
| `/lookup`                 | `/lookup`                       | Unchanged behavior; v3 YAML format              |
| (none)                    | `/searchBatch`                  | NEW: Batch search with document results         |
| (none)                    | `/countBatch`                   | NEW: Batch count operations                     |

---

## Query String Format Changes

### V2 API: String-Based Queries

V2 accepted query strings with operators parsed by the API:

```bash
# V2: String parsing by API
curl "http://api.v2/search?query=tax_name(wolf)&fields=lineage,gc"
curl "http://api.v2/search?query=tax_name(wolf) OR tax_name(strawberry)&fields=lineage"
curl "http://api.v2/count?query=genome_size:>1G"
```

**Key features:**

- Query string is parsed server-side
- OR/AND combining is inline in the string
- Operators embedded in string: `tax_name()`, `genome_size:>`, etc.

### V3 API: YAML-Based Queries

V3 requires structured YAML format (or equivalent JSON):

```bash
# V3: Structured YAML in POST body
curl -X POST http://api.v3/search \
  -H "Content-Type: application/json" \
  -d '{
    "query_yaml": "index: taxon\ntaxa: [wolf]",
    "params_yaml": "fields: [lineage, gc]"
  }'

# V3: Multi-query OR combining
curl -X POST http://api.v3/searchBatch \
  -H "Content-Type: application/json" \
  -d '{
    "searches": [{
      "query_yaml": "index: taxon\ncombine_with: OR\nqueries:\n  - taxa: [wolf]\n    taxon_filter_type: name\n  - taxa: [strawberry]\n    taxon_filter_type: name",
      "params_yaml": "fields: [lineage, gc]"
    }]
  }'

# V3: Attribute operators
curl -X POST http://api.v3/count \
  -H "Content-Type: application/json" \
  -d '{
    "query_yaml": "index: assembly\nattributes:\n  - name: genome_size\n    operator: gt\n    value: \"1G\"",
    "params_yaml": ""
  }'
```

**Key features:**

- Query is YAML-structured (more explicit)
- No server-side string parsing (Phase 9 planned to add it to non-batch endpoints)
- Operators are explicit fields (`operator: gt` instead of `:>`)
- Multi-query combining requires separate query object structure

---

## Filter Type Mapping

### V2 Implicit Behavior

V2 API automatically inferred filter behavior:

```
tax_name(Canis lupus) → exact match
tax_tree(Mammalia) → descendants
tax_id(9646) → ID lookup
```

### V3 Explicit Filter Types

V3 requires explicit `taxon_filter_type`:

```yaml
# Exact name match
taxa: [Canis lupus]
taxon_filter_type: name

# Tree (descendants)
taxa: [Mammalia]
taxon_filter_type: tree

# Lineage (ancestor matching)
taxa: [Chordata]
taxon_filter_type: lineage
```

**Migration path:**

- V2 `tax_name()` → V3 `taxon_filter_type: name`
- V2 `tax_tree()` → V3 `taxon_filter_type: tree`
- V2 `tax_id()` → V3 `taxon_filter_type: name` + numeric value

---

## Endpoint-by-Endpoint Migration

### `/search` → `/search`

**V2:**

```bash
curl "http://api.v2/search?query=tax_name(Canis%20lupus)&fields=lineage,gc&limit=10"
```

**V3 equivalent:**

```bash
curl -X POST http://api.v3/search \
  -H "Content-Type: application/json" \
  -d '{
    "query_yaml": "index: taxon\ntaxa: [\"Canis lupus\"]\ntaxon_filter_type: name",
    "params_yaml": "fields: [lineage, gc]\nsize: 10"
  }'
```

**Changes:**

- HTTP method: GET → POST (YAML in body)
- Query parameter → `query_yaml` in request body
- Field list: inline → `params_yaml: fields: [...]`
- Limit: `limit` → `params_yaml: size`

### `/count` → `/count`

**V2:**

```bash
curl "http://api.v2/count?query=tax_name(Mammalia)"
```

**V3 equivalent:**

```bash
curl -X POST http://api.v3/count \
  -H "Content-Type: application/json" \
  -d '{
    "query_yaml": "index: taxon\ntaxa: [Mammalia]\ntaxon_filter_type: name",
    "params_yaml": ""
  }'
```

### `/msearch` → `/countBatch` or `/searchBatch`

**V2 (hypothetical msearch):**

```bash
curl -X POST http://api.v2/msearch \
  -H "Content-Type: application/json" \
  -d '[
    { "query": "tax_name(Mammalia)" },
    { "query": "tax_name(Aves)" }
  ]'
```

**V3: Count only (optimized)**

```bash
curl -X POST http://api.v3/countBatch \
  -H "Content-Type: application/json" \
  -d '{
    "searches": [
      { "query_yaml": "index: taxon\ntaxa: [Mammalia]\ntaxon_filter_type: name", "params_yaml": "size: 0" },
      { "query_yaml": "index: taxon\ntaxa: [Aves]\ntaxon_filter_type: name", "params_yaml": "size: 0" }
    ]
  }'
```

**V3: With document results**

```bash
curl -X POST http://api.v3/searchBatch \
  -H "Content-Type: application/json" \
  -d '{
    "searches": [
      { "query_yaml": "index: taxon\ntaxa: [Mammalia]\ntaxon_filter_type: name", "params_yaml": "size: 10" },
      { "query_yaml": "index: taxon\ntaxa: [Aves]\ntaxon_filter_type: name", "params_yaml": "size: 10" }
    ]
  }'
```

**Key differences:**

- Request structure: JSON array → object with `searches` key
- Query format: string → YAML
- Batch operations split: `countBatch` (count only, `size: 0`) vs `searchBatch` (with results)

### `/record` → `/record`

**V2:**

```bash
curl "http://api.v2/record?query=9646"
```

**V3 equivalent:**

```bash
curl -X POST http://api.v3/record \
  -H "Content-Type: application/json" \
  -d '{
    "query_yaml": "index: taxon\ntaxa: [9646]\ntaxon_filter_type: name",
    "params_yaml": ""
  }'
```

### `/lookup` → `/lookup`

**V2:**

```bash
curl "http://api.v2/lookup?query=custom_id:123&fields=alternative_ids"
```

**V3 equivalent:**

```bash
curl -X POST http://api.v3/lookup \
  -H "Content-Type: application/json" \
  -d '{
    "query_yaml": "index: taxon\nidentifiers:\n  - name: custom_id\n    operator: exact\n    value: 123",
    "params_yaml": "fields: [alternative_ids]"
  }'
```

---

## Multi-Query Combining: V2 vs V3

### V2: String-Based OR/AND

V2 supported inline OR/AND combining in query strings:

```bash
# V2: String parsing
curl "http://api.v2/count?query=tax_name(wolf) OR tax_name(strawberry)"
curl "http://api.v2/count?query=genome_size:>1G AND gc:>0.5"
```

### V3: Structured Multi-Query (Phase 3a.1+)

V3 batch endpoints support explicit multi-query combining:

**OR combining:**

```yaml
# V3 YAML structure for OR
index: taxon
combine_with: OR
queries:
  - taxa: [wolf]
    taxon_filter_type: name
  - taxa: [strawberry]
    taxon_filter_type: name
```

**AND combining:**

```yaml
# V3 YAML structure for AND
index: assembly
combine_with: AND
queries:
  - attributes:
      - name: genome_size
        operator: gt
        value: "1G"
  - attributes:
      - name: gc
        operator: gt
        value: "0.5"
```

**Benefits:**

- Explicit combining strategy (no ambiguity)
- Supports up to 10 queries per combine operation
- Constraint validation (max 10 queries, same index required)

**Phase 9 Enhancement** (planned): Non-batch `/search` and `/count` endpoints will support string parsing like v2, converting `tax_name(wolf) OR tax_name(strawberry)` automatically to the v3 YAML multi-query structure.

---

## Attribute/Field Operators

### V2 String Syntax

V2 used shorthand operators in query strings:

```
genome_size:>1G      → greater than
genome_size:<3G      → less than
gc:0.5               → equal to
species_count:100+   → range
```

### V3 Structured Operators

V3 uses explicit operator fields:

```yaml
attributes:
  - name: genome_size
    operator: gt # gt, gte, lt, lte, eq
    value: "1G"
  - name: gc
    operator: eq
    value: "0.5"
  - name: species_count
    operator: gte
    value: "100"
```

**V2 → V3 operator mapping:**

| V2 Syntax                       | V3 Operator | Example                                                 |
| ------------------------------- | ----------- | ------------------------------------------------------- |
| `field:>value`                  | `gt`        | `genome_size: >1G` → `operator: gt, value: "1G"`        |
| `field:<value`                  | `lt`        | `genome_size: <3G` → `operator: lt, value: "3G"`        |
| `field:=value` or `field:value` | `eq`        | `gc: 0.5` → `operator: eq, value: "0.5"`                |
| `field:value+`                  | `gte`       | `species_count: 100+` → `operator: gte, value: "100"`   |
| `field:value-`                  | `lte`       | `species_count: 1000-` → `operator: lte, value: "1000"` |

---

## Response Format Changes

### V2 Response

```json
{
  "status": {
    "success": true,
    "took": 45
  },
  "count": 150000,
  "hits": [
    {
      "taxon_id": 9646,
      "scientific_name": "Canis lupus",
      "rank": "species"
    }
  ]
}
```

### V3 Response

```json
{
  "status": {
    "success": true,
    "hits": 150000,
    "took": 45
  },
  "hits": [
    {
      "taxon_id": 9646,
      "scientific_name": "Canis lupus",
      "rank": "species"
    }
  ]
}
```

**Changes:**

- V2 had separate `count` field; V3 uses `status.hits` for hit count
- Response envelope structure is now consistent across endpoints
- Batch endpoints return array of result objects with per-query status

---

## SDK Migration (Python/JavaScript/R)

### Python Example

**V2:**

```python
from genomehubs import GenomeHubsClient

client = GenomeHubsClient(api_url="http://api.v2")

# V2: String-based queries
result = client.search(query="tax_name(Canis lupus)", limit=10)
count = client.count(query="tax_tree(Mammalia)")
```

**V3:**

```python
from genomehubs.query import QueryBuilder

qb = QueryBuilder(api_base="http://api.v3")

# V3: Structured queries
result = qb.search(
    taxa=["Canis lupus"],
    taxon_filter_type="name",
    size=10
).run()

count = qb.count(
    taxa=["Mammalia"],
    taxon_filter_type="tree"
).run()

# V3: Batch operations
batch_results = qb.search_batch([
    qb.count(taxa=["Mammalia"]),
    qb.count(taxa=["Aves"])
]).run()
```

### JavaScript Example

**V2:**

```javascript
const client = new GenomeHubsClient({ apiUrl: "http://api.v2" });

// V2: String-based
const result = await client.search({
  query: "tax_name(Canis lupus)",
  limit: 10,
});
const count = await client.count({ query: "tax_tree(Mammalia)" });
```

**V3:**

```javascript
const qb = new QueryBuilder({ apiBase: "http://api.v3" });

// V3: Structured
const result = await qb.taxonTree(["Canis lupus"]).search({ size: 10 });
const count = await qb.taxonTree(["Mammalia"]).count();

// V3: Batch
const batchResults = await qb.countBatch([
  qb.taxonTree(["Mammalia"]),
  qb.taxonTree(["Aves"]),
]);
```

---

## Common Migration Patterns

### Pattern 1: Simple Counts

**V2:**

```bash
curl "http://api.v2/count?query=tax_tree(Mammalia)"
# Result: 150000
```

**V3:**

```bash
curl -X POST http://api.v3/count \
  -H "Content-Type: application/json" \
  -d '{
    "query_yaml": "index: taxon\ntaxa: [Mammalia]\ntaxon_filter_type: tree",
    "params_yaml": ""
  }'
# Result: { "status": { "success": true, "hits": 150000, "took": 45 } }
```

### Pattern 2: Batch Counting (Multiple Queries)

**V2:**

```bash
# Individual requests (inefficient)
curl "http://api.v2/count?query=tax_tree(Mammalia)"
curl "http://api.v2/count?query=tax_tree(Aves)"
```

**V3:**

```bash
# One batch request (efficient)
curl -X POST http://api.v3/countBatch \
  -H "Content-Type: application/json" \
  -d '{
    "searches": [
      { "query_yaml": "index: taxon\ntaxa: [Mammalia]\ntaxon_filter_type: tree", "params_yaml": "size: 0" },
      { "query_yaml": "index: taxon\ntaxa: [Aves]\ntaxon_filter_type: tree", "params_yaml": "size: 0" }
    ]
  }'
```

### Pattern 3: OR Combining

**V2:**

```bash
curl "http://api.v2/count?query=tax_name(wolf) OR tax_name(strawberry)"
```

**V3:**

```bash
curl -X POST http://api.v3/countBatch \
  -H "Content-Type: application/json" \
  -d '{
    "searches": [{
      "query_yaml": "index: taxon\ncombine_with: OR\nqueries:\n  - taxa: [wolf]\n    taxon_filter_type: name\n  - taxa: [strawberry]\n    taxon_filter_type: name",
      "params_yaml": "size: 0"
    }]
  }'
```

---

## Backward Compatibility Notes

**V2 query strings in V3:**

- V3 does NOT support direct string parsing in batch endpoints (design choice for explicitness)
- Phase 9 planned: Non-batch `/search` and `/count` will support string parsing for backward compatibility
- Applications should migrate to YAML structure or add a client-side string parser

**Deprecation timeline:**

- V2 API: Supported indefinitely (separate codebase)
- V3 migration: Recommended for new applications (more explicit, better performance)

---

## Checklist: Migrating Your Application

- [ ] Identify all V2 API calls (check code for API URLs)
- [ ] Convert query strings to V3 YAML format
- [ ] Map operator syntax: `:>` → `operator: gt`, etc.
- [ ] Update HTTP method: GET → POST (for structured queries)
- [ ] Change response parsing to use `status.hits` instead of `count`
- [ ] For batch operations: group queries into `/countBatch` (counts) or `/searchBatch` (results)
- [ ] Test against v3 API endpoint
- [ ] Update SDK calls if using Python/JavaScript/R client libraries
- [ ] Update documentation and examples

---

## Getting Help

- **Examples:** See [examples/QUERY-EXAMPLES.md](../../examples/QUERY-EXAMPLES.md) for curl examples
- **API Reference:** See [docs/api/batch-endpoints.md](batch-endpoints.md) for endpoint details
- **Phase 3 Planning:** [docs/planning/phases/phase-3-sdk-coverage.md](../planning/phases/phase-3-sdk-coverage.md)
- **Integration Tests:** [tests/api_endpoints.rs](../../tests/api_endpoints.rs) for test patterns
