#!/bin/bash
# Validate downloaded CLI and SDK artifacts
#
# Usage:
#   bash scripts/validate_artifacts.sh [--deep] ./path/to/extracted/artifacts
#
# This script runs smoke tests on the CLI and SDKs to verify they work after download.
# Use --deep for comprehensive testing with real API calls (slower, ~2-3 min per language).
#
# Quick validation (default):
#   - Tests: import, instantiate, URL generation
#   - Time: ~30 seconds total
#
# Deep validation (--deep):
#   - Tests: validate(), count(), search(), describe(), snippet(), response parsing
#   - Time: ~2-3 minutes (includes real API calls)
#
# Artifact structure expected:
#   artifacts/
#     goat-cli               (binary) OR target/debug/goat-cli
#     goat_sdk-*.whl         (Python wheel)
#     r/goat/                (R package source)
#     js/goat/query.js       (JavaScript module)

set -e

DEEP_MODE=false
ARTIFACTS_DIR="${1:-.}"

# Parse --deep flag
if [[ "$1" == "--deep" ]]; then
  DEEP_MODE=true
  ARTIFACTS_DIR="${2:-.}"
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# ── Color output ───────────────────────────────────────────────────────────

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

pass() {
  echo -e "${GREEN}✓${NC} $1"
}

fail() {
  echo -e "${RED}✗${NC} $1"
  exit 1
}

warn() {
  echo -e "${YELLOW}⊙${NC} $1"
}

# ── Find artifacts ────────────────────────────────────────────────────────

find_cli_binary() {
  # Try standard locations first (fast path)
  if [[ -x "$ARTIFACTS_DIR/goat-cli" ]]; then
    echo "$ARTIFACTS_DIR/goat-cli"
    return 0
  fi
  if [[ -x "$ARTIFACTS_DIR/target/release/goat-cli" ]]; then
    echo "$ARTIFACTS_DIR/target/release/goat-cli"
    return 0
  fi
  if [[ -x "$ARTIFACTS_DIR/target/debug/goat-cli" ]]; then
    echo "$ARTIFACTS_DIR/target/debug/goat-cli"
    return 0
  fi

  # Check for non-executable goat-cli files in target/ folders (CI artifacts might not have executable bit set)
  local cli_binary=$(find "$ARTIFACTS_DIR" -type f -name "goat-cli" 2>/dev/null | head -1)
  if [[ -n "$cli_binary" ]]; then
    echo "$cli_binary"
    return 0
  fi

  # Recursive search for any executable ending in -cli (handles goat-cli-macos-aarch64, etc.)
  cli_binary=$(find "$ARTIFACTS_DIR" -type f -executable -name "*-cli" 2>/dev/null | head -1)
  if [[ -n "$cli_binary" ]]; then
    echo "$cli_binary"
    return 0
  fi

  return 1
}

find_python_wheel() {
  # Look for any .whl file recursively (handles goat_cli-*.whl, goat_sdk-*.whl, etc.)
  find "$ARTIFACTS_DIR" -type f -name "*.whl" 2>/dev/null | head -1
  [[ $? -eq 0 ]] && return 0 || return 1
}

find_r_sdk() {
  # Standard location first
  if [[ -d "$ARTIFACTS_DIR/r/goat" ]]; then
    echo "$ARTIFACTS_DIR/r/goat"
    return 0
  fi

  # Search for R package by DESCRIPTION file marker
  # R packages have DESCRIPTION file at their root
  find "$ARTIFACTS_DIR" -type f -name "DESCRIPTION" 2>/dev/null | while read desc; do
    desc_dir="$(dirname "$desc")"
    # Verify it looks like an R package (has R folder or NAMESPACE)
    if [[ -d "$desc_dir/R" ]] || [[ -f "$desc_dir/NAMESPACE" ]]; then
      echo "$desc_dir"
      return 0
    fi
  done
  return 1
}

find_js_sdk() {
  # Standard location first
  if [[ -f "$ARTIFACTS_DIR/js/goat/query.js" ]]; then
    echo "$ARTIFACTS_DIR/js/goat"
    return 0
  fi

  # Search for query.js file recursively (handles various folder structures)
  find "$ARTIFACTS_DIR" -type f -name "query.js" 2>/dev/null | while read query_js; do
    js_dir="$(dirname "$query_js")"
    echo "$js_dir"
    return 0
  done
  return 1
}

# ── Main validation ────────────────────────────────────────────────────────

if [[ "$DEEP_MODE" == "true" ]]; then
  echo "=== Deep Artifact Validation (with real API calls) ==="
else
  echo "=== Quick Artifact Validation ==="
fi
echo "Checking artifacts in: $ARTIFACTS_DIR"
echo

# CLI validation (no deep mode - always quick)
if CLI_BINARY=$(find_cli_binary); then
  echo "Testing CLI..."
  bash "$SCRIPT_DIR/validate_cli.sh" "$CLI_BINARY" || exit 1
  echo
else
  warn "CLI binary not found (skipping)"
fi

# Python SDK validation
if PYTHON_WHEEL=$(find_python_wheel); then
  echo "Testing Python SDK..."
  if [[ "$DEEP_MODE" == "true" ]]; then
    bash "$SCRIPT_DIR/validate_python_sdk_deep.sh" "$PYTHON_WHEEL" || exit 1
  else
    bash "$SCRIPT_DIR/validate_python_sdk.sh" "$PYTHON_WHEEL" || exit 1
  fi
  echo
else
  warn "Python SDK wheel not found (skipping)"
fi

# R SDK validation
if R_SDK=$(find_r_sdk); then
  echo "Testing R SDK..."
  if [[ "$DEEP_MODE" == "true" ]]; then
    bash "$SCRIPT_DIR/validate_r_sdk_deep.sh" "$R_SDK" || exit 1
  else
    bash "$SCRIPT_DIR/validate_r_sdk.sh" "$R_SDK" || exit 1
  fi
  echo
else
  warn "R SDK not found (skipping)"
fi

# JavaScript SDK validation
if JS_SDK=$(find_js_sdk); then
  echo "Testing JavaScript SDK..."
  if [[ "$DEEP_MODE" == "true" ]]; then
    bash "$SCRIPT_DIR/validate_javascript_sdk_deep.sh" "$JS_SDK" || exit 1
  else
    bash "$SCRIPT_DIR/validate_javascript_sdk.sh" "$JS_SDK" || exit 1
  fi
  echo
else
  warn "JavaScript SDK not found (skipping)"
fi

pass "All available artifacts validated successfully ✓"
