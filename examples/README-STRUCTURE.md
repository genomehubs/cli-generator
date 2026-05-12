# Examples Directory Structure

Organized example queries for all API endpoints.

## Directory Layout

```
examples/
├── batch/                       # Batch query examples
│   ├── query-batch-count-*.yaml
│   ├── query-batch-search.*
│   └── (YAML format for API batch endpoints)
│
├── query/                       # Single & multi-query examples
│   ├── query-single.*
│   ├── query-multi-or.*
│   ├── query-multi-and.*
│   └── (YAML/JSON format for search & count endpoints)
│
├── report/                      # Report endpoint examples (JSON)
│   ├── histogram-simple.json
│   ├── histogram-categorized.json
│   ├── scatter.json
│   ├── countPerRank.json
│   ├── sources.json
│   ├── tree.json
│   └── map.json
│
├── test-queries.sh             # Test search/count/batch endpoints
├── test-report-queries.sh      # Test report endpoint
├── REPORT-TESTING.md           # Report testing guide
└── README-STRUCTURE.md         # This file
```

## Quick Start

### Test Reports (POST /api/v3/report)

```bash
# Run all report tests
bash examples/test-report-queries.sh

# Or test individual report
curl -X POST http://localhost:3000/api/v3/report \
  -H "Content-Type: application/json" \
  -d @examples/report/histogram-simple.json
```

See [REPORT-TESTING.md](REPORT-TESTING.md) for detailed guide.

### Test Query Endpoints

```bash
# Run all query tests (search, count/batch, search/batch)
bash examples/test-queries.sh

# Or test single batch query
curl -X POST http://localhost:3000/api/v3/count/batch \
  -H "Content-Type: application/yaml" \
  -d @examples/batch/query-batch-count-single.yaml
```

## JSON Format Benefits

- ✅ No inline YAML escaping issues
- ✅ Easy to edit with any text editor
- ✅ Clear structure and validation
- ✅ Better for version control
- ✅ Easy to test variations

## File Organization Rationale

**Before:** Mixed YAML and JSON files with inline query strings in scripts
**After:** Structured JSON examples by endpoint type

Benefits:

1. **Batch examples** isolated in `batch/` for easy browsing
2. **Report examples** organized in `report/` for quick reference
3. **No inline escaping** needed in curl commands
4. **Reusable files** - edit JSON once, test multiple times
5. **Cleaner scripts** - `@filename` instead of string interpolation

## Adding New Examples

To add a new report variant:

```bash
# Copy and modify an existing example
cp examples/report/histogram-simple.json examples/report/my-variant.json

# Edit the new file
nano examples/report/my-variant.json

# Test it
curl -X POST http://localhost:3000/api/v3/report \
  -H "Content-Type: application/json" \
  -d @examples/report/my-variant.json | jq '.'
```

## See Also

- [REPORT-TESTING.md](REPORT-TESTING.md) - Report endpoint testing guide
- [QUERY-EXAMPLES.md](QUERY-EXAMPLES.md) - Legacy query format reference (if available)
