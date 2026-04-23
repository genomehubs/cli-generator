#!/bin/bash
# Validate Python SDK wheel
#
# Usage:
#   bash scripts/validate_python_sdk.sh ./path/to/goat_sdk-*.whl

set -e

WHEEL_PATH="${1:?Wheel path required}"

if [[ ! -f "$WHEEL_PATH" ]]; then
  echo "✗ Wheel not found: $WHEEL_PATH"
  exit 1
fi

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

pass() { echo -e "${GREEN}✓${NC} $1"; }
fail() { echo -e "${RED}✗${NC} $1"; exit 1; }

# Create temp venv for testing
TEMP_VENV=$(mktemp -d)
trap "rm -rf $TEMP_VENV" EXIT

python3 -m venv "$TEMP_VENV" || fail "Failed to create venv"
source "$TEMP_VENV/bin/activate"
pip install -q "$WHEEL_PATH" pyyaml || fail "Failed to install wheel"

pass "Python SDK installed"

# Test 1: Import works
python3 -c "from goat_sdk import QueryBuilder" || fail "Failed to import QueryBuilder"
pass "Import QueryBuilder works"

# Test 2: Basic builder
python3 << 'EOF' || fail "QueryBuilder basic usage failed"
from goat_sdk import QueryBuilder
qb = QueryBuilder("taxon")
assert qb._index == "taxon", "Index not set"
EOF
pass "QueryBuilder instantiation works"

# Test 3: Builder methods
python3 << 'EOF' || fail "QueryBuilder methods failed"
from goat_sdk import QueryBuilder
qb = QueryBuilder("taxon").set_taxa(["Mammalia"], filter_type="tree").add_field("genome_size")
assert len(qb._taxa) > 0, "Taxa not set"
assert len(qb._fields) > 0, "Fields not set"
EOF
pass "QueryBuilder methods (set_taxa, add_field) work"

# Test 4: URL generation
python3 << 'EOF' || fail "URL generation failed"
from goat_sdk import QueryBuilder
qb = QueryBuilder("taxon").set_taxa(["Mammalia"], filter_type="tree").add_field("genome_size")
url = qb.to_url()
assert "genomehubs.org" in url, "URL doesn't contain API base"
assert "search" in url, "URL doesn't contain endpoint"
assert "Mammalia" in url, "Taxa not in URL"
print(f"Generated URL: {url}")
EOF
pass "URL generation works"

# Test 5: Validation
python3 << 'EOF' || fail "Validation failed"
from goat_sdk import QueryBuilder
qb = QueryBuilder("taxon").set_taxa(["Mammalia"], filter_type="tree").add_field("genome_size")
errors = qb.validate()
assert isinstance(errors, list), "validate() didn't return list"
print(f"Validation returned {len(errors)} errors")
EOF
pass "Validation works"

echo "✓ Python SDK validation passed"
