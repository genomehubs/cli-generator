#!/bin/bash
# Deep validation of R SDK - comprehensive test with $count(), $search(), response parsing
# Usage: bash scripts/validate_r_sdk_deep.sh /path/to/r/goat
# This tests real API calls, so it's slower but more thorough

set -e

R_PKG_DIR="${1:?R package directory required}"

# Normalize path to absolute
R_PKG_DIR="$(cd "$R_PKG_DIR" 2>/dev/null && pwd)" || {
    echo "✗ R package directory not found: $1"
    exit 1
}

# Check if R is installed
if ! command -v Rscript &> /dev/null; then
    echo "⊙ R not found (skipping R SDK deep validation)"
    echo "   Install R from https://www.r-project.org/ to test"
    exit 0
fi

echo "Running deep validation for R SDK..."

"$PWD/scripts/validate_r_sdk_deep.R" || {
    echo "✗ Missing helper script: scripts/validate_r_sdk_deep.R"
    exit 1
}

Rscript --vanilla "$PWD/scripts/validate_r_sdk_deep.R" "$R_PKG_DIR" || exit 1

echo "✓ R SDK deep validation passed"
