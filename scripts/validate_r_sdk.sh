#!/bin/bash
# Validate R SDK by testing QueryBuilder operations
# Usage: bash scripts/validate_r_sdk.sh /path/to/r/package
# Example: bash scripts/validate_r_sdk.sh ./r/goat

set -e

if [ -z "$1" ]; then
    echo "Usage: bash scripts/validate_r_sdk.sh /path/to/r/package"
    exit 1
fi

R_PKG_DIR="$1"

# Normalize path to absolute for R
R_PKG_DIR="$(cd "$R_PKG_DIR" 2>/dev/null && pwd)" || {
    echo "✗ R package directory not found: $1"
    exit 1
}

echo "Testing R SDK..."

# Check if R is installed
if ! command -v Rscript &> /dev/null; then
    echo "⊙ R not found (skipping R SDK validation)"
    echo "   Install R from https://www.r-project.org/ to test"
    exit 0
fi

R_VERSION=$(Rscript --version 2>&1 | head -1)
echo "✓ R found: $R_VERSION"

# Test QueryBuilder operations in R
# Create a temporary R script file
"$PWD/scripts/validate_r_sdk.R" || {
    echo "✗ Missing helper script: scripts/validate_r_sdk.R"
    exit 1
}

R_RESULT=$(Rscript --vanilla "$PWD/scripts/validate_r_sdk.R" "$R_PKG_DIR" 2>&1) || {
    echo "$R_RESULT" | sed 's/^/  /'
    echo "✗ R SDK validation failed"
    exit 1
}

echo "$R_RESULT" | sed 's/^/  /'
echo "✓ R SDK validation passed"
exit 0
