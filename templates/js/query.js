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

// Load the pre-compiled WASM module
import {
  annotate_source_labels as _annotateSourceLabels,
  annotated_values as _annotatedValues,
  parse_search_json as _parseSearchJson,
  split_source_columns as _splitSourceColumns,
  values_only as _valuesOnly,
  build_url,
  parse_response_status,
} from "./pkg/genomehubs_query.js";

/** Flatten a raw API response string or object into an array of flat record objects. */
function parseSearchJson(raw) {
  const str = typeof raw === "string" ? raw : JSON.stringify(raw);
  return JSON.parse(_parseSearchJson(str));
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
   */
  constructor(index) {
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
    // QueryParams
    this._size = 10;
    this._page = 1;
    this._sortBy = null;
    this._sortOrder = "asc";
    this._includeEstimates = true;
    this._tidy = false;
    this._taxonomy = "ncbi";
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
  toUrl(apiBase = API_BASE, apiVersion = API_VERSION) {
    const queryYaml = this.toQueryYaml();
    const paramsYaml = this.toParamsYaml();
    return build_url(queryYaml, paramsYaml, apiBase, apiVersion);
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
   * Note: Not yet available in the JavaScript SDK (requires Phase 3 WASM bindings).
   * @param {object|null} [fieldMetadata=null]
   * @param {"concise"|"verbose"} [mode="concise"]
   * @returns {string}
   */
  describe(fieldMetadata = null, mode = "concise") {
    throw new Error(
      "describe() requires WASM bindings not yet available in the JavaScript SDK.",
    );
  }

  /**
   * Generate runnable code snippets in one or more languages.
   *
   * Note: Not yet available in the JavaScript SDK (requires Phase 3 WASM bindings).
   * @param {string[]} [languages=["js"]]
   * @param {string} [siteName="{{ site_name }}"]
   * @param {string} [sdkName="{{ js_package_name }}"]
   * @param {string} [apiBase="{{ api_base_url }}"]
   * @returns {object}
   */
  snippet(
    languages = ["js"],
    siteName = "{{ site_name }}",
    sdkName = "{{ js_package_name }}",
    apiBase = "{{ api_base_url }}",
  ) {
    throw new Error(
      "snippet() requires WASM bindings not yet available in the JavaScript SDK.",
    );
  }
}

export {
  QueryBuilder,
  parseSearchJson,
  annotateSourceLabels,
  splitSourceColumns,
  valuesOnly,
  annotatedValues,
};
