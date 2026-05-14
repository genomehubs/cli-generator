#!/usr/bin/env bash
# Extract v1 feature index fixture data from a live Elasticsearch instance.
#
# Usage:
#   scripts/extract_feature_fixtures.sh <assembly_id> <output_dir> [es_base]
#
# Example:
#   scripts/extract_feature_fixtures.sh GCA_905147045.1 tests/fixtures/
#
# Outputs:
#   <output_dir>/feature_v1_<assembly_id>.json   raw ES hits (toplevel + busco-gene)
#
# Requires: curl, jq

set -euo pipefail

ASSEMBLY_ID="${1:?Usage: $0 <assembly_id> <output_dir> [es_base]}"
OUTPUT_DIR="${2:?Usage: $0 <assembly_id> <output_dir> [es_base]}"
ES_BASE="${3:-http://localhost:9200}"

mkdir -p "$OUTPUT_DIR"

SAFE_ASSEMBLY="${ASSEMBLY_ID//./_}"
OUTFILE="${OUTPUT_DIR}/feature_v1_${SAFE_ASSEMBLY}.json"

# Discover the feature index name
echo "Discovering feature index..." >&2
FEATURE_INDEX=$(curl -s "${ES_BASE}/_cat/indices/feature--*?h=index" \
    | sort | tail -1 | tr -d '[:space:]')

if [[ -z "$FEATURE_INDEX" ]]; then
    echo "Error: no feature--* index found at ${ES_BASE}" >&2
    exit 1
fi
echo "Using index: ${FEATURE_INDEX}" >&2

# Fetch toplevel sequence docs (up to 20)
echo "Fetching toplevel sequences..." >&2
TOPLEVEL=$(curl -s -X POST "${ES_BASE}/${FEATURE_INDEX}/_search" \
    -H 'Content-Type: application/json' \
    -d "{
        \"size\": 20,
        \"query\": {
            \"bool\": {
                \"filter\": [
                    {\"term\": {\"assembly_id\": \"${ASSEMBLY_ID}\"}},
                    {\"nested\": {
                        \"path\": \"attributes\",
                        \"query\": {
                            \"bool\": {
                                \"filter\": [
                                    {\"term\": {\"attributes.key\": \"feature_type\"}},
                                    {\"term\": {\"attributes.keyword_value\": \"toplevel\"}}
                                ]
                            }
                        }
                    }}
                ]
            }
        }
    }" | jq '.hits.hits')

TOPLEVEL_COUNT=$(echo "$TOPLEVEL" | jq 'length')
echo "  got ${TOPLEVEL_COUNT} toplevel docs" >&2

# Discover the dominant busco feature type for this assembly
echo "Discovering busco feature type..." >&2
BUSCO_TYPE=$(curl -s -X POST "${ES_BASE}/${FEATURE_INDEX}/_search" \
    -H 'Content-Type: application/json' \
    -d "{
        \"size\": 0,
        \"query\": {
            \"bool\": {
                \"filter\": [
                    {\"term\": {\"assembly_id\": \"${ASSEMBLY_ID}\"}},
                    {\"nested\": {
                        \"path\": \"attributes\",
                        \"query\": {
                            \"term\": {\"attributes.key\": \"feature_type\"}
                        }
                    }}
                ]
            }
        },
        \"aggs\": {
            \"attrs\": {
                \"nested\": {\"path\": \"attributes\"},
                \"aggs\": {
                    \"by_type\": {
                        \"filter\": {\"term\": {\"attributes.key\": \"feature_type\"}},
                        \"aggs\": {
                            \"vals\": {
                                \"terms\": {\"field\": \"attributes.keyword_value\", \"size\": 20}
                            }
                        }
                    }
                }
            }
        }
    }" | jq -r '
        .aggregations.attrs.by_type.vals.buckets[]
        | select(.key | test("busco"; "i"))
        | .key
    ' | head -1)

if [[ -z "$BUSCO_TYPE" ]]; then
    echo "Warning: no busco feature type found; using 'busco-gene'" >&2
    BUSCO_TYPE="busco-gene"
fi
echo "  busco feature type: ${BUSCO_TYPE}" >&2

# Fetch up to 200 busco-gene feature docs
echo "Fetching busco-gene features..." >&2
FEATURES=$(curl -s -X POST "${ES_BASE}/${FEATURE_INDEX}/_search" \
    -H 'Content-Type: application/json' \
    -d "{
        \"size\": 200,
        \"query\": {
            \"bool\": {
                \"filter\": [
                    {\"term\": {\"assembly_id\": \"${ASSEMBLY_ID}\"}},
                    {\"nested\": {
                        \"path\": \"attributes\",
                        \"query\": {
                            \"bool\": {
                                \"filter\": [
                                    {\"term\": {\"attributes.key\": \"feature_type\"}},
                                    {\"term\": {\"attributes.keyword_value\": \"${BUSCO_TYPE}\"}}
                                ]
                            }
                        }
                    }}
                ]
            }
        }
    }" | jq '.hits.hits')

FEATURE_COUNT=$(echo "$FEATURES" | jq 'length')
echo "  got ${FEATURE_COUNT} feature docs" >&2

# Write combined fixture
jq -n \
    --argjson toplevel "$TOPLEVEL" \
    --argjson features "$FEATURES" \
    --arg assembly_id "$ASSEMBLY_ID" \
    --arg index "$FEATURE_INDEX" \
    --arg busco_type "$BUSCO_TYPE" \
    '{
        "meta": {
            "assembly_id": $assembly_id,
            "source_index": $index,
            "busco_type": $busco_type
        },
        "hits": ($toplevel + $features)
    }' > "$OUTFILE"

echo "Written: ${OUTFILE}" >&2
echo "  toplevel: ${TOPLEVEL_COUNT}, busco-gene (${BUSCO_TYPE}): ${FEATURE_COUNT}" >&2
