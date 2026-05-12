#!/bin/bash
# Test script for search, count/batch endpoints with multi-query OR/AND combining
# Run the API server first: cargo run -p genomehubs-api
# Then run this script: bash examples/test-queries.sh

set -e

API="http://localhost:3000/api/v3"

echo "=== Single Query Search ==="
echo "Searching for Mammalia..."
curl -s -X POST "$API/search" \
  -H "Content-Type: application/json" \
  -d '{
    "query_yaml": "'"'$(sed 's/$/\\n/' examples/query/query-single.yaml | tr -d '\n')'"'",
    "size": 5
  }' | jq '.status, (.results | length)'

echo ""
echo "=== Multi-Query OR Search ==="
echo "Searching for Mammalia OR Aves..."
curl -s -X POST "$API/search" \
  -H "Content-Type: application/json" \
  -d '{
    "query_yaml": "'"'$(sed 's/$/\\n/' examples/query/query-multi-or.yaml | tr -d '\n')'"'",
    "size": 5
  }' | jq '.status, (.results | length)'

echo ""
echo "=== Multi-Query AND Search ==="
echo "Searching for Mammalia AND (NOT Felis)..."
curl -s -X POST "$API/search" \
  -H "Content-Type: application/json" \
  -d '{
    "query_yaml": "'"'$(sed 's/$/\\n/' examples/query/query-multi-and.yaml | tr -d '\n')'"'",
    "size": 5
  }' | jq '.status, (.results | length)'

echo ""
echo "=== Count Single Queries ==="
echo "Counting Mammalia, Aves, Reptilia separately..."
curl -s -X POST "$API/count/batch" \
  -H "Content-Type: application/yaml" \
  -d @examples/batch/query-batch-count-single.yaml | jq '.results[] | {taxa: .hits}'

echo ""
echo "=== Count Multi-Query Combining ==="
echo "Counting: (Mammalia OR Aves) total, Mammalia alone, Aves alone..."
curl -s -X POST "$API/count/batch" \
  -H "Content-Type: application/yaml" \
  -d @examples/batch/query-batch-count-multi.yaml | jq '.results[] | {count: .count}'

echo ""
echo "=== SearchBatch (Parallel Independent Searches) ==="
echo "Searching for Mammalia, Aves, Reptilia in parallel..."
# For search/batch, wrap each query YAML in a query_yaml field
curl -s -X POST "$API/search/batch" \
  -H "Content-Type: application/json" \
  -d '{
    "queries": [
      {
        "query_yaml": "index: taxon\ntaxa: [Mammalia]\nrank: species",
        "size": 3
      },
      {
        "query_yaml": "index: taxon\ntaxa: [Aves]\nrank: species",
        "size": 3
      },
      {
        "query_yaml": "index: taxon\ntaxa: [Reptilia]\nrank: species",
        "size": 3
      }
    ]
  }' | jq '.results[] | {count: .count, status: .status}'

echo ""
echo "Done!"
