#!/bin/bash
# Test fixtures against a generated SDK
#
# This script:
# 1. Auto-detects the site from the SDK
# 2. Discovers fixtures from that site's API
# 3. Tests the SDK against those fixtures
#
# Usage:
#   bash scripts/test_sdk_fixtures.sh --site goat [--python | --javascript | --r]
#
# Examples:
#   # Test Python SDK with auto-discovered fixtures
#   ./scripts/test_sdk_fixtures.sh --site goat --python
#
#   # Test all languages
#   ./scripts/test_sdk_fixtures.sh --site goat --all

set -euo pipefail

# Default values
SITE=""
TEST_PYTHON=false
TEST_JAVASCRIPT=false
TEST_R=false
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# ── Helper functions ──────────────────────────────────────────────────────────

usage() {
    cat << EOF
Test fixtures against a generated SDK

This script discovers fixtures from the site's API and tests the SDK against them.

USAGE:
    ./scripts/test_sdk_fixtures.sh --site <name> [OPTIONS]

OPTIONS:
    --site <name>       Site name (e.g., 'goat'). Required.
    --python            Test Python SDK
    --javascript        Test JavaScript SDK
    --r                 Test R SDK
    --all               Test all languages (default if none specified)
    --help              Show this help message

EXAMPLES:
    # Test Python only
    ./scripts/test_sdk_fixtures.sh --site goat --python

    # Test all languages
    ./scripts/test_sdk_fixtures.sh --site goat --all

    # Test JavaScript
    ./scripts/test_sdk_fixtures.sh --site goat --javascript

NOTES:
    - Generated SDK must exist at: workdir/my-\$SITE/\$SITE-cli/
    - Fixtures are auto-discovered from the site's API (https://\$SITE.genomehubs.org/api)
EOF
    exit 1
}

log_info() {
    echo -e "${BLUE}→${NC} $1"
}

log_success() {
    echo -e "${GREEN}✓${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}⚠${NC} $1"
}

log_error() {
    echo -e "${RED}✗${NC} $1"
}

# ── Parse arguments ───────────────────────────────────────────────────────────

while [[ $# -gt 0 ]]; do
    case $1 in
        --site)
            SITE="$2"
            shift 2
            ;;
        --python)
            TEST_PYTHON=true
            shift
            ;;
        --javascript)
            TEST_JAVASCRIPT=true
            shift
            ;;
        --r)
            TEST_R=true
            shift
            ;;
        --all)
            TEST_PYTHON=true
            TEST_JAVASCRIPT=true
            TEST_R=true
            shift
            ;;
        --help)
            usage
            ;;
        *)
            log_error "Unknown option: $1"
            usage
            ;;
    esac
done

# Validate required arguments
if [[ -z "$SITE" ]]; then
    log_error "Missing required argument: --site"
    usage
fi

# Default to all languages if none specified
if ! $TEST_PYTHON && ! $TEST_JAVASCRIPT && ! $TEST_R; then
    TEST_PYTHON=true
    TEST_JAVASCRIPT=true
    TEST_R=true
fi

# ── Verify setup ──────────────────────────────────────────────────────────────

log_info "Testing fixtures for ${SITE} SDK (discovering from site API)"

API_BASE="https://${SITE}.genomehubs.org/api"
GENERATED_SDK_ROOT="$PROJECT_ROOT/workdir/my-${SITE}/${SITE}-cli"

# Check that generated SDK exists
if [[ ! -d "$GENERATED_SDK_ROOT" ]]; then
    log_error "Generated SDK not found: $GENERATED_SDK_ROOT"
    echo ""
    echo "Generate the SDK first:"
    echo "  bash scripts/dev_site.sh --python --output ./workdir/my-${SITE} ${SITE}"
    exit 1
fi

log_success "Found generated SDK at: $GENERATED_SDK_ROOT"

# ── Discover & cache fixtures ──────────────────────────────────────────────────

FIXTURES_CACHE_DIR="$PROJECT_ROOT/tests/python/fixtures-${SITE}"
mkdir -p "$FIXTURES_CACHE_DIR"

if [[ -z "$(ls -A "$FIXTURES_CACHE_DIR" 2>/dev/null)" ]]; then
    log_info "Discovering fixtures from ${API_BASE}..."

    python "$PROJECT_ROOT/tests/python/discover_fixtures.py" \
        --site "$SITE" \
        --api-base "$API_BASE" \
        --update \
        > /dev/null || {
        log_error "Failed to discover fixtures from $API_BASE"
        echo "Is the API running? Check: $API_BASE/v2/search"
        exit 1
    }
    log_success "Discovered and cached $(ls "$FIXTURES_CACHE_DIR" | wc -l) fixtures"
else
    log_success "Found $(ls "$FIXTURES_CACHE_DIR" | wc -l) cached fixtures"
fi

# ── Test Python SDK ───────────────────────────────────────────────────────────

if $TEST_PYTHON; then
    log_info "Testing Python SDK..."

    PYTHON_SDK_PATH="$GENERATED_SDK_ROOT/python/${SITE}_sdk"
    if [[ ! -d "$PYTHON_SDK_PATH" ]]; then
        log_warn "Python SDK not found: $PYTHON_SDK_PATH (skipped)"
    else
        log_success "Found Python SDK"

        # Run pytest with the generated SDK on the path
        cd "$PROJECT_ROOT"
        PYTHONPATH="${GENERATED_SDK_ROOT}:${PROJECT_ROOT}:${PYTHONPATH:-}" \
            pytest tests/python/test_sdk_fixtures.py::TestFixtureValidation \
                --tb=short \
                -v || {
            log_error "Python fixture tests failed"
            exit 1
        }
        log_success "Python fixture tests passed"
    fi
fi

# ── Test JavaScript SDK ──────────────────────────────────────────────────────

if $TEST_JAVASCRIPT; then
    log_info "Testing JavaScript SDK..."

    JS_SDK_PATH="$GENERATED_SDK_ROOT/js/${SITE}"
    if [[ ! -d "$JS_SDK_PATH" ]]; then
        log_warn "JavaScript SDK not found: $JS_SDK_PATH (skipped)"
    else
        log_success "Found JavaScript SDK"

        # Install dependencies if needed
        if [[ ! -d "$JS_SDK_PATH/node_modules" ]]; then
            log_info "Installing JavaScript dependencies..."
            npm install --prefix "$JS_SDK_PATH" > /dev/null 2>&1 || {
                log_error "npm install failed"
                exit 1
            }
        fi

        # Run fixture tests from PROJECT_ROOT (fixtures are in tests/python/fixtures-{SITE})
        log_info "Running JavaScript fixture tests..."
        cd "$PROJECT_ROOT"
        SITE="$SITE" \
        JS_SDK_PATH="${JS_SDK_PATH}/query.js" \
        FIXTURES_DIR="$FIXTURES_CACHE_DIR" \
            node --test tests/javascript/test_sdk_fixtures.mjs || {
            log_error "JavaScript fixture tests failed"
            exit 1
        }
        log_success "JavaScript fixture tests passed"
    fi
fi

# ── Test R SDK ────────────────────────────────────────────────────────────────

if $TEST_R; then
    log_info "Testing R SDK..."

    R_SDK_PATH="$GENERATED_SDK_ROOT/r/${SITE}"
    if [[ ! -d "$R_SDK_PATH" ]]; then
        log_warn "R SDK not found: $R_SDK_PATH (skipped)"
    else
        log_success "Found R SDK"

        # Run fixture tests from PROJECT_ROOT
        log_info "Running R fixture tests (compiling Rust extension if needed)..."
        cd "$PROJECT_ROOT"
        SITE="$SITE" \
        R_SDK_PATH="$R_SDK_PATH" \
        FIXTURES_DIR="$FIXTURES_CACHE_DIR" \
            Rscript tests/r/test_sdk_fixtures.R || {
            log_error "R fixture tests failed"
            exit 1
        }
        log_success "R fixture tests passed"
    fi
fi

# ── Summary ────────────────────────────────────────────────────────────────────

echo ""
log_success "All fixture tests passed for ${SITE} SDK"
echo ""
echo "Summary:"
$TEST_PYTHON && echo "  ✓ Python SDK validated against ${SITE} API" || true
$TEST_JAVASCRIPT && echo "  ✓ JavaScript SDK validated against ${SITE} API" || true
$TEST_R && echo "  ✓ R SDK validated against ${SITE} API" || true
echo ""
echo "Fixtures cached in: $FIXTURES_CACHE_DIR"

