#!/usr/bin/env bash
set -euo pipefail

# Code verification script
# Runs formatting, linting, type checking, and tests on Rust and Python code
#
# Usage:
#   bash scripts/verify_code.sh                              # Auto-detect project root
#   bash scripts/verify_code.sh --verbose                    # Show detailed output
#   PROJECT_ROOT=/path/to/project bash scripts/verify_code.sh  # Use custom project root
#   PROJECT_ROOT=/path/to/project bash scripts/verify_code.sh --verbose  # Custom root + verbose
#
# Checks:
# - Rust: fmt, clippy, tests
# - Python: black, isort, pyright, pytest
#
# Exit codes:
#   0 = all checks pass
#   1 = one or more checks fail

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_ROOT="${PROJECT_ROOT:-$(dirname "$SCRIPT_DIR")}"

cd "$PROJECT_ROOT"

ERRORS=0
VERBOSE=0
SECTION_SEP="========================================================================"

# Parse command-line arguments
for arg in "$@"; do
    case "$arg" in
        --verbose|-v)
            VERBOSE=1
            ;;
        *)
            echo "Unknown option: $arg"
            echo "Usage: bash scripts/verify_code.sh [--verbose]"
            exit 1
            ;;
    esac
done

# Color codes for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

section_start() {
    echo ""
    echo "$SECTION_SEP"
    echo "  $1"
    echo "$SECTION_SEP"
}

check_pass() {
    echo -e "${GREEN}✓${NC} $1"
}

check_fail() {
    echo -e "${RED}✗${NC} $1"
    if [[ $VERBOSE -eq 1 && -n "${2:-}" ]]; then
        echo ""
        echo "${2}"
        echo ""
    fi
    ERRORS=$((ERRORS + 1))
}

# ==============================================================================
# RUST CHECKS
# ==============================================================================

section_start "Rust Code Verification"

# Format check
echo "Checking cargo fmt..."
if cargo fmt --all -- --check > /dev/null 2>&1; then
    check_pass "cargo fmt (code is properly formatted)"
else
    output=$(cargo fmt --all -- --check 2>&1 || true)
    check_fail "cargo fmt (run 'cargo fmt --all' to fix)" "$output"
fi

# Check compilation of all workspace crates
echo "Checking cargo check for all crates..."
if cargo check --workspace 2>/tmp/cargo_check.log; then
    check_pass "cargo check --workspace (all crates compile)"
else
    output=$(cat /tmp/cargo_check.log)
    check_fail "cargo check --workspace" "$output"
fi

# Clippy linting — covers all workspace crates (., crates/genomehubs-query, crates/genomehubs-api)
echo "Running cargo clippy (all workspace crates)..."
if cargo clippy --all-targets -- -D warnings 2>/tmp/cargo_clippy.log; then
    check_pass "cargo clippy --all-targets (no linting issues)"
else
    # Surface only the actual error lines to keep output manageable
    output=$(grep -E "^error" /tmp/cargo_clippy.log | head -20 || cat /tmp/cargo_clippy.log | tail -30)
    check_fail "cargo clippy (errors listed below — run 'cargo clippy --all-targets -- -D warnings' for full output)" "$output"
fi

# Tests — all workspace lib crates
echo "Running cargo tests..."
if cargo test --workspace --lib > /tmp/cargo_test.log 2>&1; then
    passed=$(grep -c "^test .* ok$" /tmp/cargo_test.log || echo 0)
    check_pass "cargo test --workspace --lib ($passed tests passed)"
else
    # Show failing test names and any compile errors
    output=$(grep -E "^error|FAILED|thread .* panicked" /tmp/cargo_test.log | head -20 || cat /tmp/cargo_test.log | tail -30)
    check_fail "cargo test --workspace --lib (see below)" "$output"
fi

# ==============================================================================
# PYTHON CHECKS
# ==============================================================================

section_start "Python Code Verification"

# Black formatting
echo "Checking black..."
if black --check --line-length 120 python/ tests/python/ > /dev/null 2>&1; then
    check_pass "black (code is properly formatted)"
else
    output=$(black --check --line-length 120 python/ tests/python/ 2>&1 || true)
    check_fail "black (run 'black --line-length 120 python/ tests/python/' to fix)" "$output"
fi

# Import sorting
echo "Checking isort..."
if isort --check-only --profile black --line-length 120 python/ tests/python/ > /dev/null 2>&1; then
    check_pass "isort (imports are properly sorted)"
else
    output=$(isort --check-only --profile black --line-length 120 python/ tests/python/ 2>&1 || true)
    check_fail "isort (run 'isort --profile black --line-length 120 python/ tests/python/' to fix)" "$output"
fi

# Type checking
echo "Running pyright..."
if pyright python/ tests/python/ > /dev/null 2>&1; then
    check_pass "pyright (no type errors)"
else
    output=$(pyright python/ tests/python/ 2>&1 | grep -E "error:|warning:" | head -20 || true)
    check_fail "pyright (type errors listed below)" "$output"
fi

# Python tests
echo "Running pytest..."
if python -m pytest tests/python/ -q > /tmp/pytest.log 2>&1; then
    passed=$(grep -E "^[0-9]+ passed" /tmp/pytest.log | head -1 || echo "")
    check_pass "pytest ($passed)"
else
    output=$(tail -30 /tmp/pytest.log)
    check_fail "pytest (see below)" "$output"
fi

# Python coverage — informational, fails only if below floor in pyproject.toml
if command -v coverage > /dev/null 2>&1; then
    echo "Measuring Python coverage..."
    if coverage run -m pytest tests/python/ -q > /dev/null 2>&1 && \
       coverage report --skip-empty > /tmp/coverage.log 2>&1; then
        total=$(grep "^TOTAL" /tmp/coverage.log | awk '{print $NF}')
        check_pass "coverage ($total — above floor)"
    else
        output=$(tail -5 /tmp/coverage.log || true)
        check_fail "coverage (below floor — see pyproject.toml [tool.coverage.report])" "$output"
    fi
else
    echo "  coverage not installed — skipping (pip install coverage[toml])"
fi

# ==============================================================================
# SUMMARY
# ==============================================================================

section_start "Summary"

if (( ERRORS == 0 )); then
    echo -e "${GREEN}✓ All checks passed!${NC}"
    echo ""
    exit 0
else
    echo -e "${RED}✗ $ERRORS check(s) failed${NC}"
    echo ""
    echo "Quick fixes:"
    echo "  Rust formatting:   cargo fmt --all"
    echo "  Python formatting: black --line-length 120 python/ tests/python/"
    echo "  Python imports:    isort --profile black --line-length 120 python/ tests/python/"
    echo ""
    exit 1
fi
