#!/bin/bash
set -euo pipefail

WHEEL_PATH="${1:?Wheel path required}"
OUT_PATH="${2:-scripts/fixtures-goat/fixture_mammalia_search_raw.json}"

if [[ ! -f "$WHEEL_PATH" ]]; then
  echo "✗ Wheel not found: $WHEEL_PATH"
  exit 1
fi

VENV_DIR=$(mktemp -d)
trap 'rm -rf "$VENV_DIR"' EXIT

echo "Setting up temp venv to capture fixture..."
python3 -m venv "$VENV_DIR"
source "$VENV_DIR/bin/activate"

pip install -q "$WHEEL_PATH" pyyaml

python3 scripts/capture_fixture_using_sdk.py "$OUT_PATH"

echo "Fixture capture complete: $OUT_PATH"
