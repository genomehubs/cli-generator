#!/bin/bash
# Deep validation of JavaScript SDK - comprehensive test with count(), search()
# Usage: bash scripts/validate_javascript_sdk_deep.sh /path/to/js/goat
# This tests real API calls, so it's slower but more thorough

set -e

if [ -z "$1" ]; then
    echo "Usage: bash scripts/validate_javascript_sdk_deep.sh /path/to/js/goat"
    exit 1
fi

JS_SDK_DIR="$1"

# Check if directory exists
if [[ ! -d "$JS_SDK_DIR" ]]; then
    echo "✗ JavaScript SDK directory not found: $JS_SDK_DIR"
    exit 1
fi

# Check if query.js exists
node "$PWD/scripts/validate_javascript_sdk_deep.mjs" "$JS_SDK_DIR" || exit 1

echo "✓ JavaScript SDK deep validation passed"
