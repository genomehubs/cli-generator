# Report API Testing Guide

Quick start for testing the new `POST /api/v3/report` endpoint.

## Directory Structure

```
examples/
├── batch/                          # Batch query examples
│   ├── count-single.json
│   ├── count-multi.json
│   ├── search.json
│   └── ...
├── report/                         # Report endpoint examples (JSON files)
│   ├── histogram-simple.json
│   ├── histogram-categorized.json
│   ├── scatter.json
│   ├── countPerRank.json
│   ├── sources.json
│   ├── tree.json
│   └── map.json
├── test-report-queries.sh
└── REPORT-TESTING.md (this file)
```

All report request examples are in JSON format for easy editing with any text editor.

## Setup

### 1. Start the API server

```bash
cd /Users/rchallis/projects/genomehubs/cli-generator
cargo run -p genomehubs-api
```

Server runs on `http://localhost:3000`

### 2. Run the test script

In a new terminal:

```bash
bash examples/test-report-queries.sh
```

This runs all 7 report type tests and shows formatted results.

---

## Quick Testing

### Using the test script

```bash
bash examples/test-report-queries.sh
```

### Test a single report type

```bash
curl -X POST http://localhost:3000/api/v3/report \
  -H "Content-Type: application/json" \
  -d @examples/report/histogram-simple.json | jq '.'
```

### Edit and re-test

```bash
# Edit the JSON file
nano examples/report/histogram-simple.json

# Test again
curl -X POST http://localhost:3000/api/v3/report \
  -H "Content-Type: application/json" \
  -d @examples/report/histogram-simple.json | jq '.report'
```

---

## Report Types

### 1. Histogram (Simple)

**File:** `examples/report/histogram-simple.json`

```json
{
  "query": {
    "index": "taxon",
    "taxa": ["Mammalia"]
  },
  "params": {
    "taxonomy": "ncbi"
  },
  "report": {
    "report": "histogram",
    "x": "genome_size",
    "x_opts": ";;20;log10"
  }
}
```

**Test:**

```bash
curl -X POST http://localhost:3000/api/v3/report \
  -H "Content-Type: application/json" \
  -d @examples/report/histogram-simple.json | \
  jq '.report | {type, x_field: .x.field, buckets: (.buckets | length)}'
```

---

### 2. Histogram with Categories

**File:** `examples/report/histogram-categorized.json`

```json
{
  "query": {
    "index": "taxon",
    "taxa": ["Mammalia"]
  },
  "params": {
    "taxonomy": "ncbi"
  },
  "report": {
    "report": "histogram",
    "x": "genome_size",
    "x_opts": ";;10;linear",
    "cat": "assembly_level",
    "cat_opts": ";;5+"
  }
}
```

**Test:**

```bash
curl -X POST http://localhost:3000/api/v3/report \
  -H "Content-Type: application/json" \
  -d @examples/report/histogram-categorized.json | \
  jq '.report | {type, x_field: .x.field, has_categories: (.buckets[0] | has("cat_counts"))}'
```

**AxisOpts syntax:** `min;max;size;scale`

- `;;20;log10` → auto min/max, 20 bins, log10 scale
- `;;10;linear` → auto min/max, 10 bins, linear scale
- `1e6;1e12;50;log10` → 1M-1T range, 50 bins, log10 scale

---

### 3. Scatter

**File:** `examples/report/scatter.json`

```json
{
  "query": {
    "index": "taxon",
    "taxa": ["Mammalia"]
  },
  "params": {
    "taxonomy": "ncbi"
  },
  "report": {
    "report": "scatter",
    "x": "genome_size",
    "scatter_threshold": 10000
  }
}
```

Returns raw documents if count < threshold; grid aggregation otherwise.

---

### 4. countPerRank (Per Taxonomic Rank)

**File:** `examples/report/countPerRank.json`

```json
{
  "query": {
    "index": "taxon",
    "taxa": ["Mammalia"]
  },
  "params": {
    "taxonomy": "ncbi"
  },
  "report": {
    "report": "countPerRank",
    "x": "genome_size"
  }
}
```

Shows min/max/stats for a field per taxonomic rank.

---

### 5. Sources

**File:** `examples/report/sources.json`

```json
{
  "query": {
    "index": "taxon",
    "taxa": ["Mammalia"]
  },
  "params": {
    "taxonomy": "ncbi"
  },
  "report": {
    "report": "sources"
  }
}
```

Returns top data sources by document count.

---

### 6. Tree (Newick Format)

**File:** `examples/report/tree.json`

```json
{
  "query": {
    "index": "taxon",
    "taxa": ["Mammalia"]
  },
  "params": {
    "taxonomy": "ncbi"
  },
  "report": {
    "report": "tree",
    "rank": "phylum",
    "depth": 5
  }
}
```

Hierarchical taxonomy serialized to Newick format: `(A:count,B:count);`

---

### 7. Map (Geohash Grid)

**File:** `examples/report/map.json`

```json
{
  "query": {
    "index": "taxon",
    "taxa": ["Mammalia"]
  },
  "params": {
    "taxonomy": "ncbi"
  },
  "report": {
    "report": "map",
    "x": "location",
    "precision": 4
  }
}
```

Geohash precision levels:

- 1: country
- 4: city
- 6: neighborhood
- 8: block

---

## Response Format

All reports return:

```json
{
  "status": {
    "success": true,
    "hits": 12345,
    "took": 42
  },
  "report": {
    "type": "histogram",
    "x": { "field": "genome_size", "scale": "log10", "domain": [...] },
    "buckets": [...]
  }
}
```

---

## Troubleshooting

### "field not found" error

- Verify the field exists in your ES index
- Use standard fields: `genome_size`, `assembly_level`, `taxon_rank`

### Empty results

- The query builder is stubbed (returns match-all); filter works but is simplified
- Try with specific taxa: `taxa: [Mammalia]`

### Slow computation

- Reduce bin count: `;;5` instead of `;;100`
- Filter to fewer records

### Port 3000 in use

```bash
pkill -9 genomehubs-api
```

---

## Editing Examples

Edit any JSON file to test variations:

```bash
# Open in your editor
nano examples/report/histogram-simple.json

# Change x_opts to test different scales:
# ";;20;log10"  → logarithmic
# ";;20;linear" → linear
# ";;50;sqrt"   → square root

# Save and test
curl -X POST http://localhost:3000/api/v3/report \
  -H "Content-Type: application/json" \
  -d @examples/report/histogram-simple.json | jq '.report.x.scale'
```

---

## Next: Phase 6b

After testing confirms the API works:

1. Add `ReportBuilder` SDK type
2. Add parse functions (`parse_histogram_json`, `parse_tree_json`, etc.)
3. Add language SDK methods (Python, R, JS)
4. Integration tests

See [../agent-logs/2026-05-06_001_phase-6a-report-api-core.md](../agent-logs/2026-05-06_001_phase-6a-report-api-core.md) for details.
