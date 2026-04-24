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

# Run standalone Python validation script (installed wheel required)
python3 scripts/validate_python_sdk.py || fail "Python SDK basic validation failed"
pass "Python SDK validation passed"
