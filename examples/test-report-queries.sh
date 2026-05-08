#!/bin/bash
# Test script for new report endpoint using JSON request files
# Run the API server first: cargo run -p genomehubs-api
# Then run this script: bash examples/test-report-queries.sh

set -e

API="http://localhost:3000/api/v3"

echo "=== Testing Report Endpoint ==="
echo ""

# ============================================================================
echo "1. Histogram Report (genome_size, 20 bins, log10 scale)"
echo "   Testing basic 1D histogram report"
echo ""

curl -s -X POST "$API/report" \
  -H "Content-Type: application/json" \
  -d @examples/report/histogram-simple.json | jq '{
    status: .status | {success, hits, took},
    report_type: .report.type,
    x_field: .report.x.field,
    x_scale: .report.x.scale,
    bucket_count: (.report.buckets | length),
    first_bucket: .report.buckets[0]
  }'

echo ""
echo "---"
echo ""

# ============================================================================
echo "2. Categorised Histogram (genome_size by assembly_level)"
echo "   Testing nested histogram + category breakdown"
echo ""

curl -s -X POST "$API/report" \
  -H "Content-Type: application/json" \
  -d @examples/report/histogram-categorized.json | jq '{
    status: .status | {success, hits, took},
    report_type: .report.type,
    has_categories: (.report.buckets[0] | has("cat_counts")),
    bucket_count: (.report.buckets | length),
    first_bucket: .report.buckets[0]
  }'

echo ""
echo "---"
echo ""

# ============================================================================
echo "3. Scatter Report (raw mode, threshold 10000)"
echo "   Testing scatter report with threshold"
echo ""

curl -s -X POST "$API/report" \
  -H "Content-Type: application/json" \
  -d @examples/report/scatter.json | jq '{
    status: .status | {success, hits, took},
    report_type: .report.type,
    x_field: .report.x.field,
    bucket_count: (.report.buckets | length)
  }'

echo ""
echo "---"
echo ""

# ============================================================================
echo "4. countPerRank Report (counts per taxon_rank)"
echo "   Testing counts per taxonomic rank"
echo ""

curl -s -X POST "$API/report" \
  -H "Content-Type: application/json" \
  -d @examples/report/countPerRank.json | jq '{
    status: .status | {success, hits, took},
    report_type: .report.type,
    bucket_count: (.report.buckets | length),
    first_bucket: .report.buckets[0]
  }'

echo ""
echo "---"
echo ""

# ============================================================================
echo "5. Sources Report (top sources by count)"
echo "   Testing sources aggregation"
echo ""

curl -s -X POST "$API/report" \
  -H "Content-Type: application/json" \
  -d @examples/report/sources.json | jq '{
    status: .status | {success, hits, took},
    report_type: .report.type,
    bucket_count: (.report.buckets | length),
    first_bucket: .report.buckets[0]
  }'

echo ""
echo "---"
echo ""

# ============================================================================
echo "6. Tree Report (taxonomy tree, phylum rank)"
echo "   Testing Newick serialization"
echo ""

curl -s -X POST "$API/report" \
  -H "Content-Type: application/json" \
  -d @examples/report/tree.json | jq '{
    status: .status | {success, hits, took},
    report_type: .report.type,
    newick: .report.newick,
    bucket_count: (.report.buckets | length)
  }'

echo ""
echo "---"
echo ""

# ============================================================================
echo "7. Map Report (geohash grid)"
echo "   Testing geohash aggregation"
echo ""

curl -s -X POST "$API/report" \
  -H "Content-Type: application/json" \
  -d @examples/report/map.json | jq '{
    status: .status | {success, hits, took},
    report_type: .report.type,
    geo_field: .report.field,
    bucket_count: (.report.buckets | length),
    first_bucket: .report.buckets[0]
  }'

echo ""
echo "---"
echo ""

# ============================================================================
echo "=== All Report Tests Complete ==="
echo ""
echo "Report request files in examples/report/:"
ls -1 examples/report/*.json | sed 's/^/  /'
echo ""
echo "Notes:"
echo "  - Some report types may fail if the ES index doesn't have the required fields"
echo "  - Adjust taxa, ranks, and field names to match your test index"
echo "  - Use 'jq .' without arguments to see full responses for debugging"
