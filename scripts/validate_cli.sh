#!/bin/bash
# Validate CLI binary
#
# Usage:
#   bash scripts/validate_cli.sh ./path/to/goat-cli

set -e

CLI_PATH="${1:?CLI path required}"

if [[ ! -f "$CLI_PATH" ]]; then
  echo "✗ CLI not found: $CLI_PATH"
  exit 1
fi

# Ensure CLI is executable (CI artifacts might not have the bit set)
chmod +x "$CLI_PATH"

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

pass() { echo -e "${GREEN}✓${NC} $1"; }
fail() { echo -e "${RED}✗${NC} $1"; exit 1; }

# Test 1: Help works
"$CLI_PATH" --help > /dev/null 2>&1 || fail "CLI --help failed"
pass "CLI --help works"

# Test 2: Subcommand help works
"$CLI_PATH" taxon search --help > /dev/null 2>&1 || fail "CLI taxon search --help failed"
pass "CLI taxon search --help works"

# Test 3: URL generation (no API call)
URL=$("$CLI_PATH" taxon search --taxon Mammalia --field-groups genome-size --url 2>&1)
if [[ "$URL" == *"genomehubs.org"* ]]; then
  pass "CLI URL generation works: $URL"
else
  fail "CLI URL generation didn't produce valid URL: $URL"
fi

# Test 4: List field groups
"$CLI_PATH" taxon search --list-field-groups > /dev/null 2>&1 || fail "CLI --list-field-groups failed"
pass "CLI --list-field-groups works"

echo "✓ CLI validation passed"
