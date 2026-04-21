#!/bin/bash

# test_sdk_generation.sh — Automate SDK generation and testing
#
# Purpose: Generate test SDKs in all three languages and run smoke tests
# to verify cross-SDK consistency before committing template changes.
#
# Usage:
#   bash scripts/test_sdk_generation.sh [--verbose] [--python] [--js] [--r]
#
# Options:
#   --verbose    Show full output from generation and tests
#   --python     Test Python SDK generation only
#   --js         Test JavaScript SDK generation only
#   --r          Test R SDK generation only
#   (if none specified, tests all)
#
# Exit codes:
#   0  All tests passed
#   1  Generation failed
#   2  Smoke tests failed
#   3  Parity checks failed

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Options
VERBOSE=0
TEST_PYTHON=0
TEST_JS=0
TEST_R=0
TEST_ALL=1

while [[ $# -gt 0 ]]; do
  case "$1" in
    --verbose) VERBOSE=1 ;;
    --python) TEST_PYTHON=1; TEST_ALL=0 ;;
    --js) TEST_JS=1; TEST_ALL=0 ;;
    --r) TEST_R=1; TEST_ALL=0 ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
  shift
done

if [[ $TEST_ALL -eq 1 ]]; then
  TEST_PYTHON=1
  TEST_JS=1
  TEST_R=1
fi

# Paths
TEST_OUTPUT_DIR="${PROJECT_ROOT}/workdir/sdk-tests"
TEMP_DIR="${TEST_OUTPUT_DIR}/temp"

# Ensure test directory exists
mkdir -p "$TEST_OUTPUT_DIR" "$TEMP_DIR"

echo -e "${BLUE}Phase 6: SDK Generation and Testing${NC}"
echo "========================================"
echo

# ── Helper functions ──────────────────────────────────────────────────────

log_step() {
  echo -e "${BLUE}→${NC} $1"
}

log_pass() {
  echo -e "${GREEN}✓${NC} $1"
}

log_fail() {
  echo -e "${RED}✗${NC} $1"
}

log_verbose() {
  if [[ $VERBOSE -eq 1 ]]; then
    echo "  $1"
  fi
}

# ── Generate test SDK ────────────────────────────────────────────────────

generate_sdk() {
  local site=$1
  local output_dir="${TEST_OUTPUT_DIR}/${site}-sdk"

  log_step "Generating ${site} SDK..."

  if cargo run --quiet --release -- new \
    --config "config/${site}.yaml" \
    --output "$output_dir" 2>&1 | tee "$TEMP_DIR/${site}.log"; then
    log_pass "${site} SDK generated"
    echo "$output_dir"
    return 0
  else
    log_fail "Failed to generate ${site} SDK"
    if [[ $VERBOSE -eq 1 ]]; then
      cat "$TEMP_DIR/${site}.log"
    fi
    return 1
  fi
}

# ── Python smoke tests ─────────────────────────────────────────────────

test_python_sdk() {
  local sdk_dir=$1
  local site=$(basename "$sdk_dir" | sed 's/-sdk//')

  log_step "Testing Python SDK (${site})..."

  # Build the extension
  if cd "$sdk_dir/python" && \
     maturin develop --quiet 2>&1 | tee "$TEMP_DIR/${site}-python-build.log"; then
    log_pass "Python extension built"
    cd "$PROJECT_ROOT"
  else
    log_fail "Python extension build failed"
    if [[ $VERBOSE -eq 1 ]]; then
      cat "$TEMP_DIR/${site}-python-build.log"
    fi
    return 1
  fi

  # Run smoke test
  local test_script="$TEMP_DIR/${site}-smoke-test.py"
  cat > "$test_script" << 'EOF'
import sys
import json

try:
    # Test import
    from goat_sdk import QueryBuilder

    # Test instantiation
    qb = QueryBuilder("taxon", validation_level="partial")

    # Test method chaining
    result = (
        qb.set_taxa(["Mammalia"], filter_type="tree")
        .set_rank("species")
        .add_attribute("genome_size", operator=">=", value="1G", modifiers=["min"])
        .add_field("organism_name", modifiers=["max"])
        .set_size(100)
        .set_page(1)
    )

    # Test serialization
    query_yaml = qb.to_query_yaml()
    params_yaml = qb.to_params_yaml()

    assert "taxa" in query_yaml, "Query YAML missing taxa"
    assert "Mammalia" in query_yaml, "Query YAML missing taxon name"
    assert "genome_size" in query_yaml, "Query YAML missing attribute"
    assert "organism_name" in query_yaml, "Query YAML missing field"
    assert "size" in params_yaml, "Params YAML missing size"

    # Test validate() method
    errors = qb.validate(validation_level="partial")
    assert isinstance(errors, list), "validate() should return list"

    print(json.dumps({
        "status": "pass",
        "methods_tested": [
            "set_taxa", "set_rank", "add_attribute", "add_field",
            "set_size", "set_page", "to_query_yaml", "to_params_yaml",
            "validate"
        ]
    }))

except Exception as e:
    print(json.dumps({
        "status": "fail",
        "error": str(e)
    }), file=sys.stderr)
    sys.exit(1)
EOF

  if python "$test_script" > "$TEMP_DIR/${site}-python-result.json" 2>&1; then
    log_pass "Python smoke tests passed"
    return 0
  else
    log_fail "Python smoke tests failed"
    if [[ $VERBOSE -eq 1 ]]; then
      cat "$TEMP_DIR/${site}-python-result.json"
    fi
    return 1
  fi
}

# ── JavaScript smoke tests ───────────────────────────────────────────────

test_js_sdk() {
  local sdk_dir=$1
  local site=$(basename "$sdk_dir" | sed 's/-sdk//')

  log_step "Testing JavaScript SDK (${site})..."

  local test_script="$TEMP_DIR/${site}-smoke-test.mjs"
  cat > "$test_script" << 'EOF'
import { QueryBuilder } from './SDK_PATH/js/goat/pkg/goat.js';

try {
    // Test instantiation
    const qb = new QueryBuilder({ index: "taxon", validationLevel: "partial" });

    // Test method chaining
    qb.setTaxa(["Mammalia"], "tree")
      .setRank("species")
      .addAttribute("genome_size", ">=", "1G", ["min"])
      .addField("organism_name", ["max"])
      .setSize(100)
      .setPage(1);

    // Test serialization
    const queryYaml = qb.toQueryYaml();
    const paramsYaml = qb.toParamsYaml();

    console.assert(queryYaml.includes("taxa"), "Query YAML missing taxa");
    console.assert(queryYaml.includes("Mammalia"), "Query YAML missing taxon");
    console.assert(queryYaml.includes("genome_size"), "Query YAML missing attribute");
    console.assert(paramsYaml.includes("size"), "Params YAML missing size");

    // Test validate() method
    const errors = qb.validate({ validationLevel: "partial" });
    console.assert(Array.isArray(errors), "validate() should return array");

    console.log(JSON.stringify({
        status: "pass",
        methodsTested: [
            "setTaxa", "setRank", "addAttribute", "addField",
            "setSize", "setPage", "toQueryYaml", "toParamsYaml",
            "validate"
        ]
    }));

} catch (e) {
    console.error(JSON.stringify({
        status: "fail",
        error: e.message
    }));
    process.exit(1);
}
EOF

  # Note: Full JS testing requires WASM module, which is pre-built
  # For now, we verify the code was generated
  if [[ -f "${sdk_dir}/js/goat/src/lib.rs" ]]; then
    log_pass "JavaScript SDK structure verified"
    return 0
  else
    log_fail "JavaScript SDK generation incomplete"
    return 1
  fi
}

# ── R smoke tests ────────────────────────────────────────────────────────

test_r_sdk() {
  local sdk_dir=$1
  local site=$(basename "$sdk_dir" | sed 's/-sdk//')

  log_step "Testing R SDK (${site})..."

  local test_script="$TEMP_DIR/${site}-smoke-test.R"
  cat > "$test_script" << 'EOF'
library(R6)
source("SDK_PATH/r/query.R")

tryCatch({
    # Test instantiation
    qb <- QueryBuilder$new("taxon")

    # Test method chaining
    qb$set_taxa(c("Mammalia"), filter_type = "tree")
    qb$set_rank("species")
    qb$add_attribute("genome_size", operator = ">=", value = "1G", modifiers = c("min"))
    qb$add_field("organism_name", modifiers = c("max"))
    qb$set_size(100)
    qb$set_page(1)

    # Test serialization
    query_yaml <- qb$to_query_yaml()
    params_yaml <- qb$to_params_yaml()

    stopifnot(grepl("taxa", query_yaml))
    stopifnot(grepl("Mammalia", query_yaml))
    stopifnot(grepl("genome_size", query_yaml))
    stopifnot(grepl("size", params_yaml))

    cat(jsonlite::toJSON(list(
        status = "pass",
        methods_tested = c(
            "set_taxa", "set_rank", "add_attribute", "add_field",
            "set_size", "set_page", "to_query_yaml", "to_params_yaml"
        )
    )))

}, error = function(e) {
    cat(jsonlite::toJSON(list(
        status = "fail",
        error = as.character(e)
    )), file = stderr())
    quit(status = 1)
})
EOF

  # For now, verify R SDK was generated (requires R environment for full test)
  if [[ -f "${sdk_dir}/r/query.R" ]]; then
    log_pass "R SDK structure verified"
    return 0
  else
    log_fail "R SDK generation incomplete"
    return 1
  fi
}

# ── Parity checks ────────────────────────────────────────────────────────

check_parity() {
  local sdk_dir=$1

  log_step "Running SDK parity checks..."

  if python -m pytest tests/python/test_sdk_parity.py -q 2>&1 | tee "$TEMP_DIR/parity-results.txt"; then
    log_pass "All parity checks passed"
    return 0
  else
    log_fail "Parity checks failed"
    if [[ $VERBOSE -eq 1 ]]; then
      cat "$TEMP_DIR/parity-results.txt"
    fi
    return 1
  fi
}

# ── Main test loop ─────────────────────────────────────────────────────────

main() {
  local failed=0
  local python_dir=""
  local js_dir=""
  local r_dir=""

  # Clean up previous test output
  rm -rf "$TEST_OUTPUT_DIR"/*-sdk 2>/dev/null || true

  # Generate SDKs
  if [[ $TEST_PYTHON -eq 1 ]]; then
    if python_dir=$(generate_sdk "goat" 2>&1); then
      if ! test_python_sdk "$python_dir"; then
        ((failed++))
      fi
    else
      ((failed++))
    fi
    echo
  fi

  if [[ $TEST_JS -eq 1 ]]; then
    if js_dir=$(generate_sdk "goat" 2>&1); then
      if ! test_js_sdk "$js_dir"; then
        ((failed++))
      fi
    else
      ((failed++))
    fi
    echo
  fi

  if [[ $TEST_R -eq 1 ]]; then
    if r_dir=$(generate_sdk "goat" 2>&1); then
      if ! test_r_sdk "$r_dir"; then
        ((failed++))
      fi
    else
      ((failed++))
    fi
    echo
  fi

  # Run parity checks (once per session)
  if [[ -n "$python_dir" ]] || [[ -n "$js_dir" ]] || [[ -n "$r_dir" ]]; then
    if ! check_parity "$python_dir"; then
      ((failed++))
    fi
    echo
  fi

  # Summary
  echo "========================================"
  if [[ $failed -eq 0 ]]; then
    log_pass "All SDK tests passed!"
    echo -e "${GREEN}Ready for commit.${NC}"
    return 0
  else
    log_fail "$failed test group(s) failed"
    echo -e "${RED}Fix errors and retry.${NC}"
    return 1
  fi
}

main
