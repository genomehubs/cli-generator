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
const API_VERSION = "{{ api_version }}";
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
  parse_batch_json: _parseBatchJson,
  parse_histogram_json: _parseHistogramJson,
  parse_lookup_json: _parseLookupJson,
  parse_paginated_json: _parsePaginatedJson,
  parse_phylopic_json: _parsePhylopicJson,
  parse_phylopic_batch_json: _parsePhylopicBatchJson,
  parse_record_json: _parseRecordJson,
  parse_search_json: _parseSearchJson,
  parse_tree_json: _parseTreeJson,
  render_snippet: _renderSnippet,
  split_source_columns: _splitSourceColumns,
  to_tidy_records: _toTidyRecords,
  parse_search_with_lineage_summary: _parseSearchWithLineageSummary,
  validate_query_json: _validateQueryJson,
  validate_report_yaml: _validateReportYaml,
  values_only: _valuesOnly,
  query_yaml_from_url_params: _queryYamlFromUrlParams,
  report_yaml_from_url_params: _reportYamlFromUrlParams,
  local_plot_spec_json: _localPlotSpecJson,
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

/** Extract histogram buckets from a raw `/report` JSON response. Returns an array of bucket objects. */
function parseHistogramJson(raw) {
  const str = typeof raw === "string" ? raw : JSON.stringify(raw);
  const result = _parseHistogramJson(str);
  return JSON.parse(result);
}

/** Flatten a tree report's `treeNodes` map into an array of node objects. */
function parseTreeJson(raw) {
  const str = typeof raw === "string" ? raw : JSON.stringify(raw);
  const result = _parseTreeJson(str);
  return JSON.parse(result);
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
    // YAML overrides set by fromV2Url(); take priority in toQueryYaml/toParamsYaml
    this._queryYamlOverride = null;
    this._paramsYamlOverride = null;
    this._lineageRankSummary = [];
    this._namedQueries = {};
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

  /**
   * Set lineage rank summary aggregation specs.
   * @param {object[]} specs - Array of spec objects with `rank` and `fields` keys.
   * @returns {QueryBuilder}
   */
  setLineageRankSummary(specs) {
    this._lineageRankSummary = specs.map((s) => ({ ...s }));
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

  /**
   * Restrict results to exactly the supplied taxon IDs.
   *
   * Injected as an ES `terms` filter ANDed with the main query.
   * Maximum 65,536 IDs (ES hard limit).
   *
   * @param {number[]} taxonIds - List of integer taxon IDs to include.
   * @returns {QueryBuilder}
   */
  setIdSet(taxonIds) {
    this._idSet = [...taxonIds];
    return this;
  }

  /**
   * Specify which ID field to filter on when using ``setIdSet``.
   *
   * Determines the ES field to filter on:
   * - "taxon" → taxon_id
   * - "assembly" → assembly_id
   * - "sample" → sample_id
   * - "feature" → feature_id
   *
   * If not specified, defaults to the current index type.
   *
   * @param {string} idType - One of "taxon", "assembly", "sample", "feature".
   * @returns {QueryBuilder}
   */
  setIdType(idType) {
    this._idType = idType;
    return this;
  }

  /**
   * Add a named sub-query whose results can be referenced via ``queryKey.*``
   * in attribute filters of the parent query.
   * @param {string} queryKey - Identifier for this sub-query (e.g. "queryA").
   * @param {string} queryString - Filter expression for the sub-query (e.g. "assembly_span>1e9").
   * @param {Object} [opts]
   * @param {string} [opts.index] - Target index (defaults to parent index).
   * @param {number} [opts.limit] - Maximum hits to resolve from sub-query.
   * @param {boolean} [opts.inheritScope] - Whether to inherit taxa/rank from parent.
   * @returns {QueryBuilder}
   */
  chainQuery(queryKey, queryString, opts = {}) {
    const spec = { query: queryString };
    if (opts.index != null) spec.index = opts.index;
    if (opts.limit != null) spec.limit = opts.limit;
    if (opts.inheritScope != null) spec.inherit_scope = opts.inheritScope;
    this._namedQueries[queryKey] = spec;
    return this;
  }

  // ── Serialization ──────────────────────────────────────────────────────────

  /**
   * Serialize the search query to YAML format for the WASM module.
   * Mirrors the Rust `SearchQuery` struct field names.
   * @returns {string}
   */
  toQueryYaml() {
    if (this._queryYamlOverride !== null) return this._queryYamlOverride;
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
      lines.push("exclude_ancestral:");
      for (const f of this._excludeAncestral) lines.push(`  - ${f}`);
    }
    if (this._excludeDescendant.length > 0) {
      lines.push("exclude_descendant:");
      for (const f of this._excludeDescendant) lines.push(`  - ${f}`);
    }
    if (this._excludeDirect.length > 0) {
      lines.push("exclude_direct:");
      for (const f of this._excludeDirect) lines.push(`  - ${f}`);
    }
    if (this._excludeMissing.length > 0) {
      lines.push("exclude_missing:");
      for (const f of this._excludeMissing) lines.push(`  - ${f}`);
    }
    if (this._lineageRankSummary.length > 0) {
      lines.push("lineage_rank_summary:");
      for (const spec of this._lineageRankSummary) {
        lines.push(`  - rank: ${spec.rank}`);
        if (spec.fields) {
          lines.push("    fields:");
          // Handle both array (field names) and object (field->mode mappings)
          if (Array.isArray(spec.fields)) {
            for (const field of spec.fields) {
              lines.push(`      - ${field}`);
            }
          } else {
            // Object with field -> mode mappings
            for (const [field, mode] of Object.entries(spec.fields)) {
              if (Array.isArray(mode)) {
                lines.push(`      ${field}:`);
                for (const m of mode) lines.push(`        - ${m}`);
              } else {
                lines.push(`      ${field}: ${mode}`);
              }
            }
          }
        }
      }
    }
    if (Object.keys(this._namedQueries).length > 0) {
      lines.push("named_queries:");
      for (const [key, spec] of Object.entries(this._namedQueries)) {
        lines.push(`  ${key}:`);
        lines.push(`    filter_expr: "${spec.filter_expr}"`);
        if (spec.index != null) lines.push(`    index: ${spec.index}`);
        if (spec.limit != null) lines.push(`    limit: ${spec.limit}`);
        if (spec.inherit_scope != null)
          lines.push(`    inherit_scope: ${spec.inherit_scope}`);
      }
    }
    return lines.join("\n") + "\n";
  }

  /**
   * Serialize query parameters to YAML format for the WASM module.
   * Mirrors the Rust `QueryParams` struct field names.
   * @returns {string}
   */
  toParamsYaml() {
    if (this._paramsYamlOverride !== null) return this._paramsYamlOverride;
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
    if (this._idSet && this._idSet.length > 0) {
      lines.push("id_set:");
      for (const id of this._idSet) {
        lines.push(`  - ${id}`);
      }
    }
    if (this._idType) {
      lines.push(`id_type: ${this._idType}`);
    }
    return lines.join("\n") + "\n";
  }

  // ── URL building ───────────────────────────────────────────────────────────

  /**
   * Reconstruct a builder from a v2 API or UI URL.
   *
   * Detects whether the URL is a search or report URL and returns the
   * appropriate builder type.  Report URLs (path ends in `/report` or the
   * query string contains `report=`) return a {@link ReportBuilder} with an
   * embedded query so that {@link ReportBuilder#run} can be called without
   * supplying a separate {@link QueryBuilder}.
   *
   * @param {string} url - A full v2 API or UI URL, e.g.
   *   `"https://goat.genomehubs.org/api/v2/search?tax_name=Primates&fields=genome_size"`
   *   or
   *   `"https://goat.genomehubs.org/report?report=histogram&x=genome_size&result=taxon"`.
   * @returns {QueryBuilder|ReportBuilder}
   * @throws {Error} If URL parsing fails.
   */
  static fromV2Url(url) {
    const parsed = new URL(url);
    const isReport =
      parsed.searchParams.has("report") ||
      parsed.pathname.replace(/\/$/, "").endsWith("/report");

    if (isReport) {
      const raw = _reportYamlFromUrlParams(url);
      let triple;
      try {
        triple = JSON.parse(raw);
      } catch {
        throw new Error(`fromV2Url: failed to parse report URL: ${raw}`);
      }
      if (!Array.isArray(triple))
        throw new Error(`fromV2Url: ${triple.error ?? raw}`);
      const [queryYaml, paramsYaml, reportYaml] = triple;
      const qb = new QueryBuilder("taxon");
      qb._queryYamlOverride = queryYaml;
      qb._paramsYamlOverride = paramsYaml;
      const rb = new ReportBuilder("_placeholder");
      rb._reportYamlOverride = reportYaml;
      rb._embeddedQueryBuilder = qb;
      return rb;
    }

    const raw = _queryYamlFromUrlParams(url);
    let pair;
    try {
      pair = JSON.parse(raw);
    } catch {
      throw new Error(`fromV2Url: failed to parse URL: ${raw}`);
    }
    if (!Array.isArray(pair))
      throw new Error(`fromV2Url: ${pair.error ?? raw}`);
    const [queryYaml, paramsYaml] = pair;
    const qb = new QueryBuilder("taxon");
    qb._queryYamlOverride = queryYaml;
    qb._paramsYamlOverride = paramsYaml;
    return qb;
  }

  /**
   * Build and return the full v2 API URL without making a network request.
   * Delegates to the Rust WASM module for identical output to the Python SDK.
   *
   * @param {string} [apiBase] - Override the default API base URL.
   * @param {string} [apiVersion] - Override the default API version.
   * @param {string} [endpoint="search"] - API endpoint name.
   * @returns {string}
   */
  toV2Url(apiBase = API_BASE, apiVersion = API_VERSION, endpoint = "search") {
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
   * @deprecated Use {@link toV2Url} instead.
   * @param {string} [apiBase]
   * @param {string} [apiVersion]
   * @param {string} [endpoint="search"]
   * @returns {string}
   */
  toUrl(apiBase = API_BASE, apiVersion = API_VERSION, endpoint = "search") {
    console.warn("toUrl() is deprecated; use toV2Url() instead.");
    return this.toV2Url(apiBase, apiVersion, endpoint);
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
   * POST JSON to a URL and return parsed JSON response.
   * @param {string} url
   * @param {object} payload
   * @returns {Promise<object>}
   */
  async _postJson(url, payload) {
    const resp = await fetch(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });
    if (!resp.ok)
      throw new Error(`POST ${url} failed: ${resp.status} ${resp.statusText}`);
    return resp.json();
  }

  /**
   * Fetch the count of matching records.
   * @param {string} [apiBase]
   * @returns {Promise<number>}
   */
  async count(apiBase = API_BASE) {
    const data = await this._postJson(`${apiBase}/v3/count`, {
      query_yaml: this.toQueryYaml(),
      params_yaml: this.toParamsYaml(),
    });
    const statusJson = parse_response_status(JSON.stringify(data));
    return JSON.parse(statusJson).hits ?? 0;
  }

  /**
   * Fetch results as a parsed JSON object (v3 POST) or raw text (v2 GET for non-JSON).
   * @param {string} [format="json"]
   * @param {string} [apiBase]
   * @returns {Promise<object|string>}
   */
  async search(format = "json", apiBase = API_BASE) {
    if (format !== "json") {
      const url = this.toV2Url(apiBase);
      const mimeType =
        format === "tsv" ? "text/tab-separated-values" : "text/csv";
      const resp = await fetch(url, { headers: { Accept: mimeType } });
      if (!resp.ok)
        throw new Error(
          `API request failed: ${resp.status} ${resp.statusText}`,
        );
      return resp.text();
    }
    return this._postJson(`${apiBase}/v3/search`, {
      query_yaml: this.toQueryYaml(),
      params_yaml: this.toParamsYaml(),
    });
  }

  /**
   * Fetch all matching records using v3 cursor-based pagination.
   *
   * Sends repeated POST requests to ``/v3/search`` with ``search_after`` cursors
   * until all records are retrieved or maxRecords is reached.
   *
   * @param {number} [maxRecords=Infinity]
   * @param {string} [apiBase]
   * @returns {Promise<object[]>}
   */
  async searchAll(maxRecords = Infinity, apiBase = API_BASE) {
    const CHUNK_SIZE = 1000;
    const allRecords = [];
    let searchAfter = null;
    const origSize = this._size;
    this.setSize(CHUNK_SIZE);
    try {
      while (true) {
        const payload = {
          query_yaml: this.toQueryYaml(),
          params_yaml: this.toParamsYaml(),
        };
        if (searchAfter !== null) payload.search_after = searchAfter;
        const data = await this._postJson(`${apiBase}/v3/search`, payload);
        const records = JSON.parse(_parseSearchJson(JSON.stringify(data)));
        const remaining = maxRecords - allRecords.length;
        allRecords.push(...records.slice(0, remaining));
        searchAfter = data.search_after ?? null;
        const total = data.status?.hits ?? 0;
        if (!searchAfter || allRecords.length >= Math.min(maxRecords, total))
          break;
      }
    } finally {
      this.setSize(origSize);
    }
    return allRecords.slice(0, maxRecords);
  }

  /**
   * Fetch and flatten search results, optionally joining lineage summary columns.
   *
   * Delegates to the Rust WASM function `_parseSearchWithLineageSummary` for all
   * lineage column reduction logic, keeping JS free of duplicated logic.
   *
   * @param {object|null} [lineageSummary=null] - Optional explicit config `{rank: {field: "mode"}}`.
   *        If null and lineage_rank_summary specs are set, defaults to "stats" mode for all fields.
   * @param {string} [apiBase=API_BASE] - Base URL of the API
   * @returns {Promise<object[]>} - Array of flat record objects
   */
  async toFlatRecords(lineageSummary = null, apiBase = API_BASE) {
    const response = await this.search("json", apiBase);
    const responseJson = JSON.stringify(response);

    // Only join lineage columns when the caller provides an explicit config with field modes.
    // Without a config the field types (numeric vs categorical) are unknown, so fall back
    // to basic flattening — matching Python's behavior.
    if (lineageSummary === null) {
      return JSON.parse(_parseSearchJson(responseJson));
    }

    return JSON.parse(
      _parseSearchWithLineageSummary(
        responseJson,
        JSON.stringify(lineageSummary),
      ),
    );
  }

  /**
   * Run a report query against the v3 /report endpoint.
   * @param {ReportBuilder} report - A ReportBuilder instance
   * @param {string} [apiBase=API_BASE] - Base URL of the API
   * @returns {Promise<object>} - Raw report object from the response
   */
  async report(report, apiBase = API_BASE) {
    const body = {
      query_yaml: this.toQueryYaml(),
      params_yaml: this.toParamsYaml(),
      report_yaml: report.toReportYaml(),
    };
    if (report._display !== null) body.display = report._display;
    if (report._includePlotSpec) body.include_plot_spec = true;
    const data = await this._postJson(`${apiBase}/v3/report`, body);
    if (data.plot_spec) return data;
    return data.report ?? data;
  }

  /**
   * Execute multiple searches in a single batch request.
   * @param {QueryBuilder[]} queries - Array of QueryBuilder objects
   * @param {string} [apiBase=API_BASE] - Base URL of the API
   * @returns {Promise<object[]>} - Array of batch search results
   */
  async searchBatch(queries, apiBase = API_BASE) {
    if (queries.length > 100)
      throw new Error("maximum 100 searches per batch request");

    const data = JSON.parse(
      _parseBatchJson(
        JSON.stringify(
          await this._postJson(`${apiBase}/v3/search/batch`, {
            searches: queries.map((q) => ({
              query_yaml: q.toQueryYaml(),
              params_yaml: q.toParamsYaml(),
            })),
          }),
        ),
      ),
    );
    return data.results ?? [];
  }

  /**
   * Get hit counts for multiple queries in a single batch request.
   * @param {QueryBuilder[]} queries - Array of QueryBuilder objects
   * @param {string} [apiBase=API_BASE] - Base URL of the API
   * @returns {Promise<number[]>} - Array of hit counts
   */
  async countBatch(queries, apiBase = API_BASE) {
    if (queries.length > 100)
      throw new Error("maximum 100 searches per batch request");

    const data = JSON.parse(
      _parseBatchJson(
        JSON.stringify(
          await this._postJson(`${apiBase}/v3/count/batch`, {
            searches: queries.map((q) => ({
              query_yaml: q.toQueryYaml(),
              params_yaml: q.toParamsYaml(),
            })),
          }),
        ),
      ),
    );
    const counts = [];
    for (const result of data.results ?? []) {
      counts.push(result.status?.hits ?? 0);
    }
    return counts;
  }

  /**
   * Fetch a single record by ID or identifier.
   * @param {string} recordId - Record ID to fetch
   * @param {string} [result] - Result type (taxon|assembly|sample), default from index
   * @returns {Promise<object>} - Parsed record object
   */
  async record(recordId, result = null) {
    if (!recordId) throw new Error("record() requires a recordId parameter");

    const resultType = result || this._index || "taxon";
    const params = new URLSearchParams({
      recordId,
      result: resultType,
    });
    const url = `${API_BASE}/v3/record?${params.toString()}`;

    const resp = await fetch(url, { method: "GET" });

    if (!resp.ok)
      throw new Error(`API request failed: ${resp.status} ${resp.statusText}`);

    return JSON.parse(await resp.text());
  }

  /**
   * Fetch up to 1,000 records by ID in a single request.
   * @param {string[]} recordIds - Array of record IDs (max 1,000)
   * @param {string} [result] - Result type (taxon|assembly|sample), default from index
   * @returns {Promise<object>} - Parsed batch record response
   */
  async recordBatch(recordIds, result = null) {
    if (!recordIds || recordIds.length === 0)
      throw new Error("recordBatch() requires a non-empty recordIds array");

    const resultType = result || this._index || "taxon";
    const url = `${API_BASE}/v3/record/batch`;

    const resp = await fetch(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ record_ids: recordIds, result: resultType }),
    });

    if (!resp.ok)
      throw new Error(`API request failed: ${resp.status} ${resp.statusText}`);

    return JSON.parse(await resp.text());
  }

  /**
   * Run a positional report (oxford / ribbon / painting / circos) via POST /positional.
   * @param {string} report - Sub-type: "oxford", "ribbon", "painting", or "circos"
   * @param {string} groupBy - Attribute key for shared marker (e.g. "busco_gene")
   * @param {string[]} assemblies - Assembly IDs to compare
   * @param {object} [opts] - Optional parameters
   * @param {string} [opts.featureType] - primary_type filter
   * @param {number|null} [opts.windowSize] - Regional binning in bp
   * @param {boolean} [opts.reorient=true] - Auto-orient comparison sequences
   * @param {number} [opts.maxFeatures=10000] - Hard cap on features fetched
   * @param {string|null} [opts.cat] - Category field for colour
   * @param {string|null} [opts.catOpts] - Category axis options
   * @param {Array|null} [opts.filter] - Attribute filter list
   * @param {object|null} [opts.regions] - Region config (cat, bounds, min_features, max_expansion)
   * @param {number|null} [opts.maxConnectionsPerGroup] - M:N connection cap
   * @returns {Promise<object>} - Raw report dict from the response
   */
  async positional(report, groupBy, assemblies, opts = {}) {
    const {
      featureType,
      windowSize,
      reorient = true,
      maxFeatures = 10000,
      cat,
      catOpts,
      filter,
      regions,
      maxConnectionsPerGroup,
    } = opts;
    const positionalDoc = {
      report,
      group_by: groupBy,
      assemblies: [...assemblies],
    };
    if (featureType != null) positionalDoc.feature_type = featureType;
    if (windowSize != null) positionalDoc.window_size = windowSize;
    if (!reorient) positionalDoc.reorient = false;
    if (maxFeatures !== 10000) positionalDoc.max_features = maxFeatures;
    if (cat != null) positionalDoc.cat = cat;
    if (catOpts != null) positionalDoc.cat_opts = catOpts;
    if (filter != null && filter.length > 0) positionalDoc.filter = filter;
    if (regions != null) positionalDoc.regions = regions;
    if (maxConnectionsPerGroup != null)
      positionalDoc.max_connections_per_group = maxConnectionsPerGroup;

    const positionalYaml = Object.entries(positionalDoc)
      .map(([k, v]) => {
        if (Array.isArray(v))
          return `${k}:\n${v.map((x) => `  - ${x}`).join("\n")}`;
        return `${k}: ${v}`;
      })
      .join("\n");

    const url = `${API_BASE}/v3/positional`;
    const resp = await fetch(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        query_yaml: this.toQueryYaml(),
        positional_yaml: positionalYaml,
      }),
    });
    if (!resp.ok)
      throw new Error(
        `positional API request failed: ${resp.status} ${resp.statusText}`,
      );
    const data = await resp.json();
    return data.report ?? data;
  }

  /** Oxford dot-plot (exactly 2 assemblies). Wrapper around positional(). */
  async oxford(groupBy, assemblies, opts = {}) {
    return this.positional("oxford", groupBy, assemblies, opts);
  }

  /** Ribbon/synteny (N ≥ 2 assemblies). Wrapper around positional(). */
  async ribbon(groupBy, assemblies, opts = {}) {
    return this.positional("ribbon", groupBy, assemblies, opts);
  }

  /** Chromosome painting (1 assembly). Wrapper around positional().
   * @param {string} groupBy - Attribute key for shared marker
   * @param {string} assembly - Single assembly ID
   * @param {object} [opts] - Optional parameters (same as positional)
   */
  async painting(groupBy, assembly, opts = {}) {
    return this.positional("painting", groupBy, [assembly], opts);
  }

  /**
   * Run a hybrid positional report combining remote and local assembly data.
   *
   * Parses local BUSCO / feature files and optionally fetches remote features
   * via POST /api/v3/positional.  When remoteAssemblies is empty, the plot is
   * computed entirely from local strings (no API call, WASM-friendly).
   *
   * Each entry in localFiles is an object with:
   *   - busco        {string}  Full text of a BUSCO full_table.tsv (required)
   *   - assemblyId   {string}  Label for the assembly (required)
   *   - fai          {string}  Full text of a .fai index (optional)
   *   - lengths      {string}  Full text of a two-column lengths TSV (optional)
   *
   * @param {string}   report                    - "oxford", "ribbon", or "painting"
   * @param {string}   groupBy                   - Shared marker identifier
   * @param {Array}    localFiles                - Array of local assembly objects
   * @param {object}   [opts]
   * @param {Array}    [opts.remoteAssemblies]   - Optional API assembly IDs (reference)
   * @param {boolean}  [opts.reorient=true]      - Auto-orient comparison sequences
   * @param {string}   [opts.cat]                - Category field for colour coding
   * @param {number}   [opts.windowSize]         - Bin size in bp (0 for none)
   * @param {number}   [opts.maxConnectionsPerGroup=0] - M:N cap (0 → default 25)
   * @returns {Promise<object>} - Report object in the same format as positional()
   */
  async hybridPositional(report, groupBy, localFiles, opts = {}) {
    const {
      remoteAssemblies = [],
      reorient = true,
      cat = "",
      windowSize = 0,
      maxConnectionsPerGroup = 0,
    } = opts;

    // Parse each local entry into a LocalFeatureSet object
    const localSets = localFiles.map((entry) => {
      const { assemblyId, busco, fai, lengths } = entry;
      const raw = JSON.parse(wasmModule.parse_busco_tsv(assemblyId, busco));
      if (raw.error)
        throw new Error(
          `parse_busco_tsv failed for '${assemblyId}': ${raw.error}`,
        );

      if (fai) {
        const lengthsMap = JSON.parse(wasmModule.parse_fai(fai));
        if (lengthsMap.error)
          throw new Error(
            `parse_fai failed for '${assemblyId}': ${lengthsMap.error}`,
          );
        raw.sequence_lengths = lengthsMap;
        raw.lengths_derived = false;
      } else if (lengths) {
        const lengthsMap = JSON.parse(wasmModule.parse_lengths_tsv(lengths));
        if (lengthsMap.error)
          throw new Error(
            `parse_lengths_tsv failed for '${assemblyId}': ${lengthsMap.error}`,
          );
        raw.sequence_lengths = lengthsMap;
        raw.lengths_derived = false;
      }
      return raw;
    });

    // All-local mode
    if (!remoteAssemblies || remoteAssemblies.length === 0) {
      const resultJson = wasmModule.positional_from_features(
        JSON.stringify(localSets),
        report,
        reorient,
        cat,
        windowSize,
        maxConnectionsPerGroup,
        "",
      );
      const result = JSON.parse(resultJson);
      if (result.error)
        throw new Error(`positional_from_features failed: ${result.error}`);
      return result;
    }

    // Hybrid mode: fetch remote reference via API
    const positionalDoc = {
      report,
      group_by: groupBy,
      assemblies: remoteAssemblies,
    };
    if (cat) positionalDoc.cat = cat;
    if (windowSize) positionalDoc.window_size = windowSize;
    if (!reorient) positionalDoc.reorient = false;
    if (maxConnectionsPerGroup)
      positionalDoc.max_connections_per_group = maxConnectionsPerGroup;

    const remoteData = await this._postJson(`${API_BASE}/v3/positional`, {
      query_yaml: this.toQueryYaml(),
      positional_yaml: Object.entries(positionalDoc)
        .map(([k, v]) => `${k}: ${JSON.stringify(v)}\n`)
        .join(""),
    });
    const remoteReport = remoteData.report || remoteData;

    const resultJson = wasmModule.hybrid_positional(
      JSON.stringify(remoteReport),
      JSON.stringify(localSets),
      reorient,
      maxConnectionsPerGroup,
    );
    const result = JSON.parse(resultJson);
    if (result.error)
      throw new Error(`hybrid_positional failed: ${result.error}`);
    return result;
  }

  /**
   * Lookup records by alternative identifiers (autocomplete/search-as-you-type).
   * @param {string} searchTerm - Search term for lookup
   * @param {string} [result] - Result type (taxon|assembly|sample), default from index
   * @param {number} [size=10] - Number of results to return
   * @returns {Promise<object>} - Parsed lookup result
   */
  async lookup(searchTerm, result = null, size = 10) {
    if (!searchTerm)
      throw new Error("lookup() requires a searchTerm parameter");

    const resultType = result || this._index || "taxon";
    const params = new URLSearchParams({
      searchTerm,
      result: resultType,
      size: size.toString(),
    });
    const url = `${API_BASE}/v3/lookup?${params.toString()}`;

    const resp = await fetch(url, { method: "GET" });

    if (!resp.ok)
      throw new Error(`API request failed: ${resp.status} ${resp.statusText}`);

    return JSON.parse(await resp.text());
  }

  /**
   * Resolve multiple search terms to record IDs in a single request.
   * @param {Array<string|object>} lookups - Items as strings or {search_term, result?, size?}
   * @param {string} [result] - Default result type for items that omit it
   * @param {number} [size=10] - Default page size for items that omit it
   * @returns {Promise<object>} - Batch lookup response with results array in input order
   */
  async lookupBatch(lookups, result = null, size = 10) {
    if (!lookups || lookups.length === 0)
      throw new Error("lookupBatch() requires a non-empty lookups array");

    const defaultResult = result || this._index || "taxon";

    const normalise = (item) => {
      if (typeof item === "string")
        return { search_term: item, result: defaultResult, size };
      return {
        search_term: item.search_term,
        result: item.result ?? defaultResult,
        size: item.size ?? size,
      };
    };

    const url = `${API_BASE}/v3/lookup/batch`;
    const resp = await fetch(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ lookups: lookups.map(normalise) }),
    });

    if (!resp.ok)
      throw new Error(`API request failed: ${resp.status} ${resp.statusText}`);

    return JSON.parse(await resp.text());
  }

  /**
   * Fetch a PhyloPic silhouette record for a single taxon.
   * @param {string} taxonId - NCBI taxon ID (required)
   * @param {string} [taxonomy="ncbi"] - Taxonomy name
   * @returns {Promise<object|null>} - Silhouette record, or null when none found
   */
  async phylopic(taxonId, taxonomy = "ncbi") {
    if (!taxonId) throw new Error("phylopic() requires a taxonId parameter");

    const params = new URLSearchParams({ taxon_id: taxonId, taxonomy });
    const url = `${API_BASE}/v3/phylopic?${params.toString()}`;

    const resp = await fetch(url, { method: "GET" });
    if (!resp.ok)
      throw new Error(`API request failed: ${resp.status} ${resp.statusText}`);

    return JSON.parse(_parsePhylopicJson(await resp.text()));
  }

  /**
   * Fetch PhyloPic silhouette records for multiple taxa in one request.
   * @param {string[]} taxonIds - List of NCBI taxon IDs (1–200, required)
   * @param {string} [taxonomy="ncbi"] - Taxonomy name
   * @returns {Promise<object[]>} - Array of silhouette records each with a taxonId field
   */
  async phylopicBatch(taxonIds, taxonomy = "ncbi") {
    if (!taxonIds || taxonIds.length === 0)
      throw new Error("phylopicBatch() requires at least one taxon ID");
    if (taxonIds.length > 200)
      throw new Error(
        "phylopicBatch() accepts at most 200 taxon IDs per request",
      );

    const resp = await fetch(`${API_BASE}/v3/phylopic/batch`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ taxon_ids: taxonIds, taxonomy }),
    });
    if (!resp.ok)
      throw new Error(`API request failed: ${resp.status} ${resp.statusText}`);

    return JSON.parse(_parsePhylopicBatchJson(await resp.text()));
  }

  /**
   * Fetch aggregated metadata in a single request.
   * @returns {Promise<object>} - Object with indices, taxonomies, ranks, and versions keys
   */
  async metadata() {
    const resp = await fetch(`${API_BASE}/v3/metadata`, { method: "GET" });
    if (!resp.ok)
      throw new Error(`API request failed: ${resp.status} ${resp.statusText}`);
    const data = await resp.json();
    return Object.fromEntries(
      ["indices", "taxonomies", "ranks", "versions"]
        .filter((k) => k in data)
        .map((k) => [k, data[k]]),
    );
  }

  /**
   * Return the list of available index names.
   * @returns {Promise<string[]>} - Array of index name strings
   */
  async indices() {
    const resp = await fetch(`${API_BASE}/v3/metadata/indices`, {
      method: "GET",
    });
    if (!resp.ok)
      throw new Error(`API request failed: ${resp.status} ${resp.statusText}`);
    return (await resp.json()).indices ?? [];
  }

  /**
   * Return field metadata for the given index.
   * @param {string} index - Index name, e.g. "taxon" or "assembly" (required)
   * @returns {Promise<object>} - Object mapping field name to field metadata
   */
  async fields(index) {
    if (!index) throw new Error("fields() requires an index parameter");
    const params = new URLSearchParams({ result: index });
    const resp = await fetch(
      `${API_BASE}/v3/metadata/fields?${params.toString()}`,
      { method: "GET" },
    );
    if (!resp.ok)
      throw new Error(`API request failed: ${resp.status} ${resp.statusText}`);
    return (await resp.json()).fields ?? {};
  }

  /**
   * Return the list of available taxonomy names.
   * @returns {Promise<string[]>} - Array of taxonomy name strings
   */
  async taxonomies() {
    const resp = await fetch(`${API_BASE}/v3/metadata/taxonomies`, {
      method: "GET",
    });
    if (!resp.ok)
      throw new Error(`API request failed: ${resp.status} ${resp.statusText}`);
    return (await resp.json()).taxonomies ?? [];
  }

  /**
   * Return the list of recognised taxonomic rank names.
   * @returns {Promise<string[]>} - Array of rank name strings
   */
  async ranks() {
    const resp = await fetch(`${API_BASE}/v3/metadata/ranks`, {
      method: "GET",
    });
    if (!resp.ok)
      throw new Error(`API request failed: ${resp.status} ${resp.statusText}`);
    return (await resp.json()).ranks ?? [];
  }

  /**
   * Fetch summary aggregations for specific fields.
   * @param {string} recordId - Record ID to summarize
   * @param {string} fields - Comma-separated field names to summarize
   * @param {string} [result] - Result type (taxon|assembly|sample), default from index
   * @param {string} [summaryTypes="min,max,mean"] - Summary types to compute
   * @returns {Promise<object>} - Parsed summary object
   */
  async summary(recordId, fields, result = null, summary = "histogram") {
    if (!recordId) throw new Error("summary() requires a recordId parameter");
    if (!fields) throw new Error("summary() requires a fields parameter");

    const resultType = result || this._index || "taxon";
    const params = new URLSearchParams({
      recordId,
      result: resultType,
      fields,
      summary,
    });
    const url = `${API_BASE}/v3/summary?${params.toString()}`;

    const resp = await fetch(url, { method: "GET" });

    if (!resp.ok)
      throw new Error(`API request failed: ${resp.status} ${resp.statusText}`);

    return JSON.parse(await resp.text());
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

    // field_meta.json is per-index: {"taxon": {...}, "assembly": {...}}
    // Extract the slice for this query's index so the validator sees only
    // the fields valid for this index rather than a flat cross-index union.
    let indexFieldMetadataJson = fieldMetadataJson;
    try {
      const parsed = JSON.parse(fieldMetadataJson);
      const firstVal = parsed && Object.values(parsed)[0];
      if (
        firstVal &&
        typeof firstVal === "object" &&
        !Array.isArray(firstVal)
      ) {
        indexFieldMetadataJson = JSON.stringify(parsed[this._index] || {});
      }
    } catch {
      // keep original if parse fails
    }

    const result = _validateQueryJson(
      this.toQueryYaml(),
      indexFieldMetadataJson,
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
 * Build report configurations for v3 /report POST calls.
 *
 * @example
 * const rb = new ReportBuilder("histogram").setX("genome_size").setRank("species");
 * const data = await qb.report(rb);
 */
class ReportBuilder {
  /** @param {string} reportType - Report type (e.g. "histogram", "scatter", "countPerRank") */
  constructor(reportType) {
    this._doc = { report: reportType };
    // Set by QueryBuilder.fromV2Url() for report URLs
    this._reportYamlOverride = null;
    this._embeddedQueryBuilder = null;
    // Set via setDisplay(); passed as the `display` key in the POST body.
    this._display = null;
    // Set via setIncludePlotSpec(); requests a PlotSpec in the response.
    this._includePlotSpec = false;
  }

  /** Set the X-axis field. @param {string} field @param {string} [opts=""] @returns {this} */
  setX(field, opts = "") {
    this._doc.x = field;
    if (opts) this._doc.x_opts = opts;
    return this;
  }

  /** Set the Y-axis field or fields. @param {string|string[]} field @param {string} [opts=""] @returns {this} */
  setY(field, opts = "") {
    this._doc.y = field;
    if (opts) this._doc.y_opts = opts;
    return this;
  }

  /** Set the category breakdown field. @param {string} field @param {string} [opts=""] @returns {this} */
  setCat(field, opts = "") {
    this._doc.cat = field;
    if (opts) this._doc.cat_opts = opts;
    return this;
  }

  /** Set the query field (countPerRank reports). @param {string} field @returns {this} */
  setQuery(field) {
    this._doc.query = field;
    return this;
  }

  /** Set the taxonomic rank to aggregate at. @param {string} rank @returns {this} */
  setRank(rank) {
    this._doc.rank = rank;
    return this;
  }

  /** Set the list of taxonomic ranks (countPerRank reports). @param {string[]} ranks @returns {this} */
  setRanks(ranks) {
    this._doc.ranks = ranks;
    return this;
  }

  /** Set additional fields to include in results. @param {string[]} fields @returns {this} */
  setFields(fields) {
    this._doc.fields = fields;
    return this;
  }

  /** Filter by assembly/sample status. @param {string} value @returns {this} */
  setStatusFilter(value) {
    this._doc.status_filter = value;
    return this;
  }

  /** Set the rank for category label aggregation. @param {string} rank @returns {this} */
  setCatRank(rank) {
    this._doc.cat_rank = rank;
    return this;
  }

  /** Collapse monotypic nodes in tree reports. @param {boolean} [value=true] @returns {this} */
  setCollapseMonotypic(value = true) {
    this._doc.collapse_monotypic = value;
    return this;
  }

  /** Preserve this rank when collapsing monotypic nodes. @param {string} rank @returns {this} */
  setPreserveRank(rank) {
    this._doc.preserve_rank = rank;
    return this;
  }

  /** Set the rank to count descendants at (tree reports). @param {string} rank @returns {this} */
  setCountRank(rank) {
    this._doc.count_rank = rank;
    return this;
  }

  /** Set the geographic location field (map reports). @param {string} field @returns {this} */
  setLocationField(field) {
    this._doc.location_field = field;
    return this;
  }

  /** Set the geohash resolution for map reports (1-12). @param {number} resolution @returns {this} */
  setHexResolution(resolution) {
    this._doc.hex_resolution = resolution;
    return this;
  }

  /** Set the max map points before switching to hexbin mode. @param {number} threshold @returns {this} */
  setMapThreshold(threshold) {
    this._doc.map_threshold = threshold;
    return this;
  }

  /** Set the max scatter points before switching to binned mode. @param {number} threshold @returns {this} */
  setScatterThreshold(threshold) {
    this._doc.scatter_threshold = threshold;
    return this;
  }

  // ── Arc report methods ─────────────────────────────────────────────────

  /** Set the feature filter (numerator) for an arc report. @param {string} term @returns {this} */
  setFeature(term) {
    this._doc.feature = term;
    return this;
  }

  /** Set the reference filter (denominator) for an arc report. @param {string} term @returns {this} */
  setReference(term) {
    this._doc.reference = term;
    return this;
  }

  /** Set the context filter (enables arc2 ratio) for an arc report. @param {string} term @returns {this} */
  setContext(term) {
    this._doc.context = term;
    return this;
  }

  /**
   * Add a concentric ring to a multi-ring arc report.
   * @param {string} featureTerm - Filter for this ring's numerator.
   * @param {object} [opts] - Options: { referenceTerm, label }
   * @returns {this}
   */
  addRing(featureTerm, opts = {}) {
    const ring = { feature: featureTerm };
    if (opts.referenceTerm != null) ring.reference = opts.referenceTerm;
    if (opts.label != null) ring.label = opts.label;
    if (!this._doc.rings) this._doc.rings = [];
    this._doc.rings.push(ring);
    return this;
  }

  /**
   * Run the same feature/reference arc once per taxonomic rank.
   * @param {string[]} ranks - Rank names, e.g. ["genus", "family", "order"].
   * @returns {this}
   */
  setArcRanks(ranks) {
    this._doc.ranks = [...ranks];
    return this;
  }

  /**
   * Set custom boundaries for a histogram axis (x, y, or cat).
   *
   * For numeric axes, boundaries define explicit breakpoints. For date axes,
   * provide ISO 8601 date strings or interval names ("week", "month", "quarter").
   *
   * @param {string} axisRole - Axis to configure: "x", "y", or "cat".
   * @param {(number|string)[]} boundaries - For numeric: floats in ascending order.
   *                                          For date: ISO 8601 strings or interval names.
   * @param {object} [opts] - Options: { labels: string[] }
   * @returns {this}
   */
  setAxisBoundaries(axisRole, boundaries, opts = {}) {
    const key = `${axisRole}_opts`;
    if (!(key in this._doc)) this._doc[key] = {};
    this._doc[key].boundaries = boundaries;
    if (opts.labels != null) this._doc[key].labels = opts.labels;
    return this;
  }

  /**
   * Set date-based intervals for a date-scaled axis.
   *
   * Convenience method for setting standard calendar intervals on a date axis.
   * Intervals are expanded server-side to boundaries for the current time window.
   *
   * @param {string} axisRole - Axis to configure: "x", "y", or "cat".
   * @param {string[]} intervals - Interval names, e.g. ["week", "month", "quarter"].
   * @returns {this}
   */
  setAxisDateIntervals(axisRole, intervals) {
    const key = `${axisRole}_opts`;
    if (!(key in this._doc)) this._doc[key] = {};
    this._doc[key].boundaries = { intervals };
    return this;
  }

  /**
   * Set display/presentation options for this report.
   * Accepts either an object or a YAML string.
   * @param {object|string} value - Display options (title, width, height, colorScheme, etc.)
   * @returns {this}
   */
  setDisplay(value) {
    this._display = value;
    return this;
  }

  /**
   * Request a `plot_spec` field in the API response.
   *
   * When set to `true`, the server builds and returns a fully-resolved
   * `PlotSpec` object alongside the raw report data.  Pass the result to
   * `plotSpecToVegaLite()` to produce a Vega-Lite specification.
   *
   * @param {boolean} [value=true]
   * @returns {this}
   */
  setIncludePlotSpec(value = true) {
    this._includePlotSpec = value;
    return this;
  }

  /**
   * Return the report configuration as a YAML string.
   * @returns {string}
   */
  toReportYaml() {
    if (this._reportYamlOverride !== null) return this._reportYamlOverride;
    // Simple YAML serialisation sufficient for report config (no complex types)
    const lines = [];
    for (const [key, val] of Object.entries(this._doc)) {
      if (Array.isArray(val)) {
        lines.push(`${key}:`);
        for (const item of val) lines.push(`- ${item}`);
      } else if (typeof val === "boolean") {
        lines.push(`${key}: ${val}`);
      } else if (typeof val === "number") {
        lines.push(`${key}: ${val}`);
      } else {
        lines.push(`${key}: ${val}`);
      }
    }
    return lines.join("\n") + "\n";
  }

  /**
   * Return an array of validation error strings (empty = valid).
   * @param {object|null} [fieldMeta=null] - Optional field metadata map
   * @returns {string[]}
   */
  validate(fieldMeta = null) {
    const fieldMetaJson = JSON.stringify(fieldMeta || {});
    const result = _validateReportYaml(this.toReportYaml(), fieldMetaJson);
    try {
      return JSON.parse(result);
    } catch {
      return [result];
    }
  }

  /**
   * Execute this report against a QueryBuilder's query.
   * @param {QueryBuilder} queryBuilder
   * @param {string} [apiBase=API_BASE]
   * @returns {Promise<object>}
   */
  async run(queryBuilder, apiBase = API_BASE) {
    const qb = queryBuilder ?? this._embeddedQueryBuilder;
    if (!qb)
      throw new Error(
        "run() requires a QueryBuilder argument or a ReportBuilder created via QueryBuilder.fromV2Url()",
      );
    return qb.report(this, apiBase);
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
function toTidyRecords(records, lineageSummary) {
  const str = typeof records === "string" ? records : JSON.stringify(records);
  if (lineageSummary !== undefined && lineageSummary !== null) {
    // lineageSummary is already joined via toFlatRecords; just reshape
  }
  return JSON.parse(_toTidyRecords(str));
}

/**
 * Fetch results and return as flat records, optionally joining lineage summary columns.
 *
 * @param {string|object} raw - Raw API response string or object.
 * @param {string} configJson - JSON string: `{"rank": {"field": "mode"}}` config.
 * @returns {object[]}
 */
function parseSearchWithLineageSummary(raw, configJson) {
  const str = typeof raw === "string" ? raw : JSON.stringify(raw);
  return JSON.parse(_parseSearchWithLineageSummary(str, configJson));
}

/**
 * Convert a `PlotSpec` object (from the API `plot_spec` field) to a
 * Vega-Lite v5 specification.
 *
 * The returned object can be passed directly to `vegaEmbed()` or any other
 * Vega-Lite renderer.
 *
 * @param {object} plotSpec - A `PlotSpec` from the API response.
 * @returns {object} Vega-Lite JSON specification.
 */
function plotSpecToVegaLite(plotSpec) {
  const display = plotSpec.display ?? {};
  const base = {
    $schema: "https://vega.github.io/schema/vega-lite/v5.json",
    title: display.title ?? undefined,
    width: display.width ?? 600,
    height: display.height ?? 400,
    config: _vegaConfig(display),
  };
  switch (plotSpec.report_type) {
    case "histogram":
      return _histogramSpec(plotSpec, base);
    case "scatter":
      return _scatterSpec(plotSpec, base);
    case "count_per_rank":
      return _barSpec(plotSpec, base);
    case "sources":
      return _barSpec(plotSpec, base);
    case "tree":
      return { ...base, mark: "point", data: { values: [] } };
    case "map":
      return {
        ...base,
        projection: { type: display.map?.projection ?? "mercator" },
      };
    case "arc":
      return { ...base, mark: "arc" };
    default:
      return base;
  }
}

function _vegaConfig(display) {
  const fontSize = display.font_size ?? 12;
  return {
    axis: { labelFontSize: fontSize, titleFontSize: fontSize },
    legend: { labelFontSize: fontSize },
  };
}

function _histogramSpec(plotSpec, base) {
  const x = plotSpec.x ?? {};
  const hist = plotSpec.display?.histogram ?? {};
  return {
    ...base,
    data: { values: plotSpec.data?.buckets ?? [] },
    mark: { type: "bar" },
    encoding: {
      x: {
        field: "key",
        type: "quantitative",
        scale: { type: x.scale === "log10" ? "log" : "linear" },
        axis: { title: x.label ?? x.field ?? "" },
      },
      y: {
        field: "doc_count",
        type: "quantitative",
        scale: { type: hist.y_scale === "log10" ? "log" : "linear" },
        axis: { title: plotSpec.display?.y_label ?? "Count" },
      },
    },
  };
}

function _scatterSpec(plotSpec, base) {
  const x = plotSpec.x ?? {};
  const y = plotSpec.y ?? {};
  return {
    ...base,
    data: { values: plotSpec.data?.cells ?? [] },
    mark: "point",
    encoding: {
      x: {
        field: "x",
        type: "quantitative",
        scale: { type: x.scale === "log10" ? "log" : "linear" },
        axis: { title: x.label ?? x.field ?? "" },
      },
      y: {
        field: "y",
        type: "quantitative",
        scale: { type: y.scale === "log10" ? "log" : "linear" },
        axis: { title: y.label ?? y.field ?? "" },
      },
    },
  };
}

function _barSpec(plotSpec, base) {
  const x = plotSpec.x ?? {};
  return {
    ...base,
    data: { values: plotSpec.data?.buckets ?? [] },
    mark: "bar",
    encoding: {
      y: {
        field: x.field ?? "rank",
        type: "nominal",
        axis: { title: x.label ?? x.field ?? "" },
      },
      x: { field: "count", type: "quantitative" },
    },
  };
}

/**
 * Build a PlotSpec from local delimited text content without an API call.
 *
 * Auto-detects column types from the data: columns where every non-empty value
 * is numeric are treated as numbers; everything else is treated as strings.
 *
 * @param {string} content - Full text of the TSV/CSV file.
 * @param {string} reportType - One of "histogram", "scatter", or "bar".
 * @param {Record<string, string>} [columnMap={}] - Mapping of axis roles to
 *   column names, e.g. `{ x: "genome_size", y: "c_value" }`.  Pass `{}` to
 *   use positional defaults (first column → x, second → y).
 * @param {Record<string, unknown>} [display={}] - Display options (title,
 *   width, height, etc.).
 * @param {string} [delimiter="\t"] - Field separator: `"\t"` for TSV, `","` for CSV.
 * @returns {Record<string, unknown>} PlotSpec object compatible with
 *   {@link plotSpecToVegaLite}.
 * @throws {Error} When the report type is unknown, a required column is
 *   missing, or a numeric-axis column contains non-numeric data.
 */
function localPlotSpec(
  content,
  reportType = "histogram",
  columnMap = {},
  display = {},
  delimiter = "\t",
) {
  const result = _localPlotSpecJson(
    content,
    reportType,
    JSON.stringify(columnMap),
    JSON.stringify(display),
    delimiter,
  );
  const parsed = JSON.parse(result);
  if (parsed.error) {
    throw new Error(parsed.error);
  }
  return parsed;
}

/**
 * Merge annotation dicts into `plotSpec.data.rows` by a shared key.
 *
 * For each row in `plotSpec.data.rows` whose value for `joinKey` matches an
 * annotation entry, the annotation's fields are added to the row (annotation
 * fields take precedence on key collision).  Rows with no matching annotation
 * are left unchanged.
 *
 * @param {Record<string, unknown>} plotSpec - A PlotSpec object (from the API
 *   or from {@link localPlotSpec}).
 * @param {Record<string, unknown>[]} annotations - Array of annotation objects,
 *   each containing at least `joinKey` and the fields to add.
 * @param {string} joinKey - Column name used to match rows to annotations.
 * @returns {Record<string, unknown>} The modified `plotSpec` (same object).
 */
function mergeAnnotations(plotSpec, annotations, joinKey) {
  const index = Object.fromEntries(
    annotations.filter((a) => joinKey in a).map((a) => [a[joinKey], a]),
  );
  const rows = (plotSpec.data ?? {}).rows ?? [];
  for (const row of rows) {
    const keyVal = row[joinKey];
    if (keyVal != null && keyVal in index) {
      Object.assign(row, index[keyVal]);
    }
  }
  return plotSpec;
}

export {
  QueryBuilder,
  ReportBuilder,
  parseSearchJson,
  parseResponseStatus,
  parseHistogramJson,
  parseTreeJson,
  plotSpecToVegaLite,
  localPlotSpec,
  mergeAnnotations,
  annotateSourceLabels,
  splitSourceColumns,
  valuesOnly,
  annotatedValues,
  toTidyRecords,
  parseSearchWithLineageSummary,
};
