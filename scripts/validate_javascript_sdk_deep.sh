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
if [[ ! -f "$JS_SDK_DIR/query.js" ]]; then
    echo "✗ query.js not found in: $JS_SDK_DIR"
    exit 1
fi

# Check if WASM module is built (pkg-nodejs folder expected)
if [[ ! -d "$JS_SDK_DIR/pkg-nodejs" ]]; then
  echo "✗ JavaScript SDK WASM module missing (pkg-nodejs/ not found)"
  echo "  → CI artifacts are incomplete. The WASM module should have been built and included."
  exit 1
fi

# Normalize to absolute path (required for Node.js file:// URLs)
JS_SDK_DIR="$(cd "$JS_SDK_DIR" && pwd)"

# Check if Node.js is installed
if ! command -v node &> /dev/null; then
    echo "⊙ Node.js not found (skipping JavaScript SDK deep validation)"
    echo "   Install Node.js from https://nodejs.org/ to test"
    exit 0
fi

NODE_VERSION=$(node --version)
echo "✓ Node.js found: $NODE_VERSION"

echo "Running deep validation for JavaScript SDK..."

# Run deep tests
node --input-type=module << EOF || exit 1

import { QueryBuilder, parseSearchJson, parseResponseStatus } from "file://$JS_SDK_DIR/query.js";

console.log("\n== Deep Validation: JavaScript SDK ==\n");

// Helper for checking results
function assert(condition, message) {
  if (!condition) throw new Error(message);
}

// Test 1: Count method (real API call)
console.log("Test 1: Count (count())");
const qb1 = new QueryBuilder("taxon")
  .setTaxa(["Mammalia"], "tree");
const count = await qb1.count();
assert(typeof count === "number", "count() should return number");
assert(count > 0, "Expected count > 0 for Mammalia");
console.log(\`  ✓ count() works: \${count} records found\`);

// Test 2: Search method (real API call)
console.log("Test 2: Search (search())");
const qb2 = new QueryBuilder("taxon")
  .setTaxa(["Mammalia"], "tree")
  .addField("genome_size")
  .setSize(10);
const raw = await qb2.search();
const results = parseSearchJson(raw);
assert(Array.isArray(results), "search() should return array");
assert(results.length > 0, "Expected results for Mammalia search");
console.log(\`  ✓ search() works: returned \${results.length} results\`);
console.log(\`    First result: \${JSON.stringify(results[0]).substring(0, 80)}...\`);

// Test 3: Search with attribute filter
console.log("Test 3: Attribute filters (addAttribute())");
const qb3 = new QueryBuilder("taxon")
  .setTaxa(["Mammalia"], "tree")
  .addAttribute("genome_size", "ge", "1G")
  .addField("genome_size")
  .setSize(10);
const rawFiltered = await qb3.search();
const filtered = parseSearchJson(rawFiltered);
assert(filtered.length > 0, "Expected results with genome_size >= 1G");
assert(filtered.every(r => r.genome_size !== null), "All results should have genome_size");
console.log(\`  ✓ addAttribute() works: \${filtered.length} results with genome_size >= 1G\`);

// Test 4: Multiple attribute filters
console.log("Test 4: Multiple attribute filters");
const qb4 = new QueryBuilder("taxon")
  .setTaxa(["Mammalia"], "tree")
  .addAttribute("genome_size", "ge", "1G")
  .addAttribute("genome_size", "le", "3G")
  .addField("genome_size")
  .setSize(10);
const rawMultiFiltered = await qb4.search();
const multiFiltered = parseSearchJson(rawMultiFiltered);
assert(multiFiltered.length > 0, "Expected results in 1G-3G range");
console.log(\`  ✓ Multiple filters work: \${multiFiltered.length} results with 1G <= genome_size <= 3G\`);

// Test 5: Describe method
console.log("Test 5: Query description (describe())");
const qb5 = new QueryBuilder("taxon")
  .setTaxa(["Mammalia"], "tree")
  .addAttribute("genome_size", "ge", "1G");
const description = await qb5.describe();
assert(typeof description === "string", "describe() should return string");
assert(description.length > 0, "Description should not be empty");
console.log(\`  ✓ describe() works\`);
console.log(\`    \${description.substring(0, 100)}...\`);

// Test 6: Snippet generation
console.log("Test 6: Code snippet generation (snippet())");
const snippets = await qb5.snippet(["python", "r", "javascript"], "goat", "goat_sdk");
assert(snippets.python, "Should generate python snippet");
assert(snippets.r, "Should generate r snippet");
assert(snippets.javascript, "Should generate javascript snippet");
console.log(\`  ✓ snippet() works for all languages\`);

// Test 7: Different operators
console.log("Test 7: Different attribute operators");
const ops = [
  { op: "gt", name: "greater than" },
  { op: "ge", name: "greater than or equal" },
  { op: "le", name: "less than or equal" },
  { op: "eq", name: "equals" },
  { op: "exists", name: "exists" }
];

for (const {op, name} of ops) {
  try {
    const qb_op = new QueryBuilder("taxon")
      .setTaxa(["Mammalia"], "tree")
      .addAttribute("genome_size", op, op === "exists" ? null : "1G")
      .setSize(1);
    // Just verify URL builds - don't search each one
    const url = qb_op.toUrl();
    assert(url.includes("genome_size"), \`URL should include genome_size for operator '\${op}'\`);
    console.log(\`    ✓ '\${op}' (\${name}) works\`);
  } catch (e) {
    console.error(\`    ✗ '\${op}' failed: \${e.message}\`);
    throw e;
  }
}

console.log("\n✓ All deep validation tests passed!\n");

EOF

echo "✓ JavaScript SDK deep validation passed"
