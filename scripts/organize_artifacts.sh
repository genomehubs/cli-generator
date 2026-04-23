#!/bin/bash
# Organize messy CI artifact downloads into a clean structure
#
# Usage:
#   bash scripts/organize_artifacts.sh /path/to/downloads
#
# This script auto-detects CLI, Python wheel, R package, and JavaScript SDK
# across various CI output formats and folder structures, then copies them
# to a standardized ./artifacts/ directory ready for validate_artifacts.sh
#
# Example:
#   # Downloads are scattered across different files/folders
#   ls ~/Downloads/
#   # → goat-cli-macos-aarch64/ (folder)
#   # → goat_0.1.0.tar (R package)
#   # → goat_0.1.0.tar (JS package, same name!)
#   # → goat_cli-0.1.0-*.whl (Python wheel)
#
#   bash scripts/organize_artifacts.sh ~/Downloads/
#   # Organized artifacts in: ./artifacts
#
#   bash scripts/validate_artifacts.sh ./artifacts

set -e

INPUT_DIR="${1:-.}"
OUTPUT_DIR="./artifacts"

if [[ ! -d "$INPUT_DIR" ]]; then
  echo "✗ Input directory not found: $INPUT_DIR"
  exit 1
fi

echo "Scanning for artifacts in: $INPUT_DIR"
echo ""

mkdir -p "$OUTPUT_DIR"

# Create .gitignore to prevent 2k+ files from cluttering git status
echo "*" > "$OUTPUT_DIR/.gitignore"
echo "!.gitignore" >> "$OUTPUT_DIR/.gitignore"

# ────────────────────────────────────────────────────────────────────────────
# CLI Binary Detection
# ────────────────────────────────────────────────────────────────────────────

echo "Scanning for CLI binary..."

# Look for CLI binary: might be executable, or might be in target/release/ folder
cli_found=$(find "$INPUT_DIR" -type f -name "goat-cli" 2>/dev/null | head -1)

# If not found, look for any file named *-cli (don't require executable bit, since macOS downloads lose it)
if [[ -z "$cli_found" ]]; then
  cli_found=$(find "$INPUT_DIR" -type f -name "*-cli" 2>/dev/null | head -1)
fi

if [[ -n "$cli_found" ]]; then
  cli_filename=$(basename "$cli_found")
  echo "  Found: $cli_filename"
  # Use cat instead of cp to preserve executable bit on macOS downloads
  cat "$cli_found" > "$OUTPUT_DIR/goat-cli"
  chmod +x "$OUTPUT_DIR/goat-cli"
  echo "  ✓ Copied to: $OUTPUT_DIR/goat-cli"
else
  echo "  ⊙ No CLI binary found"
fi

echo ""

# ────────────────────────────────────────────────────────────────────────────
# Python Wheel Detection
# ────────────────────────────────────────────────────────────────────────────

echo "Scanning for Python SDK wheel..."

# Look for goat SDK wheels in order of preference:
# 1. goat_sdk*.whl (new naming after generator fix)
# 2. goat_cli*.whl (current naming during transition)
# 3. Any other .whl as fallback
wheel_found=$(find "$INPUT_DIR" -type f -name "goat_sdk*.whl" 2>/dev/null | head -1)
if [[ -z "$wheel_found" ]]; then
  wheel_found=$(find "$INPUT_DIR" -type f -name "goat_cli*.whl" 2>/dev/null | head -1)
fi
if [[ -z "$wheel_found" ]]; then
  wheel_found=$(find "$INPUT_DIR" -type f -name "goat*.whl" 2>/dev/null | head -1)
fi

if [[ -n "$wheel_found" ]]; then
  wheel_filename=$(basename "$wheel_found")
  echo "  Found: $wheel_filename"
  # Use cat instead of cp to preserve file integrity
  cat "$wheel_found" > "$OUTPUT_DIR/${wheel_filename}"
  echo "  ✓ Copied to: $OUTPUT_DIR/${wheel_filename}"
else
  echo "  ⊙ No Python wheel (.whl) found"
fi

echo ""

# ────────────────────────────────────────────────────────────────────────────
# R Package Detection
# ────────────────────────────────────────────────────────────────────────────

echo "Scanning for R SDK package..."

# R packages have a DESCRIPTION file at the root
# First check if there are .tar or .tar.gz files that might be R packages
r_tarball=$(find "$INPUT_DIR" -maxdepth 2 -type f \( -name "goat*.tar" -o -name "goat*.tar.gz" \) 2>/dev/null | head -1)

if [[ -n "$r_tarball" ]]; then
  echo "  Found tarball: $(basename "$r_tarball")"

  # Extract to temp, check for DESCRIPTION, copy to proper location
  temp_extract=$(mktemp -d)
  trap "rm -rf $temp_extract" EXIT

  if tar -tzf "$r_tarball" 2>/dev/null | grep -q "DESCRIPTION"; then
    # This is likely the R package
    tar -xzf "$r_tarball" -C "$temp_extract" 2>/dev/null || tar -xf "$r_tarball" -C "$temp_extract"

    # Find the extracted folder
    extracted_folder=$(find "$temp_extract" -maxdepth 1 -type d ! -name "$(basename "$temp_extract")" | head -1)

    if [[ -f "$extracted_folder/DESCRIPTION" ]]; then
      echo "  ✓ Verified as R package"
      mkdir -p "$OUTPUT_DIR/r"
      cp -r "$extracted_folder" "$OUTPUT_DIR/r/goat"
      echo "  ✓ Copied to: $OUTPUT_DIR/r/goat"
    fi
  fi
fi

# Also search for R packages by DESCRIPTION file (if already extracted)
if [[ ! -d "$OUTPUT_DIR/r/goat" ]]; then
  r_pkg=$(find "$INPUT_DIR" -type f -name "DESCRIPTION" 2>/dev/null | while read desc; do
    desc_dir="$(dirname "$desc")"
    # Verify it's an R package (has R/ or NAMESPACE)
    if [[ -d "$desc_dir/R" ]] || [[ -f "$desc_dir/NAMESPACE" ]]; then
      echo "$desc_dir"
      break
    fi
  done)

  if [[ -n "$r_pkg" ]]; then
    echo "  Found R package: $(basename "$r_pkg")"
    mkdir -p "$OUTPUT_DIR/r"
    cp -r "$r_pkg" "$OUTPUT_DIR/r/goat"
    echo "  ✓ Copied to: $OUTPUT_DIR/r/goat"
  fi
fi

if [[ ! -d "$OUTPUT_DIR/r/goat" ]]; then
  echo "  ⊙ No R package found"
fi

echo ""

# ────────────────────────────────────────────────────────────────────────────
# JavaScript Package Detection
# ────────────────────────────────────────────────────────────────────────────

echo "Scanning for JavaScript SDK package..."

# JavaScript packages contain query.js
# Look for query.js files
js_query=$(find "$INPUT_DIR" -type f -name "query.js" 2>/dev/null | head -1)

if [[ -n "$js_query" ]]; then
  js_dir=$(dirname "$js_query")
  echo "  Found: $(basename "$js_dir")"
  mkdir -p "$OUTPUT_DIR/js"
  cp -r "$js_dir" "$OUTPUT_DIR/js/goat"
  echo "  ✓ Copied to: $OUTPUT_DIR/js/goat"

  # WASM packages (pkg-nodejs/ and pkg-web/) should have been copied as part of js_dir
  # But verify they're present
  if [[ ! -d "$OUTPUT_DIR/js/goat/pkg-nodejs" ]]; then
    # Try copying from adjacent location if they exist elsewhere
    if [[ -d "$(dirname "$js_dir")/pkg-nodejs" ]]; then
      cp -r "$(dirname "$js_dir")/pkg-nodejs" "$OUTPUT_DIR/js/goat/"
    fi
  fi
  if [[ ! -d "$OUTPUT_DIR/js/goat/pkg-web" ]]; then
    if [[ -d "$(dirname "$js_dir")/pkg-web" ]]; then
      cp -r "$(dirname "$js_dir")/pkg-web" "$OUTPUT_DIR/js/goat/"
    fi
  fi
else
  # Check if there's a tarball that might contain JS (same issue as R)
  js_tarball=$(find "$INPUT_DIR" -maxdepth 2 -type f \( -name "goat*.tar" -o -name "goat*.tar.gz" \) 2>/dev/null | grep -v "\.tar$" | head -1)

  if [[ -n "$js_tarball" ]]; then
    # Try extracting this one (if not already done for R)
    temp_extract=$(mktemp -d)
    trap "rm -rf $temp_extract" EXIT

    tar -xzf "$js_tarball" -C "$temp_extract" 2>/dev/null || tar -xf "$js_tarball" -C "$temp_extract"

    extracted_folder=$(find "$temp_extract" -maxdepth 1 -type d ! -name "$(basename "$temp_extract")" | head -1)

    # Look for query.js in the extracted content
    if find "$extracted_folder" -name "query.js" 2>/dev/null | grep -q .; then
      js_pkg=$(find "$extracted_folder" -type f -name "query.js" | head -1 | xargs dirname)
      echo "  Found nested in tarball: $(basename "$js_pkg")"
      mkdir -p "$OUTPUT_DIR/js"
      cp -r "$js_pkg" "$OUTPUT_DIR/js/goat"
      echo "  ✓ Copied to: $OUTPUT_DIR/js/goat"
    fi
  else
    echo "  ⊙ No JavaScript package found"
  fi
fi

echo ""

# ────────────────────────────────────────────────────────────────────────────
# Summary
# ────────────────────────────────────────────────────────────────────────────

echo "=================================================="
echo "Organization Complete"
echo "=================================================="
echo ""
echo "Artifacts organized in: $OUTPUT_DIR/"
echo ""

if [[ -x "$OUTPUT_DIR/goat-cli" ]]; then
  echo "✓ CLI binary found"
else
  echo "⊙ CLI binary not found"
fi

if ls "$OUTPUT_DIR"/*.whl 1>/dev/null 2>&1; then
  echo "✓ Python SDK wheel found"
else
  echo "⊙ Python SDK wheel not found"
fi

if [[ -d "$OUTPUT_DIR/r/goat" ]]; then
  echo "✓ R SDK package found"
else
  echo "⊙ R SDK package not found"
fi

if [[ -d "$OUTPUT_DIR/js/goat" ]]; then
  echo "✓ JavaScript SDK package found"
else
  echo "⊙ JavaScript SDK package not found"
fi

echo ""
echo "Next: Run validation"
echo "  bash scripts/validate_artifacts.sh $OUTPUT_DIR"
echo ""
