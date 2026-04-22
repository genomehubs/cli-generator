/**
 * QueryBuilder for {{ site_display_name }}
 *
 * Builds API queries programmatically with method chaining.
 * Works in Node.js (≥ 18). URL building is backed by a pre-compiled
 * WebAssembly module generated from the same Rust code as the Python SDK,
 * guaranteeing identical output across all language bindings.
 *
 * To rebuild the WASM module after updating cli-generator:
 *   bash build-wasm.sh
 */

const API_BASE = "{{ api_base_url }}";
const API_VERSION = "v2";
const UI_BASE = "{{ ui_base }}";

// Load the pre-compiled WASM module (Node.js)
// Use dynamic import to handle CommonJS exports from wasm-pack (Node.js target)
const wasmModule = await import("./pkg-nodejs/genomehubs_query.js");
const {
  annotate_source_labels: _annotateSourceLabels,
  annotated_values: _annotatedValues,
  build_ui_url: _buildUiUrl,
  build_url_for_endpoint: _buildUrlForEndpoint,
  describe_query: _describeQuery,
  parse_paginated_json: _parsePaginatedJson,
  parse_search_json: _parseSearchJson,
  render_snippet: _renderSnippet,
  split_source_columns: _splitSourceColumns,
  to_tidy_records: _toTidyRecords,
  validate_query_json: _validateQueryJson,
  values_only: _valuesOnly,
  build_url,
  parse_response_status,
  version,
} = wasmModule;

/** Flatten a raw API response string or object into an array of flat record objects. */
function parseSearchJson(raw) {
  const str = typeof raw === "string" ? raw : JSON.stringify(raw);
  return JSON.parse(_parseSearchJson(str));
}

/** Parse the status block from a raw API response. Returns `{hits, ok, error?}`. */
function parseResponseStatus(raw) {
  const str = typeof raw === "string" ? raw : JSON.stringify(raw);
  return JSON.parse(parse_response_status(str));
}

/** Add `{field}__label` columns. mode: "all" | "non_direct" | "ancestral_only" */
function annotateSourceLabels(records, mode = "non_direct") {
  const str = typeof records === "string" ? records : JSON.stringify(records);
  return JSON.parse(_annotateSourceLabels(str, mode));
}

/** Replace {field}/{field}__source pairs with {field}__direct/descendant/ancestral columns. */
function splitSourceColumns(records) {
  const str = typeof records === "string" ? records : JSON.stringify(records);
  return JSON.parse(_splitSourceColumns(str));
}

/**
 * Strip all __* sub-key columns, keeping identity and bare field values.
 *
 * @param {string|object[]} records - Flat records from parseSearchJson.
 * @param {string[]} [keepColumns] - __* column names to keep despite stripping,
 *   e.g. ["assembly_span__min"]. Use qb.fieldModifiers() to build this list.
 * @returns {object[]}
 */
function valuesOnly(records, keepColumns = []) {
  const str = typeof records === "string" ? records : JSON.stringify(records);
  return JSON.parse(_valuesOnly(str, JSON.stringify(keepColumns)));
}

/**
 * Replace non-direct field values with their annotated label string, then strip __* columns.
 *
 * @param {string|object[]} records - Flat records from parseSearchJson.
 * @param {string} [mode] - "all" | "non_direct" | "ancestral_only" (default: "non_direct")
 * @param {string[]} [keepColumns] - __* column names to keep after label promotion,
 *   e.g. ["assembly_span__min"]. Use qb.fieldModifiers() to build this list.
 * @returns {object[]}
 */
function annotatedValues(records, mode = "non_direct", keepColumns = []) {
  const str = typeof records === "string" ? records : JSON.stringify(records);
  return JSON.parse(_annotatedValues(str, mode, JSON.stringify(keepColumns)));
}

/**
 * Accumulates a genomehubs SearchQuery incrementally.
 *
 * @example
 * import { QueryBuilder } from "./query.js";
 * const qb = new QueryBuilder("taxon")
 *   .setTaxa(["Mammalia"], "tree")
 *   .addAttribute("genome_size", "ge", "1000000000")
 *   .addField("assembly_span")
 *   .setSize(10);
 * const url = qb.toUrl();
 */
class QueryBuilder {
  /**
   * @param {string} index - Index to query: "taxon", "assembly", "sample".
   * @param {Object} [options] - Optional configuration.
   * @param {string} [options.validationLevel="full"] - "full" (fetch from API) or "partial" (no API fetch).
   * @param {string} [options.apiBase="{{ api_base_url }}"] - Base URL for API metadata endpoints (v3+).
   */
  constructor(index, options = {}) {
    this._index = index;
    this._taxa = [];
    this._assemblies = [];
    this._samples = [];
    this._rank = null;
    this._taxonFilterType = "name";
    this._attributes = [];
    this._fields = [];
    this._names = [];
    this._ranks = [];
    this._excludeAncestral = [];
    this._excludeDescendant = [];
    this._excludeDirect = [];
    this._excludeMissing = [];
    // QueryParams
    this._size = 10;
    this._page = 1;
    this._sortBy = null;
    this._sortOrder = "asc";
    this._includeEstimates = true;
    this._tidy = false;
    this._taxonomy = "ncbi";
    // Validation options
    this._validationLevel = options.validationLevel || "full";
    this._apiBase = options.apiBase || API_BASE;
  }

  // ── Identifiers ────────────────────────────────────────────────────────────

  /**
   * Set the taxon filter.
   * @param {string[]} taxa - Taxon names or IDs. Prefix with "!" for exclusion.
   * @param {string} [filterType="name"] - One of "name", "tree", "lineage".
   * @returns {QueryBuilder}
   */
  setTaxa(taxa, filterType = "name") {
    this._taxa = [...taxa];
    this._taxonFilterType = filterType;
    return this;
  }

  /**
   * Restrict results to a single taxonomic rank, e.g. "species".
   * @param {string} rank
   * @returns {QueryBuilder}
   */
  setRank(rank) {
    this._rank = rank;
    return this;
  }

  /**
   * Filter by assembly accession IDs.
   * @param {string[]} accessions
   * @returns {QueryBuilder}
   */
  setAssemblies(accessions) {
    this._assemblies = [...accessions];
    return this;
  }

  /**
   * Filter by sample accession IDs.
   * @param {string[]} accessions
   * @returns {QueryBuilder}
   */
  setSamples(accessions) {
    this._samples = [...accessions];
    return this;
  }

  // ── Attributes ─────────────────────────────────────────────────────────────

  /**
   * Add an attribute filter.
   * @param {string} name - Field name, e.g. "genome_size".
   * @param {string|null} [operator] - Comparison operator: "eq","ne","lt","le","gt","ge","exists","missing".
   * @param {string|string[]|null} [value] - Scalar or array. Size suffixes "G"/"M"/"K" are accepted.
   * @param {string[]|null} [modifiers] - Summary modifiers e.g. ["min", "direct"].
   * @returns {QueryBuilder}
   */
  addAttribute(name, operator = null, value = null, modifiers = null) {
    const entry = { name };
    if (operator !== null) entry.operator = operator;
    if (value !== null) entry.value = value;
    if (modifiers !== null) entry.modifier = [...modifiers];
    this._attributes.push(entry);
    return this;
  }

  /**
   * Replace all attribute filters at once.
   * @param {Array<{name: string, operator?: string, value?: string|string[], modifier?: string[]}>} attributes
   * @returns {QueryBuilder}
   */
  setAttributes(attributes) {
    this._attributes = attributes.map((a) => ({ ...a }));
    return this;
  }

  /**
   * Request a field in the response.
   *
   * Accepts either the plain field name or the ``"field:modifier"`` shorthand.
   * For example, ``.addField("assembly_span:min")`` is equivalent to
   * ``.addField("assembly_span", ["min"])``.
   *
   * @param {string} name - Field name, e.g. "assembly_span", or shorthand
   *   "assembly_span:min".
   * @param {string[]|null} [modifiers] - Additional summary modifiers.
   * @returns {QueryBuilder}
   */
  addField(name, modifiers = null) {
    // Parse "field:modifier" shorthand.
    const colonIdx = name.indexOf(":");
    let bareName = name;
    let colonModifiers = [];
    if (colonIdx !== -1) {
      bareName = name.slice(0, colonIdx);
      colonModifiers = [name.slice(colonIdx + 1)];
    }
    const allModifiers = modifiers
      ? [...colonModifiers, ...modifiers]
      : colonModifiers;
    const entry = { name: bareName };
    if (allModifiers.length > 0) entry.modifier = allModifiers;
    this._fields.push(entry);
    return this;
  }

  /**
   * Return the ``__modifier`` column names implied by any field requests with modifiers.
   *
   * Summary modifiers (``min``, ``max``, …) and status modifiers (``direct``,
   * ``ancestral``, ``descendant``) all produce a ``{field}__modifier`` column in
   * the parsed output when the user explicitly requests them via ``:modifier``
   * syntax.  This is distinct from the automatically-added ``{field}__source``
   * metadata column which is never in this list.
   *
   * Pass the result to ``valuesOnly`` or ``annotatedValues`` as ``keepColumns``
   * so that these explicitly requested columns survive the ``__*`` stripping step.
   *
   * @example
   * const qb = new QueryBuilder("taxon")
   *   .addField("genome_size:direct")  // → genome_size__direct preserved
   *   .addField("assembly_span:min");  // → assembly_span__min preserved
   * const values = valuesOnly(records, qb.fieldModifiers());
   *
   * @returns {string[]}
   */
  fieldModifiers() {
    return this._fields.flatMap((f) =>
      (f.modifier || []).map((mod) => `${f.name}__${mod}`),
    );
  }

  /**
   * Replace the field selection at once.
   * @param {Array<string|{name: string, modifier?: string[]}>} fields
   * @returns {QueryBuilder}
   */
  setFields(fields) {
    this._fields = fields.map((f) =>
      typeof f === "string" ? { name: f } : { ...f },
    );
    return this;
  }

  /**
   * Set the name classes to include.
   * @param {string[]} nameClasses - e.g. ["scientific_name"].
   * @returns {QueryBuilder}
   */
  setNames(nameClasses) {
    this._names = [...nameClasses];
    return this;
  }

  /**
   * Set the lineage rank columns to include.
   * @param {string[]} ranks - e.g. ["genus", "family"].
   * @returns {QueryBuilder}
   */
  setRanks(ranks) {
    this._ranks = [...ranks];
    return this;
  }

  // ── Exclusion filters (field-level) ────────────────────────────────────────

  /**
   * Normalise a fields argument: shallow-copy an array, or return [] for null/undefined.
   * @param {string[]|null|undefined} fields
   * @returns {string[]}
   */
  _normaliseFields(fields) {
    return Array.isArray(fields) ? [...fields] : [];
  }

  /**
   * Exclude records with ancestrally derived estimated values for specified fields.
   * @param {string[]|null} fields - Array of field names, or null to clear.
   * @returns {QueryBuilder}
   */
  setExcludeAncestral(fields) {
    this._excludeAncestral = this._normaliseFields(fields);
    return this;
  }

  /**
   * Add a field to exclude ancestrally derived values for.
   * @param {string} field
   * @returns {QueryBuilder}
   */
  addExcludeAncestral(field) {
    if (!this._excludeAncestral.includes(field)) {
      this._excludeAncestral.push(field);
    }
    return this;
  }

  /**
   * Exclude records with descendant-derived estimated values for specified fields.
   * @param {string[]|null} fields - Array of field names, or null to clear.
   * @returns {QueryBuilder}
   */
  setExcludeDescendant(fields) {
    this._excludeDescendant = this._normaliseFields(fields);
    return this;
  }

  /**
   * Add a field to exclude descendant-derived values for.
   * @param {string} field
   * @returns {QueryBuilder}
   */
  addExcludeDescendant(field) {
    if (!this._excludeDescendant.includes(field)) {
      this._excludeDescendant.push(field);
    }
    return this;
  }

  /**
   * Exclude records with directly estimated values for specified fields.
   * @param {string[]|null} fields - Array of field names, or null to clear.
   * @returns {QueryBuilder}
   */
  setExcludeDirect(fields) {
    this._excludeDirect = this._normaliseFields(fields);
    return this;
  }

  /**
   * Add a field to exclude direct estimates for.
   * @param {string} field
   * @returns {QueryBuilder}
   */
  addExcludeDirect(field) {
    if (!this._excludeDirect.includes(field)) {
      this._excludeDirect.push(field);
    }
    return this;
  }

  /**
   * Exclude records with missing values for specified fields.
   * @param {string[]|null} fields - Array of field names, or null to clear.
   * @returns {QueryBuilder}
   */
  setExcludeMissing(fields) {
    this._excludeMissing = this._normaliseFields(fields);
    return this;
  }

  /**
   * Add a field to exclude records with missing values for.
   * @param {string} field
   * @returns {QueryBuilder}
   */
  addExcludeMissing(field) {
    if (!this._excludeMissing.includes(field)) {
      this._excludeMissing.push(field);
    }
    return this;
  }

  /**
   * Exclude all non-direct estimates (ancestral and descendant).
   *
   * Shorthand for: setExcludeAncestral() + setExcludeDescendant().
   *
   * @param {string[]|null} fields - Array of field names, or null to clear.
   * @returns {QueryBuilder}
   */
  setExcludeDerived(fields) {
    const normalised = this._normaliseFields(fields);
    this._excludeAncestral = normalised;
    this._excludeDescendant = [...normalised];
    return this;
  }

  /**
   * Exclude ancestral estimates and missing values.
   *
   * Shorthand for: setExcludeAncestral() + setExcludeMissing().
   *
   * @param {string[]|null} fields - Array of field names, or null to clear.
   * @returns {QueryBuilder}
   */
  setExcludeEstimated(fields) {
    const normalised = this._normaliseFields(fields);
    this._excludeAncestral = normalised;
    this._excludeMissing = [...normalised];
    return this;
  }

  // ── Query params ───────────────────────────────────────────────────────────

  /**
   * Set the page size.
   * @param {number} size
   * @returns {QueryBuilder}
   */
  setSize(size) {
    this._size = size;
    return this;
  }

  /**
   * Set the page number (1-based).
   * @param {number} page
   * @returns {QueryBuilder}
   */
  setPage(page) {
    this._page = page;
    return this;
  }

  /**
   * Sort results by a field.
   * @param {string} field
   * @param {string} [order="asc"] - "asc" or "desc".
   * @returns {QueryBuilder}
   */
  setSort(field, order = "asc") {
    this._sortBy = field;
    this._sortOrder = order;
    return this;
  }

  /**
   * Control whether estimated values are included.
   * @param {boolean} value
   * @returns {QueryBuilder}
   */
  setIncludeEstimates(value) {
    this._includeEstimates = value;
    return this;
  }

  /**
   * Set the taxonomy source, e.g. "ncbi" or "ott".
   * @param {string} taxonomy
   * @returns {QueryBuilder}
   */
  setTaxonomy(taxonomy) {
    this._taxonomy = taxonomy;
    return this;
  }

  // ── Serialization ──────────────────────────────────────────────────────────

  /**
   * Serialize the search query to YAML format for the WASM module.
   * Mirrors the Rust `SearchQuery` struct field names.
   * @returns {string}
   */
  toQueryYaml() {
    const lines = [];
    lines.push(`index: ${this._index}`);

    if (this._taxa.length > 0) {
      lines.push("taxa:");
      for (const t of this._taxa) lines.push(`  - "${t.replace(/"/g, '\\"')}"`);
    }
    if (this._assemblies.length > 0) {
      lines.push("assemblies:");
      for (const a of this._assemblies) lines.push(`  - "${a}"`);
    }
    if (this._samples.length > 0) {
      lines.push("samples:");
      for (const s of this._samples) lines.push(`  - "${s}"`);
    }
    if (this._rank) {
      lines.push(`rank: "${this._rank}"`);
    }
    if (this._taxonFilterType !== "name") {
      lines.push(`taxon_filter_type: ${this._taxonFilterType}`);
    }
    if (this._attributes.length > 0) {
      lines.push("attributes:");
      for (const attr of this._attributes) {
        lines.push(`  - name: "${attr.name}"`);
        if (attr.operator) lines.push(`    operator: ${attr.operator}`);
        if (attr.value !== null && attr.value !== undefined) {
          const val = Array.isArray(attr.value)
            ? attr.value.join(",")
            : attr.value;
          lines.push(`    value: "${val}"`);
        }
        if (attr.modifier && attr.modifier.length > 0) {
          lines.push("    modifier:");
          for (const m of attr.modifier) lines.push(`      - ${m}`);
        }
      }
    }
    if (this._fields.length > 0) {
      lines.push("fields:");
      for (const f of this._fields) {
        lines.push(`  - name: "${f.name}"`);
        if (f.modifier && f.modifier.length > 0) {
          lines.push("    modifier:");
          for (const m of f.modifier) lines.push(`      - ${m}`);
        }
      }
    }
    if (this._names.length > 0) {
      lines.push("names:");
      for (const n of this._names) lines.push(`  - ${n}`);
    }
    if (this._ranks.length > 0) {
      lines.push("ranks:");
      for (const r of this._ranks) lines.push(`  - ${r}`);
    }
    if (this._excludeAncestral.length > 0) {
      lines.push("excludeAncestral:");
      for (const f of this._excludeAncestral) lines.push(`  - ${f}`);
    }
    if (this._excludeDescendant.length > 0) {
      lines.push("excludeDescendant:");
      for (const f of this._excludeDescendant) lines.push(`  - ${f}`);
    }
    if (this._excludeDirect.length > 0) {
      lines.push("excludeDirect:");
      for (const f of this._excludeDirect) lines.push(`  - ${f}`);
    }
    if (this._excludeMissing.length > 0) {
      lines.push("excludeMissing:");
      for (const f of this._excludeMissing) lines.push(`  - ${f}`);
    }
    return lines.join("\n") + "\n";
  }

  /**
   * Serialize query parameters to YAML format for the WASM module.
   * Mirrors the Rust `QueryParams` struct field names.
   * @returns {string}
   */
  toParamsYaml() {
    const lines = [];
    lines.push(`size: ${this._size}`);
    lines.push(`page: ${this._page}`);
    lines.push(`include_estimates: ${this._includeEstimates}`);
    if (this._tidy) lines.push("tidy: true");
    lines.push(`taxonomy: ${this._taxonomy}`);
    if (this._sortBy) {
      lines.push(`sort_by: "${this._sortBy}"`);
      lines.push(`sort_order: ${this._sortOrder}`);
    }
    return lines.join("\n") + "\n";
  }

  // ── URL building ───────────────────────────────────────────────────────────

  /**
   * Build and return the full API URL without making a network request.
   * Delegates to the Rust WASM module for identical output to the Python SDK.
   *
   * @param {string} [apiBase] - Override the default API base URL.
   * @param {string} [apiVersion] - Override the default API version.
   * @returns {string}
   */
  toUrl(apiBase = API_BASE, apiVersion = API_VERSION, endpoint = "search") {
    const queryYaml = this.toQueryYaml();
    const paramsYaml = this.toParamsYaml();
    if (endpoint === "search") {
      return build_url(queryYaml, paramsYaml, apiBase, apiVersion);
    }
    return _buildUrlForEndpoint(
      queryYaml,
      paramsYaml,
      apiBase,
      apiVersion,
      endpoint,
    );
  }

  /**
   * Build and return the full UI URL without making a network request.
   * Targets the web interface rather than the REST API — no API version
   * component is inserted.
   *
   * @param {string} [uiBase] - Override the default UI base URL.
   * @param {string} [endpoint="search"] - UI route name.
   * @returns {string}
   */
  toUiUrl(uiBase = UI_BASE, endpoint = "search") {
    const queryYaml = this.toQueryYaml();
    const paramsYaml = this.toParamsYaml();
    return _buildUiUrl(queryYaml, paramsYaml, uiBase, endpoint);
  }

  // ── API calls ──────────────────────────────────────────────────────────────

  /**
   * Fetch the count of matching records.
   * @param {string} [apiBase]
   * @returns {Promise<number>}
   */
  async count(apiBase = API_BASE) {
    // Clone this builder with size=0 for counting
    const counter = new QueryBuilder(this._index);
    counter.merge(this);
    counter.setSize(0);
    const url = counter.toUrl(apiBase);
    const resp = await fetch(url, { headers: { Accept: "application/json" } });
    if (!resp.ok)
      throw new Error(`API request failed: ${resp.status} ${resp.statusText}`);
    const text = await resp.text();
    const statusJson = parse_response_status(text);
    return JSON.parse(statusJson).hits ?? 0;
  }

  /**
   * Fetch results as a parsed JSON object.
   * @param {string} [format="json"]
   * @param {string} [apiBase]
   * @returns {Promise<object>}
   */
  async search(format = "json", apiBase = API_BASE) {
    const url = this.toUrl(apiBase);
    const resp = await fetch(url, { headers: { Accept: "application/json" } });
    if (!resp.ok)
      throw new Error(`API request failed: ${resp.status} ${resp.statusText}`);
    if (format === "json") {
      return resp.json();
    }
    return resp.text();
  }

  /**
   * Fetch all matching records using cursor-based pagination.
   *
   * Uses the `/searchPaginated` endpoint in chunks of 1 000 records per page.
   * Pagination continues until all pages are retrieved or maxRecords is reached.
   *
   * @param {number} [maxRecords=Infinity]
   * @param {string} [apiBase]
   * @returns {Promise<object[]>}
   */
  async searchAll(maxRecords = Infinity, apiBase = API_BASE) {
    const CHUNK_SIZE = 1000;
    const allRecords = [];
    let searchAfter = null;

    while (true) {
      let url = this.toUrl(apiBase, API_VERSION, "searchPaginated");
      url += (url.includes("?") ? "&" : "?") + `size=${CHUNK_SIZE}`;
      if (searchAfter !== null) {
        url += `&searchAfter=${encodeURIComponent(JSON.stringify(searchAfter))}`;
      }

      const resp = await fetch(url, {
        headers: { Accept: "application/json" },
      });
      if (!resp.ok)
        throw new Error(
          `API request failed: ${resp.status} ${resp.statusText}`,
        );

      const page = JSON.parse(_parsePaginatedJson(await resp.text()));
      const records = page.records ?? [];
      const remaining = maxRecords - allRecords.length;
      allRecords.push(...records.slice(0, remaining));

      if (!page.hasMore || allRecords.length >= maxRecords) break;
      searchAfter = page.searchAfter;
    }

    return allRecords;
  }

  // ── Utilities ──────────────────────────────────────────────────────────────

  /**
   * Reset query state while preserving index and params.
   * @returns {QueryBuilder}
   */
  reset() {
    this._taxa = [];
    this._assemblies = [];
    this._samples = [];
    this._rank = null;
    this._taxonFilterType = "name";
    this._attributes = [];
    this._fields = [];
    this._names = [];
    this._ranks = [];
    return this;
  }

  /**
   * Merge non-default state from another builder into this one.
   * @param {QueryBuilder} other
   * @returns {QueryBuilder}
   */
  merge(other) {
    if (other._index !== this._index) {
      throw new Error(
        `Cannot merge builders with different indexes: '${this._index}' vs '${other._index}'`,
      );
    }
    this._taxa.push(...other._taxa);
    this._assemblies.push(...other._assemblies);
    this._samples.push(...other._samples);
    this._attributes.push(...other._attributes);
    this._fields.push(...other._fields);
    this._names.push(...other._names);
    this._ranks.push(...other._ranks);
    if (other._rank !== null) this._rank = other._rank;
    if (other._taxonFilterType !== "name")
      this._taxonFilterType = other._taxonFilterType;
    if (other._size !== 10) this._size = other._size;
    if (other._page !== 1) this._page = other._page;
    if (other._sortBy !== null) {
      this._sortBy = other._sortBy;
      this._sortOrder = other._sortOrder;
    }
    if (!other._includeEstimates)
      this._includeEstimates = other._includeEstimates;
    if (other._tidy) this._tidy = other._tidy;
    if (other._taxonomy !== "ncbi") this._taxonomy = other._taxonomy;
    return this;
  }

  /**
   * Create a new builder by merging all provided builders.
   * @param {...QueryBuilder} builders
   * @returns {QueryBuilder}
   */
  static combine(...builders) {
    if (builders.length === 0)
      throw new Error("combine() requires at least one builder");
    const result = new QueryBuilder(builders[0]._index);
    for (const b of builders) result.merge(b);
    return result;
  }

  /**
   * Get a human-readable description of this query.
   *
   * @param {object|null} [fieldMetadata] - Field metadata from the API (optional).
   *   If provided, field display names will be used in the description.
   * @param {string} [mode="concise"] - "concise" or "verbose"
   * @returns {string}
   */
  async describe(fieldMetadata = null, mode = "concise") {
    const fieldMetadataJson = fieldMetadata
      ? JSON.stringify(fieldMetadata)
      : "{}";
    const result = _describeQuery(
      this.toQueryYaml(),
      this.toParamsYaml(),
      fieldMetadataJson,
      mode,
    );
    // Parse JSON string response back to plain string (WASM returns JSON-encoded)
    try {
      return JSON.parse(result);
    } catch {
      return result; // Fall back to raw result if not JSON
    }
  }

  /**
   * Generate runnable code snippets in one or more languages.
   *
   * @param {string[]} [languages=["js"]]
   * @param {string} [siteName="{{ site_name }}"]
   * @param {string} [sdkName="{{ js_package_name }}"]
   * @param {string} [apiBase="{{ api_base_url }}"]
   * @returns {object}
   */
  async snippet(
    languages = ["js"],
    siteName = "{{ site_name }}",
    sdkName = "{{ js_package_name }}",
    apiBase = "{{ api_base_url }}",
  ) {
    const snapshot = {
      index: this._index,
      taxa: this._taxa,
      taxon_filter: this._taxonFilterType,
      rank: this._rank,
      filters: this._attributes.map((attr) => [
        attr.name,
        attr.operator || "",
        attr.value || "",
      ]),
      sorts: this._sortBy
        ? [[this._sortBy, this._sortOrder === "desc" ? "desc" : "asc"]]
        : [],
      flags: this._flags,
      selections: this._fields.map((field) => field.name),
    };
    const snapshotJson = JSON.stringify(snapshot);
    const languagesStr = languages.join(",");
    const result = _renderSnippet(
      snapshotJson,
      siteName,
      apiBase,
      sdkName,
      languagesStr,
    );
    try {
      return JSON.parse(result);
    } catch {
      // If WASM returns an error, parse it as JSON and throw
      try {
        const error = JSON.parse(result);
        if (error.error) {
          throw new Error(error.error);
        }
      } catch {
        // Re-throw original error if JSON parsing failed
      }
      throw new Error(`snippet() failed: ${result}`);
    }
  }

  /**
   * Validate this query against field metadata and configuration.
   *
   * Validate the current query state.
   *
   * Returns an array of error strings. Empty array means the query is valid.
   *
   * @returns {Promise<string[]>} Promise that resolves to array of validation error messages (empty if valid)
   */
  /**
   * Validate the current query against metadata and field constraints.
   *
   * @param {string} [validationLevel] - Override instance validationLevel: "full" or "partial".
   * @returns {Promise<string[]>} Array of validation error messages (empty if valid).
   *
   * **Validation Modes:**
   * - "full": Attempts to fetch metadata from API (v3+) via /api/v3/metadata/fields and
   *   /api/v3/metadata/validation-config. Falls back to local files (Node.js) or empty
   *   metadata (browser) if API is unavailable. Gracefully handles 404s without logging.
   * - "partial": Skips API fetch entirely. Uses local files (Node.js) or empty metadata
   *   (browser) for validation. Best until v3 API endpoints are available.
   *
   * **Integration with api-refactoring-plan.md (v3):**
   * Currently defaults to "full" for forward compatibility. When v3 API is deployed with
   * metadata endpoints available, validation will automatically use them. Until then,
   * "" mode parameter or set `options.validationLevel = "partial"` in constructor to
   * avoid 404 attempts and logs.
   */
  async validate(validationLevel) {
    let fieldMetadata = {};
    let validationConfig = {};
    let synonyms = {};

    // Use override or instance setting
    const level = validationLevel || this._validationLevel;

    // If full mode, attempt API fetch first (graceful fallback if unavailable)
    if (level === "full") {
      try {
        const apiFieldsUrl = `${this._apiBase}/api/v3/metadata/fields`;
        const apiConfigUrl = `${this._apiBase}/api/v3/metadata/validation-config`;

        try {
          const fieldsResponse = await fetch(apiFieldsUrl);
          if (fieldsResponse.ok) {
            fieldMetadata = await fieldsResponse.json();
          }
          // Silently skip on 404 or other non-ok status (API not ready yet)
        } catch {
          // Network error or fetch not available; continue with fallback
        }

        try {
          const configResponse = await fetch(apiConfigUrl);
          if (configResponse.ok) {
            validationConfig = await configResponse.json();
          }
          // Silently skip on 404 or other non-ok status (API not ready yet)
        } catch {
          // Network error or fetch not available; continue with fallback
        }
      } catch {
        // Fallback to local/empty metadata below
      }
    }

    // Try to load metadata files from disk (Node.js only; browsers will skip gracefully)
    // This is the fallback for full mode and the only source for partial mode
    try {
      const { default: fs } = await import("fs");
      const { dirname } = await import("path");
      const { fileURLToPath } = await import("url");

      const __dirname = dirname(fileURLToPath(import.meta.url));

      try {
        const fieldMetaPath = `${__dirname}/generated/field_meta.json`;
        if (fs.existsSync(fieldMetaPath)) {
          const content = fs.readFileSync(fieldMetaPath, "utf8");
          fieldMetadata = JSON.parse(content);
        }
      } catch {
        // Silently continue if not available
      }

      try {
        const configPath = `${__dirname}/generated/validation_config.json`;
        if (fs.existsSync(configPath)) {
          const content = fs.readFileSync(configPath, "utf8");
          validationConfig = JSON.parse(content);
        }
      } catch {
        // Silently continue if not available
      }
    } catch {
      // If fs is not available (browser), continue with empty metadata
      // This is expected in browser environments
    }

    const fieldMetadataJson = JSON.stringify(fieldMetadata || {});
    const validationConfigJson = JSON.stringify(validationConfig || {});
    const synonymsJson = JSON.stringify(synonyms);

    const result = _validateQueryJson(
      this.toQueryYaml(),
      fieldMetadataJson,
      validationConfigJson,
      synonymsJson,
    );

    try {
      return JSON.parse(result);
    } catch {
      // If parsing fails, return the result as a single error
      return [result];
    }
  }
}

/**
 * Reshape flat records from parseSearchJson into long/tidy format.
 *
 * Each flat record is exploded so that every bare field becomes its own row
 * with columns: identity columns (taxon_id, scientific_name, …), `field`,
 * `value`, and `source`.  Explicitly-requested modifier columns are emitted
 * as separate rows with `field` set to `"{bare}:{modifier}"` and `source`
 * as `null`.
 *
 * Suitable as input for d3 or Vega-Lite pivot charts.
 *
 * @param {string|object[]} records - Flat records from parseSearchJson.
 * @returns {object[]}
 */
function toTidyRecords(records) {
  const str = typeof records === "string" ? records : JSON.stringify(records);
  return JSON.parse(_toTidyRecords(str));
}

export {
  QueryBuilder,
  parseSearchJson,
  parseResponseStatus,
  annotateSourceLabels,
  splitSourceColumns,
  valuesOnly,
  annotatedValues,
  toTidyRecords,
};
