#!/bin/bash
# Measure code coverage for Rust and Python
# Usage: bash scripts/measure_coverage.sh
# Output: coverage/ directory with HTML reports

echo "================================================"
echo "Code Coverage Measurement"
echo "================================================"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Create coverage directory
mkdir -p coverage

# ────────────────────────────────────────────────────────────────────────────
# Rust Coverage (tarpaulin)
# ────────────────────────────────────────────────────────────────────────────

echo ""
echo "📊 Measuring Rust code coverage..."
echo "   Command: cargo tarpaulin --out Html --output-dir ./coverage"

if ! command -v cargo-tarpaulin &> /dev/null; then
    echo -e "${YELLOW}⚠️  cargo-tarpaulin not found, installing...${NC}"
    cargo install cargo-tarpaulin
fi

cargo tarpaulin \
    --out Html \
    --output-dir ./coverage \
    --timeout 300 \
    --exclude-files tests/generated_goat_cli.rs \
    2>&1 | tee coverage/rust_measurement.log

# Find the coverage percentage from tarpaulin output (handle macOS grep without -P)
RUST_COVERAGE=$(grep "Coverage:" coverage/rust_measurement.log | sed 's/.*Coverage: //; s/%.*//' | tail -1)

if [ -z "$RUST_COVERAGE" ]; then
    RUST_COVERAGE="See HTML report"
fi

echo ""
echo "✅ Rust coverage: ${RUST_COVERAGE}%"
echo "   HTML report: coverage/tarpaulin-report.html"
echo ""

# ────────────────────────────────────────────────────────────────────────────
# Python Coverage (coverage.py)
# ────────────────────────────────────────────────────────────────────────────

echo "📊 Measuring Python code coverage..."
echo "   Command: coverage run -m pytest && coverage report"

# Check if dev dependencies are installed
if ! python -c "import coverage" 2>/dev/null; then
    echo -e "${YELLOW}⚠️  coverage.py not found, installing...${NC}"
    pip install 'coverage[toml]' pytest pyyaml
fi

# Run tests with coverage
python -m coverage run -m pytest tests/python/ -v --tb=short

# Check if tests had issues (but continue to show coverage report anyway)
if [ $? -ne 0 ]; then
    echo ""
    echo -e "${YELLOW}⚠️  Python tests encountered issues${NC}"
    echo "If you see ImportError for cli_generator, try:"
    echo "  maturin develop --features extension-module"
    echo ""
fi

# Generate reports
echo ""
python -m coverage report --skip-empty
python -m coverage html --directory ./coverage/python

# Extract Python coverage percentage more reliably
# Look for the TOTAL line and get the rightmost percentage value
PYTHON_COVERAGE=$(python -m coverage report --skip-empty 2>/dev/null | grep "^TOTAL" | tail -1 | awk '{print $NF}' | sed 's/%//')

if [ -z "$PYTHON_COVERAGE" ]; then
    PYTHON_COVERAGE="See HTML report"
fi

echo ""
echo "✅ Python coverage: ${PYTHON_COVERAGE}"
echo "   HTML report: coverage/python/index.html"
echo ""

# ────────────────────────────────────────────────────────────────────────────
# Summary
# ────────────────────────────────────────────────────────────────────────────

echo ""
echo "================================================"
echo "Coverage Summary"
echo "================================================"
echo ""
if [[ "$RUST_COVERAGE" == "See HTML report" ]]; then
    echo "Rust coverage:   $RUST_COVERAGE (check coverage/tarpaulin-report.html)"
else
    echo "Rust coverage:   ${RUST_COVERAGE}%"
fi

if [[ "$PYTHON_COVERAGE" == "See HTML report" ]]; then
    echo "Python coverage: $PYTHON_COVERAGE (check coverage/python/index.html)"
else
    echo "Python coverage: ${PYTHON_COVERAGE}%"
fi
echo ""
echo "📂 Coverage reports:"
echo "   Rust:   ./coverage/tarpaulin-report.html"
echo "   Python: ./coverage/python/index.html"
echo ""
echo "Next steps:"
echo "  1. Open coverage reports in browser: open coverage/tarpaulin-report.html"
echo "  2. Identify gaps and add tests to improve coverage"
echo "  3. Target: 85%+ line coverage, 80%+ branch coverage"
echo ""
