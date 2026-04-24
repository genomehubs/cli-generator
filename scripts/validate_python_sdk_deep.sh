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

# Run deep tests via external Python script for easier development and debugging
python3 scripts/validate_python_sdk_deep.py || exit 1

echo "✓ Python SDK deep validation passed"
