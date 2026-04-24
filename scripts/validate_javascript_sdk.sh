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

# Delegate to the Node.js validator script for basic checks
node "$PWD/scripts/validate_javascript_sdk.js" "$JS_SDK_DIR" || fail "JavaScript SDK basic validation failed"
pass "JavaScript SDK validation passed"
