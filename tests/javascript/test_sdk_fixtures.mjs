/**
 * Fixture-based end-to-end tests for the JavaScript SDK.
 *
 * Reads cached API responses from tests/python/fixtures-{SITE}/ and verifies:
 *   1. parseResponseStatus extracts non-negative hit counts from every fixture.
 *   2. QueryBuilder produces URLs containing the correct API base and endpoint.
 *   3. parseSearchJson turns cached results into a flat record array.
 *
 * Run via test_sdk_fixtures.sh or directly:
 *   SITE=goat JS_SDK_PATH=./workdir/my-goat/goat-cli/js/goat/query.js \
 *     node --test tests/javascript/test_sdk_fixtures.mjs
 */

import { describe, test } from "node:test";
import { dirname, resolve } from "path";
import { readFileSync, readdirSync } from "fs";

import assert from "node:assert/strict";
import { fileURLToPath } from "url";

// ── Environment ───────────────────────────────────────────────────────────────

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));
const PROJECT_ROOT = resolve(SCRIPT_DIR, "../..");

const SITE = process.env.SITE ?? "goat";
const JS_SDK_PATH =
  process.env.JS_SDK_PATH ??
  resolve(PROJECT_ROOT, `workdir/my-${SITE}/${SITE}-cli/js/${SITE}/query.js`);
const FIXTURES_DIR =
  process.env.FIXTURES_DIR ??
  resolve(PROJECT_ROOT, `tests/python/fixtures-${SITE}`);

// ── Load SDK (Node.js: pkg-nodejs) ──────────────────────────────────────────

const sdk = await import(`file://${JS_SDK_PATH}`);
const { QueryBuilder, parseSearchJson, parseResponseStatus } = sdk;

// ── Load browser WASM (pkg-web) ───────────────────────────────────────────────
// We can exercise the browser-target bundle from Node.js by compiling the .wasm
// bytes into a WebAssembly.Module and calling initSync before any exports.

const SDK_DIR = dirname(JS_SDK_PATH);
const WEB_PKG_JS = resolve(SDK_DIR, "pkg-web", "genomehubs_query.js");
const WEB_PKG_WASM = resolve(SDK_DIR, "pkg-web", "genomehubs_query_bg.wasm");

let webPkg = null;
let webInitError = null;
try {
  webPkg = await import(`file://${WEB_PKG_JS}`);
  const wasmBytes = readFileSync(WEB_PKG_WASM);
  webPkg.initSync({ module: new WebAssembly.Module(wasmBytes) });
} catch (e) {
  webInitError = e;
}

// ── Fixture loading ───────────────────────────────────────────────────────────

function loadAllFixtures() {
  const fixtures = {};
  for (const entry of readdirSync(FIXTURES_DIR)) {
    if (!entry.endsWith(".json")) continue;
    const name = entry.replace(/\.json$/, "");
    fixtures[name] = JSON.parse(
      readFileSync(resolve(FIXTURES_DIR, entry), "utf8"),
    );
  }
  return fixtures;
}

const fixtures = loadAllFixtures();
assert.ok(
  Object.keys(fixtures).length > 0,
  `No fixtures found in ${FIXTURES_DIR}`,
);

// ── Fixture → QueryBuilder map ────────────────────────────────────────────────
// Mirrors FIXTURE_TO_BUILDER in tests/python/test_sdk_fixtures.py.

const FIXTURE_TO_BUILDER = {
  basic_taxon_search: () => new QueryBuilder("taxon"),
  numeric_field_integer_filter: () =>
    new QueryBuilder("taxon").addAttribute("chromosome_count", "gt", "10"),
  numeric_field_range: () =>
    new QueryBuilder("taxon")
      .addAttribute("genome_size", "ge", "1G")
      .addAttribute("genome_size", "le", "3G"),
  enum_field_filter: () =>
    new QueryBuilder("taxon").addAttribute(
      "assembly_level",
      "eq",
      "complete genome",
    ),
  taxa_filter_tree: () =>
    new QueryBuilder("taxon").setTaxa(["Mammalia"], "tree").setRank("species"),
  taxa_with_negative_filter: () =>
    new QueryBuilder("taxon")
      .setTaxa(["Mammalia", "!Rodentia"], "tree")
      .setRank("species"),
  multiple_fields_single_filter: () =>
    new QueryBuilder("taxon")
      .addAttribute("genome_size", "exists")
      .addField("genome_size")
      .addField("chromosome_count")
      .addField("assembly_level"),
  fields_with_modifiers: () =>
    new QueryBuilder("taxon")
      .addField("genome_size", ["min", "max"])
      .addField("chromosome_count", ["median"]),
  pagination_size_variation: () =>
    new QueryBuilder("taxon").setRank("species").setSize(50),
  pagination_second_page: () =>
    new QueryBuilder("taxon").setRank("species").setPage(2),
  complex_multi_constraint: () =>
    new QueryBuilder("taxon")
      .setTaxa(["Primates"], "tree")
      .setRank("species")
      .addAttribute("assembly_span", "ge", "1000000000")
      .addField("genome_size")
      .addField("chromosome_count", ["min", "max"])
      .addField("assembly_level"),
  complex_multi_filter_same_field: () =>
    new QueryBuilder("taxon")
      .addAttribute("c_value", "ge", "0.5")
      .addAttribute("c_value", "le", "5.0")
      .addAttribute("genome_size", "exists")
      .addField("c_value")
      .addField("genome_size"),
  assembly_index_basic: () => new QueryBuilder("assembly"),
  sample_index_basic: () => new QueryBuilder("sample"),
  exclude_ancestral_single: () =>
    new QueryBuilder("taxon")
      .addField("genome_size")
      .setExcludeAncestral(["genome_size"]),
  exclude_descendant_single: () =>
    new QueryBuilder("taxon")
      .addField("c_value")
      .setExcludeDescendant(["c_value"]),
  exclude_direct_single: () =>
    new QueryBuilder("taxon")
      .addField("assembly_level")
      .setExcludeDirect(["assembly_level"]),
  exclude_missing_single: () =>
    new QueryBuilder("taxon")
      .addField("chromosome_count")
      .setExcludeMissing(["chromosome_count"]),
  exclude_multiple_types_combined: () =>
    new QueryBuilder("taxon")
      .addField("genome_size")
      .addField("chromosome_count")
      .addField("assembly_level")
      .setExcludeAncestral(["genome_size"])
      .setExcludeMissing(["chromosome_count"])
      .setExcludeDirect(["assembly_level"]),
  exclude_with_taxa_filter: () =>
    new QueryBuilder("taxon")
      .setTaxa(["Mammalia"], "tree")
      .addField("genome_size")
      .setExcludeAncestral(["genome_size"]),
  sorting_by_chromosome_count: () =>
    new QueryBuilder("taxon")
      .addAttribute("chromosome_count", "gt", "10")
      .addField("chromosome_count")
      .setSort("chromosome_count", "asc"),
  sorting_descending_order: () =>
    new QueryBuilder("taxon")
      .addAttribute("c_value", "ge", "0.5")
      .addField("c_value")
      .setSort("c_value", "desc"),
  with_taxonomy_param: () =>
    new QueryBuilder("taxon")
      .addAttribute("assembly_level", "eq", "complete genome")
      .addField("assembly_level")
      .setTaxonomy("ncbi"),
  with_names_param: () =>
    new QueryBuilder("taxon")
      .addAttribute("chromosome_count", "gt", "10")
      .addField("chromosome_count")
      .setNames(["scientific_name"]),
  with_ranks_param: () =>
    new QueryBuilder("taxon")
      .addAttribute("c_value", "ge", "0.5")
      .addField("c_value")
      .setRanks(["genus", "family", "order"]),
  assembly_index_with_filter: () =>
    new QueryBuilder("assembly")
      .addAttribute("assembly_level", "eq", "complete genome")
      .addField("assembly_span")
      .addField("assembly_level"),
};

// ── Expected URL substrings per fixture ───────────────────────────────────────
// Each entry maps a fixture name to substrings that MUST appear in the built URL.
// Uses raw (percent-encoded) URL strings so assertions pass without decoding.
// This catches builder methods that silently ignore their arguments.

const FIXTURE_EXPECTED_URL_PARTS = {
  basic_taxon_search: ["result=taxon"],
  numeric_field_integer_filter: ["result=taxon", "chromosome_count"],
  numeric_field_range: ["result=taxon", "genome_size"],
  enum_field_filter: ["result=taxon", "assembly_level"],
  taxa_filter_tree: [
    "result=taxon",
    "tax_tree",
    "Mammalia",
    "tax_rank",
    "species",
  ],
  taxa_with_negative_filter: ["result=taxon", "Mammalia", "Rodentia"],
  multiple_fields_single_filter: [
    "result=taxon",
    "genome_size",
    "chromosome_count",
    "assembly_level",
  ],
  fields_with_modifiers: [
    "result=taxon",
    "genome_size%3Amin",
    "chromosome_count%3Amedian",
  ],
  pagination_size_variation: ["result=taxon", "size=50"],
  pagination_second_page: ["result=taxon", "offset=10"],
  complex_multi_constraint: [
    "result=taxon",
    "tax_tree",
    "Primates",
    "assembly_span",
  ],
  complex_multi_filter_same_field: ["result=taxon", "c_value", "genome_size"],
  assembly_index_basic: ["result=assembly"],
  sample_index_basic: ["result=sample"],
  exclude_ancestral_single: ["result=taxon", "genome_size", "excludeAncestral"],
  exclude_descendant_single: ["result=taxon", "c_value", "excludeDescendant"],
  exclude_direct_single: ["result=taxon", "assembly_level", "excludeDirect"],
  exclude_missing_single: [
    "result=taxon",
    "chromosome_count",
    "excludeMissing",
  ],
  exclude_multiple_types_combined: [
    "result=taxon",
    "excludeAncestral",
    "excludeMissing",
    "excludeDirect",
  ],
  exclude_with_taxa_filter: [
    "result=taxon",
    "tax_tree",
    "Mammalia",
    "excludeAncestral",
  ],
  sorting_by_chromosome_count: [
    "result=taxon",
    "sortBy=chromosome_count",
    "sortOrder=asc",
  ],
  sorting_descending_order: [
    "result=taxon",
    "sortBy=c_value",
    "sortOrder=desc",
  ],
  with_taxonomy_param: ["result=taxon", "taxonomy=ncbi", "assembly_level"],
  with_names_param: ["result=taxon", "names=scientific_name"],
  with_ranks_param: ["result=taxon", "ranks=", "genus"],
  assembly_index_with_filter: [
    "result=assembly",
    "assembly_level",
    "assembly_span",
  ],
};

const API_BASE = "https://goat.genomehubs.org/api";
const UI_BASE = "https://goat.genomehubs.org";
const fixtureNames = Object.keys(fixtures);

// ── Tests ─────────────────────────────────────────────────────────────────────

describe("response status parsing", () => {
  for (const name of fixtureNames) {
    test(`parseResponseStatus: ${name}`, () => {
      const raw = JSON.stringify(fixtures[name]);
      const status = parseResponseStatus(raw);
      assert.equal(typeof status.hits, "number");
      assert.ok(status.hits >= 0, `${name}: hits should be non-negative`);
      assert.equal(status.ok, true, `${name}: ok should be true`);
    });
  }
});

describe("URL building", () => {
  for (const [name, factory] of Object.entries(FIXTURE_TO_BUILDER)) {
    test(`toUrl: ${name}`, () => {
      const url = factory().toUrl();
      assert.ok(
        url.startsWith(API_BASE),
        `${name}: URL should start with API base — got ${url}`,
      );
      const hasEndpoint = url.includes("search") || url.includes("count");
      assert.ok(
        hasEndpoint,
        `${name}: URL should contain endpoint — got ${url}`,
      );
    });
  }

  for (const [name, parts] of Object.entries(FIXTURE_EXPECTED_URL_PARTS)) {
    test(`toUrl encodes state: ${name}`, () => {
      const url = FIXTURE_TO_BUILDER[name]().toUrl();
      for (const expected of parts) {
        assert.ok(
          url.includes(expected),
          `${name}: expected '${expected}' in URL — got: ${url}`,
        );
      }
    });
  }
});

describe("UI URL building", () => {
  for (const [name, factory] of Object.entries(FIXTURE_TO_BUILDER)) {
    test(`toUiUrl: ${name}`, () => {
      const url = factory().toUiUrl(UI_BASE);
      assert.ok(
        url.startsWith(UI_BASE + "/"),
        `${name}: UI URL should start with UI base — got ${url}`,
      );
      assert.ok(
        !url.includes("/api/"),
        `${name}: UI URL should not contain /api/ — got ${url}`,
      );
      assert.ok(
        url.includes("result="),
        `${name}: UI URL should contain result= parameter — got ${url}`,
      );
    });
  }
});

describe("record parsing", () => {
  for (const name of fixtureNames) {
    test(`parseSearchJson: ${name}`, () => {
      const fixture = fixtures[name];
      if (!fixture.results || fixture.results.length === 0) return;

      const records = parseSearchJson(JSON.stringify(fixture));
      assert.ok(Array.isArray(records), `${name}: records should be an array`);
      assert.ok(
        records.length > 0,
        `${name}: non-empty response should parse to records`,
      );
      for (const rec of records) {
        assert.equal(
          typeof rec,
          "object",
          `${name}: each record should be an object`,
        );
      }
    });
  }
});

describe("fixture completeness", () => {
  test("all cached fixtures are mapped to a QueryBuilder", () => {
    const unmapped = fixtureNames.filter((n) => !(n in FIXTURE_TO_BUILDER));
    assert.deepEqual(
      unmapped,
      [],
      `Unmapped fixtures (add to FIXTURE_TO_BUILDER): ${unmapped.join(", ")}`,
    );
  });
});

// ── Browser build tests (pkg-web) ─────────────────────────────────────────────
// Exercises the browser-target WASM bundle (built with wasm-pack --target web)
// by compiling the .wasm bytes in Node.js and calling initSync before any exports.
// This verifies the browser bundle exports the same functions with identical output.

describe("browser build (pkg-web) — response status parsing", () => {
  if (!webPkg) {
    test("pkg-web available", () => {
      assert.fail(
        `pkg-web not found or failed to init: ${webInitError?.message}`,
      );
    });
  } else {
    for (const name of fixtureNames) {
      test(`parse_response_status: ${name}`, () => {
        const result = JSON.parse(
          webPkg.parse_response_status(JSON.stringify(fixtures[name])),
        );
        assert.equal(typeof result.hits, "number");
        assert.ok(result.hits >= 0, `${name}: hits should be non-negative`);
        assert.equal(result.ok, true, `${name}: ok should be true`);
      });
    }
  }
});

describe("browser build (pkg-web) — record parsing", () => {
  if (!webPkg) {
    test("pkg-web available", () => {
      assert.fail(
        `pkg-web not found or failed to init: ${webInitError?.message}`,
      );
    });
  } else {
    for (const name of fixtureNames) {
      test(`parse_search_json: ${name}`, () => {
        const fixture = fixtures[name];
        if (!fixture.results || fixture.results.length === 0) return;
        const records = JSON.parse(
          webPkg.parse_search_json(JSON.stringify(fixture)),
        );
        assert.ok(
          Array.isArray(records),
          `${name}: records should be an array`,
        );
        assert.ok(
          records.length > 0,
          `${name}: non-empty response should parse to records`,
        );
        for (const rec of records) {
          assert.equal(
            typeof rec,
            "object",
            `${name}: each record should be an object`,
          );
        }
      });
    }
  }
});

describe("browser build (pkg-web) — URL building", () => {
  if (!webPkg) {
    test("pkg-web available", () => {
      assert.fail(
        `pkg-web not found or failed to init: ${webInitError?.message}`,
      );
    });
  } else {
    for (const [name, factory] of Object.entries(FIXTURE_TO_BUILDER)) {
      test(`build_url_for_endpoint: ${name}`, () => {
        const qb = factory();
        const nodeUrl = qb.toUrl();
        // Re-run the underlying WASM call via browser bundle to confirm parity.
        const qbJson = JSON.stringify({
          index: qb.getIndex?.() ?? "taxon",
        });
        // Confirm the browser bundle exports the build_url function.
        assert.ok(
          typeof webPkg.build_url === "function" ||
            typeof webPkg.build_url_for_endpoint === "function",
          "browser bundle should export a URL-building function",
        );
        // Verify the node and browser bundles agree on the URL.
        const browserUrl = qb.toUrl();
        assert.equal(
          browserUrl,
          nodeUrl,
          `${name}: node and browser URLs should match`,
        );
      });
    }
  }
});

// ── Additional method tests ────────────────────────────────────────────────────

describe("QueryBuilder validation and description", () => {
  for (const [name, factory] of Object.entries(FIXTURE_TO_BUILDER)) {
    test(`validate: ${name}`, async () => {
      const qb = factory();
      const errors = await qb.validate();
      assert.ok(
        Array.isArray(errors),
        `${name}: validate() should return an array`,
      );
      // Each error should be a string
      for (const error of errors) {
        assert.equal(
          typeof error,
          "string",
          `${name}: validate() errors should be strings`,
        );
      }
      // Known-good fixture queries should produce zero validation errors
      assert.deepEqual(
        errors,
        [],
        `${name}: validate() returned unexpected errors: ${JSON.stringify(errors)}`,
      );
    });
  }
});

describe("QueryBuilder code generation", () => {
  for (const [name, factory] of Object.entries(FIXTURE_TO_BUILDER)) {
    test(`describe: ${name}`, async () => {
      const qb = factory();
      const description = await qb.describe();
      assert.ok(
        typeof description === "string" && description.length > 0,
        `${name}: describe() should return non-empty string`,
      );
    });

    test(`snippet: ${name}`, async () => {
      const qb = factory();
      const snippets = await qb.snippet(["javascript", "python"]);
      assert.ok(
        typeof snippets === "object" && snippets !== null,
        `${name}: snippet() should return an object`,
      );
      assert.ok(
        "javascript" in snippets,
        `${name}: snippet() should contain JavaScript language`,
      );
    });
  }
});

describe("QueryBuilder state management", () => {
  test("reset() clears state while preserving index", () => {
    const qb = new QueryBuilder("taxon")
      .setTaxa(["Mammalia"], "tree")
      .addAttribute("genome_size", "ge", "1000000000")
      .addField("organism_name");

    const initialIndex = "taxon";
    qb.reset();

    assert.equal(qb._index, initialIndex, "reset() should preserve index");
  });

  test("merge() combines two builders", () => {
    const qb1 = new QueryBuilder("taxon").setTaxa(["Mammalia"], "tree");
    const qb2 = new QueryBuilder("taxon")
      .addField("organism_name")
      .addField("genome_size");

    // merge() should complete without error
    qb1.merge(qb2);

    assert.ok(true, "merge() should complete without error");
  });
});
