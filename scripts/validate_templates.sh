#!/usr/bin/env bash
set -euo pipefail

# Template validation script
# Extracts code from Tera templates and validates formatting/style
#
# Usage: bash scripts/validate_templates.sh
#
# Checks:
# - Rust templates with rustfmt
# - Python templates with black and isort

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
TEMPLATES_DIR="$PROJECT_ROOT/templates"
TEMP_DIR=$(mktemp -d)

trap "rm -rf $TEMP_DIR" EXIT

ERRORS=0

echo "Validating Tera templates for formatting and style..."

# ==============================================================================
# Helper: Detect Rust templates (support both flat and organized structure)
# ==============================================================================

find_rust_templates() {
    if [[ -d "$TEMPLATES_DIR/rust" ]]; then
        find "$TEMPLATES_DIR/rust" -name "*.tera" -type f
    else
        # Fall back to flat structure: find .tera files that are likely Rust (*.rs.tera)
        find "$TEMPLATES_DIR" -maxdepth 1 -name "*.rs.tera" -type f
    fi
}

# ==============================================================================
# Helper: Detect Python templates
# ==============================================================================

find_python_templates() {
    if [[ -d "$TEMPLATES_DIR/python" ]]; then
        find "$TEMPLATES_DIR/python" -name "*.tera" -type f
    else
        # Fall back to flat structure: find .tera files that are likely Python (*.py.tera, *.pyi.tera)
        find "$TEMPLATES_DIR" -maxdepth 1 \( -name "*.py.tera" -o -name "*.pyi.tera" \) -type f
    fi
}

# ==============================================================================
# RUST TEMPLATES
# ==============================================================================

echo ""
echo "Checking Rust templates..."

while IFS= read -r template; do
    [[ -z "$template" ]] && continue

    template_name=$(basename "$template")
    temp_file="$TEMP_DIR/${template_name%.tera}.rs"

    # Extract Rust code by replacing Tera expressions with valid Rust
    # {{ ... }} -> 0 (valid integer expression)
    # {% ... %} -> // comment
    perl -pe '
        s/{{[^}]*?}}/0/g;
        s/{%-?[^%]*?-?%}/\/\/ template block/g;
    ' "$template" > "$temp_file"

    if rustfmt --check "$temp_file" 2>/dev/null; then
        echo "  ✓ $template_name"
    else
        echo "  ✗ $template_name (rustfmt formatting issue)"
        ERRORS=$((ERRORS + 1))
    fi
done < <(find_rust_templates)

# ==============================================================================
# PYTHON TEMPLATES
# ==============================================================================

echo ""
echo "Checking Python templates..."

while IFS= read -r template; do
    [[ -z "$template" ]] && continue

    template_name=$(basename "$template")
    temp_file="$TEMP_DIR/${template_name%.tera}.py"

    # Extract Python code by replacing Tera expressions with valid Python
    # {{ ... }} -> None (valid Python expression)
    # {% ... %} -> # comment
    perl -pe '
        s/{{[^}]*?}}/None/g;
        s/{%-?[^%]*?-?%}/# template block/g;
    ' "$template" > "$temp_file"

    # Check with black
    if ! black --check --quiet "$temp_file" 2>/dev/null; then
        echo "  ✗ $template_name (black formatting issue)"
        ERRORS=$((ERRORS + 1))
    # Check with isort
    elif ! isort --check-only --quiet --profile black "$temp_file" 2>/dev/null; then
        echo "  ✗ $template_name (isort import ordering issue)"
        ERRORS=$((ERRORS + 1))
    else
        echo "  ✓ $template_name"
    fi
done < <(find_python_templates)

# ==============================================================================
# SUMMARY
# ==============================================================================

echo ""
if (( ERRORS == 0 )); then
    echo "✓ All templates validated successfully"
    exit 0
else
    echo "✗ $ERRORS template(s) failed validation"
    echo ""
    echo "Tips for fixing template issues:"
    echo "  1. Generate the CLI: cargo run -- new goat --output-dir /tmp/test-goat"
    echo "  2. Edit the generated file (gets full syntax highlighting + linting)"
    echo "  3. Verify formatting: cargo fmt / black / isort"
    echo "  4. Apply changes back to the template"
    echo "  5. Regenerate and verify output is identical"
    exit 1
fi
