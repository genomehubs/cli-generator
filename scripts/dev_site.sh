#!/usr/bin/env bash
# Generate a local development copy of a site CLI, with optional WASM rebuild.
#
# Usage:
#   bash scripts/dev_site.sh [OPTIONS] [SITE]
#
# Arguments:
#   SITE          Site name to generate (default: goat)
#
# Options:
#   --rebuild-wasm   Rebuild crates/genomehubs-query/pkg/ via wasm-pack before
#                    generating. Required whenever a new #[wasm_bindgen] function
#                    is added to crates/genomehubs-query/src/lib.rs. The bundled
#                    pkg/ is committed to the repo; forgetting to rebuild it means
#                    generated JS projects will see "is not a function" errors at
#                    runtime for any newly-added WASM export.
#   --python         After generating, run maturin develop + Python smoke-test.
#   --output DIR     Output directory (default: /tmp/<site>-cli)
#   -h, --help       Show this help.
#
# Examples:
#   bash scripts/dev_site.sh                          # generate goat to /tmp/goat-cli
#   bash scripts/dev_site.sh --rebuild-wasm           # rebuild WASM first, then generate
#   bash scripts/dev_site.sh --rebuild-wasm --python  # also run Python smoke-test
#   bash scripts/dev_site.sh boat                     # generate boat instead
#   bash scripts/dev_site.sh --output /tmp/my-goat goat

set -euo pipefail

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

SITE="goat"
REBUILD_WASM=0
RUN_PYTHON=0
OUTPUT_DIR=""

# ── Argument parsing ──────────────────────────────────────────────────────────

while [[ $# -gt 0 ]]; do
    case "$1" in
        --rebuild-wasm) REBUILD_WASM=1; shift ;;
        --python)       RUN_PYTHON=1;   shift ;;
        --output)       OUTPUT_DIR="$2"; shift 2 ;;
        -h|--help)
            sed -n '/^# Usage:/,/^[^#]/p' "$0" | grep '^#' | sed 's/^# \?//'
            exit 0
            ;;
        -*)
            echo "Unknown option: $1" >&2; exit 1 ;;
        *)
            SITE="$1"; shift ;;
    esac
done

OUTPUT_DIR="${OUTPUT_DIR:-/tmp/${SITE}-cli}"
SITE_CLI_DIR="${OUTPUT_DIR}/${SITE}-cli"
SDK_NAME="${SITE}_sdk"

cd "$PROJECT_ROOT"

GREEN='\033[0;32m'; YELLOW='\033[1;33m'; RED='\033[0;31m'; NC='\033[0m'
ok()   { echo -e "${GREEN}✓${NC}  $*"; }
warn() { echo -e "${YELLOW}!${NC}  $*"; }
fail() { echo -e "${RED}✗${NC}  $*" >&2; exit 1; }

# ── Step 1: Rebuild WASM pkg if requested ─────────────────────────────────────

if [[ $REBUILD_WASM -eq 1 ]]; then
    echo "Rebuilding WASM package..."
    if ! command -v wasm-pack &>/dev/null; then
        fail "wasm-pack not found. Install with: cargo install wasm-pack"
    fi
    (
        cd crates/genomehubs-query
        wasm-pack build --target nodejs --features wasm
    )

    # wasm-pack --target nodejs generates CJS. Convert to ESM so query.js
    # (which uses ESM import syntax) can load it directly in Node ≥ 18.
    PKG_JS="crates/genomehubs-query/pkg/genomehubs_query.js"
    PKG_JSON="crates/genomehubs-query/pkg/package.json"

    # Add ESM header imports (idempotent — skip if already present)
    if ! grep -q "^import { readFileSync" "$PKG_JS"; then
        sed -i '' 's|/\* @ts-self-types=.*\*/|/* @ts-self-types="./genomehubs_query.d.ts" */\n\nimport { readFileSync } from '"'"'fs'"'"';\nimport { fileURLToPath } from '"'"'url'"'"';|' "$PKG_JS"
    fi
    # Replace require('fs').readFileSync with ESM equivalent
    sed -i '' \
        's|const wasmPath = `\${__dirname}/\(.*\)`;|const wasmPath = fileURLToPath(new URL('"'"'./\1'"'"', import.meta.url));|' \
        "$PKG_JS"
    sed -i '' 's|const wasmBytes = require(.fs.)\.readFileSync(wasmPath);|const wasmBytes = readFileSync(wasmPath);|' "$PKG_JS"

    # Replace exports.X = X with a single named export block at the end
    ALL_EXPORTS=$(grep "^exports\." "$PKG_JS" | sed "s/exports\.\([^ =]*\) = .*/\1/" | tr '\n' ' ')
    sed -i '' '/^exports\./d' "$PKG_JS"
    if [[ -n "$ALL_EXPORTS" ]]; then
        EXPORT_NAMES=$(echo "$ALL_EXPORTS" | tr ' ' ',' | sed 's/,$//')
        echo "" >> "$PKG_JS"
        echo "export { ${EXPORT_NAMES} };" >> "$PKG_JS"
    fi

    # Mark the pkg as ESM
    if ! grep -q '"type": "module"' "$PKG_JSON"; then
        sed -i '' 's|"main": "genomehubs_query.js",|"main": "genomehubs_query.js",\n  "type": "module",|' "$PKG_JSON"
    fi

    ok "WASM package rebuilt → crates/genomehubs-query/pkg/"
    echo ""
    echo "  NOTE: the updated pkg/ should be committed so generated projects"
    echo "  pick up any new WASM exports (e.g. a new parse_* function)."
    echo ""
else
    # Warn if pkg/ looks stale (lib.rs newer than the built wasm)
    PKG_JS="crates/genomehubs-query/pkg/genomehubs_query.js"
    LIB_RS="crates/genomehubs-query/src/lib.rs"
    if [[ -f "$PKG_JS" && "$LIB_RS" -nt "$PKG_JS" ]]; then
        warn "crates/genomehubs-query/src/lib.rs is newer than pkg/genomehubs_query.js"
        warn "Run with --rebuild-wasm if you added a new #[wasm_bindgen] export."
    fi
fi

# ── Step 2: Clean previous output and regenerate ─────────────────────────────

echo "Generating ${SITE}-cli into ${OUTPUT_DIR}..."
rm -rf "$OUTPUT_DIR"
cargo run -- new "$SITE" --config sites/ --output-dir "$OUTPUT_DIR"
ok "Generated ${SITE}-cli → ${SITE_CLI_DIR}"

# ── Step 3: Quick Rust smoke-test (URL flag, no network) ──────────────────────

echo ""
echo "Running Rust smoke-test (--url flag)..."
(
    cd "$SITE_CLI_DIR"
    cargo build -q 2>&1
    BINARY="./target/debug/${SITE}-cli"
    URL_OUT=$("$BINARY" taxon search \
        --taxon Mammalia --taxon-filter tree \
        --filter genome_size ge 1000000000 \
        --size 10 --url 2>&1)
    if echo "$URL_OUT" | grep -q "goat.genomehubs.org\|api"; then
        ok "CLI --url output: ${URL_OUT}"
    else
        fail "CLI --url produced unexpected output: ${URL_OUT}"
    fi
)

# ── Step 4: JS smoke-test ─────────────────────────────────────────────────────

JS_PACKAGE="${SITE//-/_}"
JS_DIR="$(cd "${SITE_CLI_DIR}/js/${JS_PACKAGE}" 2>/dev/null && pwd || true)"

if [[ -d "$JS_DIR" ]]; then
    echo ""
    echo "Running JS smoke-test (toUrl, no network)..."
    SMOKE_MJS=$(mktemp /tmp/js-smoke-XXXXXX.mjs)
    cat > "$SMOKE_MJS" << JSEOF
import { QueryBuilder } from '${JS_DIR}/query.js';
const qb = new QueryBuilder('taxon').setTaxa(['Mammalia'], 'tree').setSize(10);
console.log(qb.toUrl());
JSEOF
    URL_OUT=$(node "$SMOKE_MJS" 2>&1)
    rm -f "$SMOKE_MJS"
    if echo "$URL_OUT" | grep -q "api"; then
        ok "JS toUrl(): ${URL_OUT}"
    else
        fail "JS toUrl() produced unexpected output: ${URL_OUT}"
    fi

    # Check that all expected parse functions are exported from the bundled pkg
    PKG_JS="${JS_DIR}/pkg/genomehubs_query.js"
    for fn in parse_response_status parse_search_json annotate_source_labels split_source_columns; do
        if [[ -f "$PKG_JS" ]] && ! grep -q "$fn" "$PKG_JS"; then
            warn "${fn} not found in bundled pkg/genomehubs_query.js"
            warn "Run with --rebuild-wasm to include newly-added WASM exports."
        fi
    done
fi

# ── Step 5: Optional Python smoke-test ───────────────────────────────────────

if [[ $RUN_PYTHON -eq 1 ]]; then
    echo ""
    echo "Building Python extension (maturin develop)..."
    if ! command -v maturin &>/dev/null; then
        fail "maturin not found. Install with: pip install maturin"
    fi
    (
        cd "$SITE_CLI_DIR"
        maturin develop --features extension-module -q
    )
    ok "maturin develop complete"

    echo "Running Python smoke-test..."
    python3 -c "
from ${SDK_NAME}.query import QueryBuilder
qb = QueryBuilder('taxon')
url = qb.to_url()
assert 'api' in url, f'Unexpected URL: {url}'
desc = qb.describe()
assert isinstance(desc, str) and len(desc) > 0, 'describe() returned empty'
snip = qb.snippet(site_name='${SITE}', sdk_name='${SDK_NAME}')
assert 'python' in snip, 'snippet() missing python key'
print('  to_url():  ', url)
print('  describe():', desc)
print('  snippet() keys:', list(snip.keys()))
"
    ok "Python smoke-test passed"
fi

echo ""
ok "Done. Dev site at: ${SITE_CLI_DIR}"
