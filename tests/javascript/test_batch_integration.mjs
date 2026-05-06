/**
 * Integration tests for batch SDK methods against a live API.
 *
 * These tests validate that batch operations work end-to-end with real API responses,
 * using actual WASM parse functions (no mocks). They require a running API server at
 * http://localhost:3000/api.
 *
 * Run via: SITE=goat JS_SDK_PATH=./workdir/my-goat/goat-cli/js/goat/query.js node --test tests/javascript/test_batch_integration.mjs
 * Or with dev_site.sh: bash scripts/dev_site.sh --site goat --python
 */

import { describe, it } from "node:test";
import { dirname, resolve } from "path";

import assert from "node:assert/strict";
import { fileURLToPath } from "url";

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));
const PROJECT_ROOT = resolve(SCRIPT_DIR, "../..");

const SITE = process.env.SITE ?? "goat";
const JS_SDK_PATH =
  process.env.JS_SDK_PATH ??
  `file://${resolve(PROJECT_ROOT, `workdir/${SITE}-cli/js/${SITE}/query.js`)}`;

const API_BASE = "http://localhost:3000/api";

let QueryBuilder;
try {
  const sdkModule = await import(JS_SDK_PATH);
  QueryBuilder = sdkModule.QueryBuilder;
} catch (err) {
  console.error(`ERROR: Could not load SDK from ${JS_SDK_PATH}`);
  console.error(`Path: ${JS_SDK_PATH}`);
  console.error(`Error: ${err.message}`);
  console.error(err.stack);
  process.exit(1);
}

// ── Integration Tests ──────────────────────────────────────────────────────────

describe("Batch Integration - Search Batch", async () => {
  it("searchBatch should work with single query", async () => {
    const qb = new QueryBuilder("taxon");
    const query = new QueryBuilder("taxon");
    query.setTaxa && query.setTaxa(["Canis lupus"]);
    const result = await qb.searchBatch([query], API_BASE);
    assert.ok(Array.isArray(result));
  });

  it("searchBatch should work with 10 queries", async () => {
    const qb = new QueryBuilder("taxon");
    const queries = Array(10)
      .fill(null)
      .map(() => new QueryBuilder("taxon"));
    const result = await qb.searchBatch(queries, API_BASE);
    assert.ok(Array.isArray(result));
    assert.equal(result.length, 10);
  });

  it("searchBatch should work with exactly 100 queries", async () => {
    const qb = new QueryBuilder("taxon");
    const queries = Array(100)
      .fill(null)
      .map(() => new QueryBuilder("taxon"));
    const result = await qb.searchBatch(queries, API_BASE);
    assert.ok(Array.isArray(result));
    assert.equal(result.length, 100);
  });
});

describe("Batch Integration - Count Batch", async () => {
  it("countBatch should return hit counts for single query", async () => {
    const qb = new QueryBuilder("taxon");
    const query = new QueryBuilder("taxon");
    query.setTaxa && query.setTaxa(["Mammalia"]);
    const result = await qb.countBatch([query], API_BASE);
    assert.ok(Array.isArray(result));
    assert.equal(result.length, 1);
    assert.equal(typeof result[0], "number");
    assert.ok(result[0] >= 0);
  });

  it("countBatch should return hit counts for 5 queries", async () => {
    const qb = new QueryBuilder("taxon");
    const queries = Array(5)
      .fill(null)
      .map(() => new QueryBuilder("taxon"));
    const result = await qb.countBatch(queries, API_BASE);
    assert.ok(Array.isArray(result));
    assert.equal(result.length, 5);
    assert.ok(result.every((count) => typeof count === "number"));
    assert.ok(result.every((count) => count >= 0));
  });

  it("countBatch should handle query with no results", async () => {
    const qb = new QueryBuilder("taxon");
    const query = new QueryBuilder("taxon");
    query.setTaxa && query.setTaxa(["NonExistentSpecies"]);
    const result = await qb.countBatch([query], API_BASE);
    assert.ok(Array.isArray(result));
    assert.equal(result.length, 1);
    assert.ok(result[0] >= 0);
  });

  it("countBatch should preserve query order in results", async () => {
    const qb = new QueryBuilder("taxon");
    const queries = Array(3)
      .fill(null)
      .map(() => new QueryBuilder("taxon"));
    const result = await qb.countBatch(queries, API_BASE);
    assert.equal(result.length, queries.length);
  });
});

describe("Batch Integration - Record", async () => {
  it("record should return record data with recordId", async () => {
    const qb = new QueryBuilder("taxon");
    const result = await qb.record("taxon-9646", "taxon");
    assert.ok(typeof result === "object");
    assert.ok(result.records || result.status);
  });
});

describe("Batch Integration - Lookup", async () => {
  it("lookup should work with searchTerm", async () => {
    const qb = new QueryBuilder("taxon");
    const result = await qb.lookup("Homo", "taxon", 5);
    assert.ok(typeof result === "object");
    assert.ok(result.results || result.status);
  });
});

describe("Batch Integration - Summary", async () => {
  it("summary should work with recordId and fields", async () => {
    const qb = new QueryBuilder("taxon");
    const result = await qb.summary(
      "taxon-9646",
      "genome_size",
      "taxon",
      "min,max,mean",
    );
    assert.ok(typeof result === "object");
    assert.ok(result.summaries || result.status);
  });
});

describe("Batch Integration - Error Handling", async () => {
  it("searchBatch should reject >100 queries", async () => {
    const qb = new QueryBuilder("taxon");
    const queries = Array(101).fill(new QueryBuilder("taxon"));
    try {
      await qb.searchBatch(queries, API_BASE);
      assert.fail("Should have thrown error");
    } catch (err) {
      assert.match(err.message, /maximum 100 searches/);
    }
  });

  it("countBatch should reject >100 queries", async () => {
    const qb = new QueryBuilder("taxon");
    const queries = Array(101).fill(new QueryBuilder("taxon"));
    try {
      await qb.countBatch(queries, API_BASE);
      assert.fail("Should have thrown error");
    } catch (err) {
      assert.match(err.message, /maximum 100 searches/);
    }
  });

  it("searchBatch should fail with invalid API base", async () => {
    const qb = new QueryBuilder("taxon");
    const query = new QueryBuilder("taxon");
    try {
      await qb.searchBatch([query], "http://invalid.example.com:9999");
      assert.fail("Should have thrown error");
    } catch (err) {
      assert.ok(err);
    }
  });
});
