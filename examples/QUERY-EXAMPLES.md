# Query Examples - Both Formats Supported

All endpoints support **two request formats**:

1. **YAML strings** (programmatic use, easy to process)
2. **JSON objects** (direct curl, more readable)

Both are converted internally, so use whichever is more convenient.

## Start the API

```bash
cargo run -p genomehubs-api
```

## Single Query Search

**Option 1: JSON objects (recommended for curl)**

```bash
curl -X POST http://localhost:3000/api/v3/search \
  -H "Content-Type: application/json" \
  -d @examples/query-single-json-objects.json | jq
```

**Option 2: YAML strings**

```bash
curl -X POST http://localhost:3000/api/v3/search \
  -H "Content-Type: application/json" \
  -d @examples/query-single.json | jq
```

## Multi-Query OR Search (Mammalia OR Aves)

Returns combined results for both groups:

**Option 1: JSON objects (recommended for curl)**

```bash
curl -X POST http://localhost:3000/api/v3/search \
  -H "Content-Type: application/json" \
  -d @examples/query-multi-or-json-objects.json | jq
```

**Option 2: YAML strings**

```bash
curl -X POST http://localhost:3000/api/v3/search \
  -H "Content-Type: application/json" \
  -d @examples/query-multi-or.json | jq
```

## Count Batch: Total vs Individual Counts

**Note: Field is `searches`, items use `query`/`params` (JSON objects) or `query_yaml`/`params_yaml` (YAML strings)**

**Option 1: JSON objects (recommended for curl)**

```bash
curl -X POST http://localhost:3000/api/v3/countBatch \
  -H "Content-Type: application/json" \
  -d @examples/query-batch-count-multi-json-objects.json | jq '.results[] | {count: .count}'
```

**Option 2: YAML strings**

```bash
curl -X POST http://localhost:3000/api/v3/countBatch \
  -H "Content-Type: application/json" \
  -d @examples/query-batch-count-multi.json | jq '.results[] | {count: .count}'
```

Expected output (example):

```json
{"count": 150000}   # Total: Mammalia OR Aves
{"count": 80000}    # Mammalia
{"count": 70000}    # Aves
```

## SearchBatch (Parallel Independent Searches)

Returns separate result arrays:

```bash
curl -X POST http://localhost:3000/api/v3/searchBatch \
  -H "Content-Type: application/json" \
  -d @examples/query-batch-search.json | jq '.results[] | {count: .count}'
```

Expected output (example):

```json
{"count": 80000}    # Mammalia results
{"count": 70000}    # Aves results
{"count": 45000}    # Reptilia results
```

## Key Differences

| Feature               | `/search`          | `/countBatch`                   | `/searchBatch`                         |
| --------------------- | ------------------ | ------------------------------- | -------------------------------------- |
| Multi-query combining | ✅ OR/AND          | ✅ OR/AND per item              | ❌ Each separate                       |
| Result combining      | ✅ Single ES query | ✅ Supports both total & unique | ❌ Independent                         |
| Use case              | "Mammalia OR Aves" | "Total (A OR B) vs A vs B"      | Parallel searches for different groups |
| ES execution          | 1 query            | \_msearch (N queries)           | \_msearch (N queries)                  |

## Request Format Options

Both query formats are supported everywhere:

```json
{
  "query": { "index": "taxon", "taxa": ["Mammalia"] },
  "params": { "size": 10 }
}
```

OR

```json
{
  "query_yaml": "index: taxon\ntaxa: [Mammalia]",
  "params_yaml": "size: 10"
}
```

For batch endpoints, use `queries` instead of `query`:

```json
{
  "searches": [
    {
      "query": { "index": "taxon", "taxa": ["Mammalia"] },
      "params": { "size": 0 }
    },
    { "query": { "index": "taxon", "taxa": ["Aves"] }, "params": { "size": 0 } }
  ]
}
```
