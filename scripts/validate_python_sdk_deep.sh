#!/bin/bash
# Deep validation of Python SDK - comprehensive test with .count(), .search(), parse_response_status
# Usage: bash scripts/validate_python_sdk_deep.sh /path/to/goat_sdk-*.whl
# This tests real API calls, so it's slower but more thorough

set -e

WHEEL_PATH="${1:?Wheel path required}"

if [[ ! -f "$WHEEL_PATH" ]]; then
  echo "✗ Wheel not found: $WHEEL_PATH"
  exit 1
fi

# Create temp venv
VENV_DIR=$(mktemp -d)
trap "rm -rf $VENV_DIR" EXIT

echo "Setting up test environment in temp venv..."

# Install wheel and dependencies
python3 -m venv "$VENV_DIR" > /dev/null 2>&1 || python -m venv "$VENV_DIR" > /dev/null 2>&1

# Activate
source "$VENV_DIR/bin/activate"

pip install -q "$WHEEL_PATH" pyyaml 2>&1 || {
  echo "✗ Failed to install wheel"
  exit 1
}

# Run deep tests
python3 << 'EOF' || exit 1
import json
from goat_sdk import QueryBuilder, parse_response_status

print("\n== Deep Validation: Python SDK ==\n")

# Test 1: Validate method
print("Test 1: Validation (.validate())")
qb = QueryBuilder("taxon").set_taxa(["Mammalia"], filter_type="tree").add_field("genome_size")
errors = qb.validate()
assert isinstance(errors, list), "validate() should return list"
print(f"  ✓ validate() works, returned: {len(errors)} errors")

# Test 2: Count method (real API call)
print("Test 2: Count (.count())")
qb = QueryBuilder("taxon").set_taxa(["Mammalia"], filter_type="tree")
count = qb.count()
assert isinstance(count, int), "count() should return int"
assert count > 0, "Expected count > 0 for Mammalia"
print(f"  ✓ count() works: {count} records found")

# Test 3: Search method (real API call)
print("Test 3: Search (.search())")
qb = (
    QueryBuilder("taxon")
    .set_taxa(["Mammalia"], filter_type="tree")
    .add_field("genome_size")
    .set_size(10)
)
results = qb.search()
assert isinstance(results, list), "search() should return list"
assert len(results) > 0, "Expected results for Mammalia search"
print(f"  ✓ search() works: returned {len(results)} results")
print(f"    First result: {results[0]}")

# Test 4: Search with attribute filter
print("Test 4: Attribute filters (.add_attribute())")
qb = (
    QueryBuilder("taxon")
    .set_taxa(["Mammalia"], filter_type="tree")
    .add_attribute("genome_size", "ge", "1G")
    .add_field("genome_size")
    .set_size(10)
)
results = qb.search()
assert all("genome_size" in r for r in results), "All results should have genome_size"
print(f"  ✓ add_attribute() works: {len(results)} results with genome_size >= 1G")

# Test 5: Multiple attribute filters
print("Test 5: Multiple attribute filters")
qb = (
    QueryBuilder("taxon")
    .set_taxa(["Mammalia"], filter_type="tree")
    .add_attribute("genome_size", "ge", "1G")
    .add_attribute("genome_size", "le", "3G")
    .add_field("genome_size")
    .set_size(10)
)
results = qb.search()
assert len(results) > 0, "Expected results in 1G-3G range"
print(f"  ✓ Multiple filters work: {len(results)} results with 1G <= genome_size <= 3G")

# Test 6: Response parsing
print("Test 6: Response parsing (parse_response_status())")
qb = QueryBuilder("taxon").set_taxa(["Insecta"], filter_type="tree").add_field("genome_size").set_size(5)
response = qb.search_raw()
status_json = json.loads(parse_response_status(json.dumps(response)))
assert "hits" in status_json, "Status should have 'hits' field"
assert "took" in status_json, "Status should have 'took' field"
print(f"  ✓ parse_response_status() works")
print(f"    Total hits: {status_json['hits']}")
print(f"    Query time: {status_json['took']}ms")

# Test 7: Describe method
print("Test 7: Query description (.describe())")
qb = (
    QueryBuilder("taxon")
    .set_taxa(["Mammalia"], filter_type="tree")
    .add_attribute("genome_size", "ge", "1G")
)
description = qb.describe()
assert isinstance(description, str), "describe() should return string"
assert len(description) > 0, "Description should not be empty"
print(f"  ✓ describe() works")
print(f"    {description[:100]}...")

# Test 8: Snippet generation
print("Test 8: Code snippet generation (.snippet())")
snippets = qb.snippet(site_name="goat", sdk_name="goat_sdk", languages=["python", "r", "javascript"])
assert "python" in snippets, "Should generate python snippet"
assert "r" in snippets, "Should generate r snippet"
assert "javascript" in snippets, "Should generate javascript snippet"
print(f"  ✓ snippet() works for all languages")

print("\n✓ All deep validation tests passed!\n")
EOF

echo "✓ Python SDK deep validation passed"
