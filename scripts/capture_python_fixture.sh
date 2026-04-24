#!/usr/bin/env bash
# Capture a raw JSON search response using the installed Python SDK wheel
# Usage: bash scripts/capture_python_fixture.sh /path/to/wheel.whl /path/to/output.json [index taxa]

set -euo pipefail

WHEEL_PATH="${1:?Wheel path required}"
OUT_PATH="${2:?Output path required}"
INDEX="${3:-taxon}"
TAXA="${4:-Mammalia}"

if [[ ! -f "$WHEEL_PATH" ]]; then
  echo "✗ Wheel not found: $WHEEL_PATH"
  exit 1
fi

VENV_DIR=$(mktemp -d)
trap 'rm -rf "$VENV_DIR"' EXIT

python3 -m venv "$VENV_DIR"
source "$VENV_DIR/bin/activate"
pip install -q "$WHEEL_PATH" pyyaml

python3 - <<PY
import json
from goat_sdk import QueryBuilder

qb = QueryBuilder("$INDEX").set_taxa(["$TAXA"], filter_type="tree").add_field("genome_size").set_size(5)
raw = qb.search(format="json")
with open("$OUT_PATH", "w", encoding="utf-8") as fh:
    fh.write(raw)
print("Wrote fixture:", "$OUT_PATH")
PY

echo "✓ Fixture capture complete"
