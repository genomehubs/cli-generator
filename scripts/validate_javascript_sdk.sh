#!/bin/bash
# Validate JavaScript SDK
#
# Usage:
#   bash scripts/validate_javascript_sdk.sh ./path/to/goat/js/dir

set -e

JS_SDK_DIR="${1:?JavaScript SDK directory path required}"

# Convert to absolute path
JS_SDK_DIR="$(cd "$JS_SDK_DIR" && pwd)"

if [[ ! -f "$JS_SDK_DIR/query.js" ]]; then
  echo "✗ query.js not found in: $JS_SDK_DIR"
  exit 1
fi

# Check if WASM module is built (pkg-nodejs folder expected)
if [[ ! -d "$JS_SDK_DIR/pkg-nodejs" ]]; then
  echo "✗ JavaScript SDK WASM module missing (pkg-nodejs/ not found)"
  echo "  → CI artifacts are incomplete. The WASM module must be built and included."
  exit 1
fi

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

pass() { echo -e "${GREEN}✓${NC} $1"; }
fail() { echo -e "${RED}✗${NC} $1"; exit 1; }

# Check Node.js available
if ! command -v node &> /dev/null; then
  echo "⊙ Node.js not found — skipping JavaScript SDK tests"
  exit 0
fi

pass "Node.js found: $(node --version)"

# Test 1: Import works
node --input-type=module << EOF || fail "Failed to import QueryBuilder"
const { QueryBuilder } = await import("file://$JS_SDK_DIR/query.js");
console.log("Import successful");
EOF
pass "Import QueryBuilder works"

# Test 2: Basic builder
node --input-type=module << EOF || fail "QueryBuilder instantiation failed"
const { QueryBuilder } = await import("file://$JS_SDK_DIR/query.js");
const qb = new QueryBuilder("taxon");
if (qb._index !== "taxon") throw new Error("Index not set");
console.log("Instantiation successful");
EOF
pass "QueryBuilder instantiation works"

# Test 3: Builder methods
node --input-type=module << EOF || fail "QueryBuilder methods failed"
const { QueryBuilder } = await import("file://$JS_SDK_DIR/query.js");
const qb = new QueryBuilder("taxon")
  .setTaxa(["Mammalia"], "tree")
  .addField("genome_size");
if (qb._taxa.length === 0) throw new Error("Taxa not set");
if (qb._fields.length === 0) throw new Error("Fields not set");
console.log("Methods successful");
EOF
pass "QueryBuilder methods (setTaxa, addField) work"

# Test 4: URL generation
node --input-type=module << EOF || fail "URL generation failed"
const { QueryBuilder } = await import("file://$JS_SDK_DIR/query.js");
const qb = new QueryBuilder("taxon")
  .setTaxa(["Mammalia"], "tree")
  .addField("genome_size");
const url = qb.toUrl();
if (!url.includes("genomehubs.org")) throw new Error("URL missing API base");
if (!url.includes("search")) throw new Error("URL missing endpoint");
if (!url.includes("Mammalia")) throw new Error("Taxa not in URL");
console.log("URL generated: " + url);
EOF
pass "URL generation works"

echo "✓ JavaScript SDK validation passed"
