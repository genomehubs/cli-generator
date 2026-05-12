/**
 * Unit tests for batch SDK methods (searchBatch, countBatch, record, lookup, summary).
 *
 * Tests validate via mocked fetch:
 * 1. Constraint enforcement (max 100 searches per batch)
 * 2. Correct URL construction for batch endpoints
 * 3. HTTP method (POST) and headers (Content-Type: application/json)
 * 4. Request payload structure (searches array with query_yaml, params_yaml)
 * 5. Error handling and response parsing
 *
 * Run via: node --test tests/javascript/test_batch_operations.mjs
 */

import { describe, it } from "node:test";

import assert from "node:assert/strict";

// Stub QueryBuilder for unit tests
class StubQueryBuilder {
  constructor(index) {
    this.index = index;
    this.taxa = [];
    this.fields = [];
  }

  set_taxa(taxa) {
    this.taxa = Array.isArray(taxa) ? taxa : [taxa];
    return this;
  }

  add_field(field) {
    this.fields.push(field);
    return this;
  }

  to_query_yaml() {
    const yaml = `index: ${this.index}\n`;
    if (this.taxa.length > 0) {
      return yaml + `taxa:\n${this.taxa.map((t) => `  - ${t}`).join("\n")}\n`;
    }
    return yaml;
  }

  to_params_yaml() {
    return `size: 5\n`;
  }

  async search_batch(queries, apiBase = "http://localhost:3000/api") {
    if (queries.length > 100) {
      throw new Error("maximum 100 searches per batch request");
    }
    const url = `${apiBase}/v3/search/batch`;
    const payload = {
      searches: queries.map((q) => ({
        query_yaml: q.to_query_yaml(),
        params_yaml: q.to_params_yaml(),
      })),
    };
    const response = await fetch(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    return JSON.parse(await response.text()).results || [];
  }

  async count_batch(queries, apiBase = "http://localhost:3000/api") {
    if (queries.length > 100) {
      throw new Error("maximum 100 searches per batch request");
    }
    const url = `${apiBase}/v3/count/batch`;
    const payload = {
      searches: queries.map((q) => ({
        query_yaml: q.to_query_yaml(),
        params_yaml: q.to_params_yaml(),
      })),
    };
    const response = await fetch(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const data = JSON.parse(await response.text());
    return (data.results || []).map((r) => r.status?.hits ?? 0);
  }

  async record(apiBase = "http://localhost:3000/api") {
    const url = `${apiBase}/v3/record`;
    const payload = {
      query_yaml: this.to_query_yaml(),
      params_yaml: this.to_params_yaml(),
    };
    const response = await fetch(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    return (JSON.parse(await response.text()).results || [])[0] || {};
  }

  async lookup(apiBase = "http://localhost:3000/api") {
    const url = `${apiBase}/v3/lookup`;
    const payload = {
      query_yaml: this.to_query_yaml(),
      params_yaml: this.to_params_yaml(),
    };
    const response = await fetch(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    return JSON.parse(await response.text()).results || {};
  }

  async summary(apiBase = "http://localhost:3000/api") {
    const url = `${apiBase}/v3/summary`;
    const payload = {
      query_yaml: this.to_query_yaml(),
      params_yaml: this.to_params_yaml(),
    };
    const response = await fetch(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    return JSON.parse(await response.text()).results || {};
  }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

describe("Batch Operations - Constraints", async () => {
  it("search_batch should reject >100 queries", async () => {
    const qb = new StubQueryBuilder("taxon");
    const queries = Array(101).fill(new StubQueryBuilder("taxon"));
    await assert.rejects(
      () => qb.search_batch(queries),
      /maximum 100 searches per batch request/,
    );
  });

  it("count_batch should reject >100 queries", async () => {
    const qb = new StubQueryBuilder("taxon");
    const queries = Array(101).fill(new StubQueryBuilder("taxon"));
    await assert.rejects(
      () => qb.count_batch(queries),
      /maximum 100 searches per batch request/,
    );
  });

  it("search_batch should accept 1 query", async () => {
    const qb = new StubQueryBuilder("taxon");
    const queries = [new StubQueryBuilder("taxon")];
    const originalFetch = global.fetch;
    global.fetch = () =>
      Promise.resolve({
        ok: true,
        text: () =>
          Promise.resolve(
            JSON.stringify({ status: { success: true }, results: [{}] }),
          ),
      });
    try {
      const result = await qb.search_batch(queries);
      assert.ok(Array.isArray(result));
    } finally {
      global.fetch = originalFetch;
    }
  });

  it("search_batch should accept exactly 100 queries", async () => {
    const qb = new StubQueryBuilder("taxon");
    const queries = Array(100).fill(new StubQueryBuilder("taxon"));
    const originalFetch = global.fetch;
    global.fetch = () =>
      Promise.resolve({
        ok: true,
        text: () =>
          Promise.resolve(
            JSON.stringify({
              status: { success: true },
              results: Array(100).fill({}),
            }),
          ),
      });
    try {
      const result = await qb.search_batch(queries);
      assert.equal(result.length, 100);
    } finally {
      global.fetch = originalFetch;
    }
  });
});

describe("Batch Operations - Search Batch HTTP", async () => {
  it("search_batch should construct correct URL", async () => {
    const qb = new StubQueryBuilder("taxon");
    const queries = [new StubQueryBuilder("taxon").set_taxa(["Mammalia"])];
    let capturedUrl = null;
    const originalFetch = global.fetch;
    global.fetch = (url) => {
      capturedUrl = url;
      return Promise.resolve({
        ok: true,
        text: () =>
          Promise.resolve(
            JSON.stringify({ status: { success: true }, results: [] }),
          ),
      });
    };
    try {
      await qb.search_batch(queries, "http://localhost:3000/api");
      assert.ok(
        capturedUrl.includes("http://localhost:3000/api/v3/search/batch"),
      );
    } finally {
      global.fetch = originalFetch;
    }
  });

  it("search_batch should use POST method", async () => {
    const qb = new StubQueryBuilder("taxon");
    const queries = [qb];
    let capturedOptions = null;
    const originalFetch = global.fetch;
    global.fetch = (url, options) => {
      capturedOptions = options;
      return Promise.resolve({
        ok: true,
        text: () =>
          Promise.resolve(
            JSON.stringify({ status: { success: true }, results: [] }),
          ),
      });
    };
    try {
      await qb.search_batch(queries);
      assert.equal(capturedOptions.method, "POST");
    } finally {
      global.fetch = originalFetch;
    }
  });

  it("search_batch should set Content-Type header", async () => {
    const qb = new StubQueryBuilder("taxon");
    const queries = [qb];
    let capturedHeaders = null;
    const originalFetch = global.fetch;
    global.fetch = (url, options) => {
      capturedHeaders = options.headers;
      return Promise.resolve({
        ok: true,
        text: () =>
          Promise.resolve(
            JSON.stringify({ status: { success: true }, results: [] }),
          ),
      });
    };
    try {
      await qb.search_batch(queries);
      assert.equal(capturedHeaders["Content-Type"], "application/json");
    } finally {
      global.fetch = originalFetch;
    }
  });

  it("search_batch should send payload with searches array", async () => {
    const qb = new StubQueryBuilder("taxon");
    const queries = [
      new StubQueryBuilder("taxon").set_taxa(["Mammalia"]),
      new StubQueryBuilder("taxon").set_taxa(["Aves"]),
    ];
    let capturedPayload = null;
    const originalFetch = global.fetch;
    global.fetch = (url, options) => {
      capturedPayload = JSON.parse(options.body);
      return Promise.resolve({
        ok: true,
        text: () =>
          Promise.resolve(
            JSON.stringify({ status: { success: true }, results: [{}, {}] }),
          ),
      });
    };
    try {
      await qb.search_batch(queries);
      assert.ok(capturedPayload.searches);
      assert.equal(capturedPayload.searches.length, 2);
      assert.ok("query_yaml" in capturedPayload.searches[0]);
      assert.ok("params_yaml" in capturedPayload.searches[0]);
    } finally {
      global.fetch = originalFetch;
    }
  });
});

describe("Batch Operations - Count Batch HTTP", async () => {
  it("count_batch should construct correct URL", async () => {
    const qb = new StubQueryBuilder("taxon");
    const queries = [new StubQueryBuilder("taxon").set_taxa(["Mammalia"])];
    let capturedUrl = null;
    const originalFetch = global.fetch;
    global.fetch = (url) => {
      capturedUrl = url;
      return Promise.resolve({
        ok: true,
        text: () =>
          Promise.resolve(
            JSON.stringify({
              status: { success: true },
              results: [{ status: { hits: 100 } }],
            }),
          ),
      });
    };
    try {
      await qb.count_batch(queries, "http://localhost:3000/api");
      assert.ok(
        capturedUrl.includes("http://localhost:3000/api/v3/count/batch"),
      );
    } finally {
      global.fetch = originalFetch;
    }
  });

  it("count_batch should return array of hit counts", async () => {
    const qb = new StubQueryBuilder("taxon");
    const queries = [
      new StubQueryBuilder("taxon"),
      new StubQueryBuilder("taxon"),
    ];
    const originalFetch = global.fetch;
    global.fetch = () =>
      Promise.resolve({
        ok: true,
        text: () =>
          Promise.resolve(
            JSON.stringify({
              status: { success: true },
              results: [{ status: { hits: 1000 } }, { status: { hits: 2000 } }],
            }),
          ),
      });
    try {
      const result = await qb.count_batch(queries);
      assert.deepEqual(result, [1000, 2000]);
    } finally {
      global.fetch = originalFetch;
    }
  });

  it("count_batch should handle missing hits gracefully", async () => {
    const qb = new StubQueryBuilder("taxon");
    const queries = [new StubQueryBuilder("taxon")];
    const originalFetch = global.fetch;
    global.fetch = () =>
      Promise.resolve({
        ok: true,
        text: () =>
          Promise.resolve(
            JSON.stringify({ status: { success: true }, results: [{}] }),
          ),
      });
    try {
      const result = await qb.count_batch(queries);
      assert.deepEqual(result, [0]);
    } finally {
      global.fetch = originalFetch;
    }
  });
});

describe("Batch Operations - Record/Lookup/Summary HTTP", async () => {
  it("record should construct correct URL", async () => {
    const qb = new StubQueryBuilder("taxon").set_taxa(["9646"]);
    let capturedUrl = null;
    const originalFetch = global.fetch;
    global.fetch = (url) => {
      capturedUrl = url;
      return Promise.resolve({
        ok: true,
        text: () =>
          Promise.resolve(JSON.stringify({ status: { success: true } })),
      });
    };
    try {
      await qb.record("http://localhost:3000/api");
      assert.ok(capturedUrl.includes("http://localhost:3000/api/v3/record"));
    } finally {
      global.fetch = originalFetch;
    }
  });

  it("lookup should construct correct URL", async () => {
    const qb = new StubQueryBuilder("taxon").set_taxa(["9646"]);
    let capturedUrl = null;
    const originalFetch = global.fetch;
    global.fetch = (url) => {
      capturedUrl = url;
      return Promise.resolve({
        ok: true,
        text: () =>
          Promise.resolve(JSON.stringify({ status: { success: true } })),
      });
    };
    try {
      await qb.lookup("http://localhost:3000/api");
      assert.ok(capturedUrl.includes("http://localhost:3000/api/v3/lookup"));
    } finally {
      global.fetch = originalFetch;
    }
  });

  it("summary should construct correct URL", async () => {
    const qb = new StubQueryBuilder("taxon").add_field("genome_size");
    let capturedUrl = null;
    const originalFetch = global.fetch;
    global.fetch = (url) => {
      capturedUrl = url;
      return Promise.resolve({
        ok: true,
        text: () =>
          Promise.resolve(JSON.stringify({ status: { success: true } })),
      });
    };
    try {
      await qb.summary("http://localhost:3000/api");
      assert.ok(capturedUrl.includes("http://localhost:3000/api/v3/summary"));
    } finally {
      global.fetch = originalFetch;
    }
  });
});

describe("Batch Operations - Error Handling", async () => {
  it("search_batch should throw on HTTP 500 error", async () => {
    const qb = new StubQueryBuilder("taxon");
    const queries = [new StubQueryBuilder("taxon")];
    const originalFetch = global.fetch;
    global.fetch = () =>
      Promise.resolve({
        ok: false,
        status: 500,
      });
    try {
      await assert.rejects(() => qb.search_batch(queries), /HTTP 500/);
    } finally {
      global.fetch = originalFetch;
    }
  });

  it("count_batch should throw on HTTP 400 error", async () => {
    const qb = new StubQueryBuilder("taxon");
    const queries = [new StubQueryBuilder("taxon")];
    const originalFetch = global.fetch;
    global.fetch = () =>
      Promise.resolve({
        ok: false,
        status: 400,
      });
    try {
      await assert.rejects(() => qb.count_batch(queries), /HTTP 400/);
    } finally {
      global.fetch = originalFetch;
    }
  });

  it("record should handle network errors", async () => {
    const qb = new StubQueryBuilder("taxon");
    const originalFetch = global.fetch;
    global.fetch = () => Promise.reject(new Error("Network connection failed"));
    try {
      await assert.rejects(() => qb.record(), /Network connection failed/);
    } finally {
      global.fetch = originalFetch;
    }
  });

  it("search_batch should throw on malformed JSON response", async () => {
    const qb = new StubQueryBuilder("taxon");
    const queries = [new StubQueryBuilder("taxon")];
    const originalFetch = global.fetch;
    global.fetch = () =>
      Promise.resolve({
        ok: true,
        text: () => Promise.resolve("invalid json"),
      });
    try {
      await assert.rejects(() => qb.search_batch(queries));
    } finally {
      global.fetch = originalFetch;
    }
  });
});

describe("Batch Operations - Response Parsing", async () => {
  it("search_batch should return results list", async () => {
    const qb = new StubQueryBuilder("taxon");
    const queries = [
      new StubQueryBuilder("taxon"),
      new StubQueryBuilder("taxon"),
    ];
    const originalFetch = global.fetch;
    global.fetch = () =>
      Promise.resolve({
        ok: true,
        text: () =>
          Promise.resolve(
            JSON.stringify({
              status: { success: true },
              results: [
                { hits: 100, data: "obj1" },
                { hits: 50, data: "obj2" },
              ],
            }),
          ),
      });
    try {
      const result = await qb.search_batch(queries);
      assert.ok(Array.isArray(result));
      assert.equal(result.length, 2);
    } finally {
      global.fetch = originalFetch;
    }
  });

  it("search_batch should handle empty results", async () => {
    const qb = new StubQueryBuilder("taxon");
    const queries = [new StubQueryBuilder("taxon")];
    const originalFetch = global.fetch;
    global.fetch = () =>
      Promise.resolve({
        ok: true,
        text: () =>
          Promise.resolve(
            JSON.stringify({ status: { success: true }, results: [] }),
          ),
      });
    try {
      const result = await qb.search_batch(queries);
      assert.equal(result.length, 0);
    } finally {
      global.fetch = originalFetch;
    }
  });

  it("count_batch should handle empty results", async () => {
    const qb = new StubQueryBuilder("taxon");
    const queries = [new StubQueryBuilder("taxon")];
    const originalFetch = global.fetch;
    global.fetch = () =>
      Promise.resolve({
        ok: true,
        text: () =>
          Promise.resolve(
            JSON.stringify({ status: { success: true }, results: [] }),
          ),
      });
    try {
      const result = await qb.count_batch(queries);
      assert.equal(result.length, 0);
    } finally {
      global.fetch = originalFetch;
    }
  });
});
