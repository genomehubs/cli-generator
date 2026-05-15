"""Type stubs for the ``cli_generator`` Rust extension module.

Keep this file in sync with the ``#[pyfunction]`` exports in ``src/lib.rs``.
Pyright (and Pylance) use these stubs for type checking — the compiled ``.so``
file does not need to be present during static analysis.

When adding a new Rust function:
1. Add the ``#[pyfunction]`` in ``src/lib.rs``.
2. Register it with ``m.add_function(...)`` in the ``#[pymodule]``.
3. Add a typed signature here.
4. Add it to ``__init__.py``'s imports and ``__all__``.
"""

def version() -> str:
    """Return the cli-generator version string (e.g. ``"0.1.0"``).

    The value is read from the ``CARGO_PKG_VERSION`` environment variable at
    compile time and is therefore always in sync with ``Cargo.toml``.

    Returns:
        Package version string.
    """
    ...

def build_url(
    query_yaml: str,
    params_yaml: str,
    api_base: str,
    api_version: str,
    endpoint: str,
) -> str:
    """Build a fully-encoded genomehubs API query URL from YAML inputs.

    Args:
        query_yaml: YAML-serialised ``SearchQuery`` (index, taxa, attributes, …).
        params_yaml: YAML-serialised ``QueryParams`` (size, page, sort, …).
        api_base: Base URL without trailing slash, e.g.
            ``"https://goat.genomehubs.org/api"``.
        api_version: API version path component, e.g. ``"v2"``.
        endpoint: Endpoint name, e.g. ``"search"`` or ``"count"``.

    Returns:
        Fully-encoded URL ready for an HTTP GET request.

    Raises:
        ValueError: If either YAML string cannot be parsed.
    """
    ...

def build_ui_url(
    query_yaml: str,
    params_yaml: str,
    ui_base: str,
    endpoint: str,
) -> str:
    """Build a fully-encoded genomehubs UI URL from YAML inputs.

    Produces the same query parameters as ``build_url`` but targets the web
    interface rather than the REST API.  No version component is inserted;
    the result is ``{ui_base}/{endpoint}?result=…&query=…``.

    Args:
        query_yaml: YAML-serialised ``SearchQuery`` (index, taxa, attributes, …).
        params_yaml: YAML-serialised ``QueryParams`` (size, page, sort, …).
        ui_base: Base URL of the web UI without trailing slash, e.g.
            ``"https://goat.genomehubs.org"``.
        endpoint: Endpoint name, e.g. ``"search"``.

    Returns:
        Fully-encoded UI URL string.

    Raises:
        ValueError: If either YAML string cannot be parsed.
    """
    ...

def render_snippet(
    snapshot_json: str,
    site_name: str,
    api_base: str,
    sdk_name: str,
    languages: str = "python",
) -> str:
    """Render code snippets for a query in one or more languages.

    Args:
        snapshot_json: JSON-serialised ``QuerySnapshot`` containing filters, sorts,
            selections, and other query components.
        site_name: Short identifier for the target site, e.g. ``"goat"``.
        api_base: Base URL of the API, e.g. ``"https://goat.genomehubs.org/api"``.
        sdk_name: Import name of the generated SDK package, e.g. ``"goat_sdk"``.
        languages: Comma-separated list of language codes to render, e.g.
            ``"python"`` or ``"python,r"`` (default: ``"python"``).  Each code
            must match a loaded snippet template.

    Returns:
        JSON object string mapping language name to rendered code snippet, e.g.
        ``'{"python": "import goat_sdk as sdk\\n..."}'``.

    Raises:
        ValueError: If the snapshot JSON cannot be parsed.
        RuntimeError: If template rendering fails.
    """
    ...

def describe_query(
    query_yaml: str,
    params_yaml: str,
    field_metadata_json: str,
    mode: str = "concise",
) -> str:
    """Describe a query in human-readable form.

    Args:
        query_yaml: YAML-serialised ``SearchQuery``.
        params_yaml: YAML-serialised ``QueryParams``.
        field_metadata_json: JSON-serialised field metadata dictionary mapping field names
            to metadata objects. Pass ``"{}"`` for no metadata (uses canonical names).
        mode: Output format — ``"concise"`` for a one-line summary, ``"verbose"`` for
            a detailed breakdown with bullet points (default: ``"concise"``).

    Returns:
        English prose description of the query.

    Raises:
        ValueError: If any YAML or JSON string cannot be parsed.
    """
    ...

def parse_response_status(raw: str) -> str:
    """Parse the ``status`` block from a raw genomehubs API JSON response.

    Args:
        raw: Raw JSON response body from the genomehubs API.

    Returns:
        Compact JSON string of the form
        ``'{"hits":N,"ok":true|false,"error":null|"msg"}'``.
        On completely invalid input returns an error-flagged object rather
        than raising.
    """
    ...

def parse_search_json(raw: str) -> str:
    """Parse a raw genomehubs ``/search`` JSON response into a flat record array.

    Each element of the returned array corresponds to one result record with:

    - Identity columns: ``taxon_id``, ``assembly_id``, or ``sample_id``;
      ``scientific_name``; ``taxon_rank``.
    - ``{field}`` — representative value (``null`` for stub fields with no value).
    - ``{field}__source`` — ``"direct"``, ``"ancestor"``, or ``"descendant"``
      (taxon index only; ``null`` for assembly/sample).
    - Stat sub-keys present on the raw object: ``{field}__min``, ``{field}__max``,
      ``{field}__median``, ``{field}__mode``, ``{field}__mean``, ``{field}__count``,
      ``{field}__sp_count``, ``{field}__from``, ``{field}__to``, ``{field}__length``.

    Args:
        raw: Raw JSON response body from the genomehubs ``/search`` endpoint.

    Returns:
        Compact JSON array string.  Parse with ``json.loads()`` or pass directly
        to ``polars.read_json()`` / ``pandas.read_json()``.
    """
    ...

def annotate_source_labels(records_json: str, mode: str = "non_direct") -> str:
    """Add ``{field}__label`` columns to already-flat parsed records.

    Operates on the output of :func:`parse_search_json` without re-parsing the
    raw HTTP response.

    Label format examples:

    - Direct value ``3.4`` → ``"3.4"`` (all modes)
    - Descendant value ``57`` → ``"57 (Descendant)"`` (non_direct, all modes)
    - Ancestral value ``2`` → ``"2 (Ancestral)"`` (all three modes)
    - List value ``["A", "B"]`` → ``"A, B"`` (joined with comma)

    Args:
        records_json: JSON array string from :func:`parse_search_json`.
        mode: One of:

            - ``"non_direct"`` (default) — annotate descendant and ancestral only.
            - ``"ancestral_only"`` — annotate ancestral values only.
            - ``"all"`` — annotate every value including direct.

    Returns:
        Compact JSON array string with ``{field}__label`` columns added.
    """
    ...

def split_source_columns(records_json: str) -> str:
    """Reshape flat parsed records into split-source columns.

    Operates on the output of :func:`parse_search_json` without re-parsing the
    raw HTTP response.  Each ``{field}`` / ``{field}__source`` pair is replaced
    by three columns:

    - ``{field}__direct`` — value when source is ``"direct"``, else ``null``.
    - ``{field}__descendant`` — value when source is ``"descendant"``, else ``null``.
    - ``{field}__ancestral`` — value when source is ``"ancestor"``, else ``null``.

    All other columns (stat sub-keys, ``__length``, identity fields) are kept
    unchanged.

    Args:
        records_json: JSON array string from :func:`parse_search_json`.

    Returns:
        Compact JSON array string with split-source columns.
    """
    ...

def values_only(records_json: str, keep_columns_json: str = "") -> str:
    """Strip all ``__*`` sub-key columns from flat records.

    Operates on the output of :func:`parse_search_json`.  All metadata columns
    whose names contain ``__`` (``{field}__source``, ``{field}__min``,
    ``{field}__label``, ``{field}__direct``, etc.) are removed.  Identity columns
    (``taxon_id``, ``scientific_name``, ``taxon_rank``, …) and bare ``{field}``
    value columns are preserved.

    ``keep_columns_json`` is a JSON array of ``__*`` column names to **preserve**
    despite containing ``__``.  Use this when the caller requested a specific
    summary statistic via ``field:modifier`` syntax, e.g.::

        keep = json.dumps(qb.field_modifiers())
        rows = json.loads(values_only(flat_json, keep))

    Pass ``""`` or ``"[]"`` to strip all ``__*`` columns (default behaviour).

    Args:
        records_json: JSON array string from :func:`parse_search_json`.
        keep_columns_json: JSON array of column names to preserve, e.g.
            ``'["assembly_span__min"]'``.  Default ``""`` strips all.

    Returns:
        Compact JSON array string containing only identity and value columns.
    """
    ...

def annotated_values(records_json: str, mode: str = "non_direct", keep_columns_json: str = "") -> str:
    """Return records with non-direct values replaced by their annotated label.

    Chains :func:`annotate_source_labels` then promotes each ``{field}__label``
    into ``{field}``, then strips all remaining ``__*`` metadata columns.

    ``keep_columns_json`` works identically to :func:`values_only` — pass a
    JSON array of column names to preserve specific stat sub-keys after label
    promotion, e.g. ``'["assembly_span__min"]'``.

    Fields that have no label (e.g. ``"direct"`` source in ``"non_direct"`` mode)
    keep their original numeric/string value.

    Example output column for an ancestral genome_size::

        genome_size = "8215200000 (Ancestral)"

    Args:
        records_json: JSON array string from :func:`parse_search_json`.
        mode: One of ``"non_direct"`` (default), ``"ancestral_only"``, or ``"all"``.
        keep_columns_json: JSON array of column names to preserve, e.g.
            ``'["assembly_span__min"]'``.  Default ``""`` strips all.

    Returns:
        Compact JSON array string with labelled values and no ``__*`` columns.
    """
    ...

def to_tidy_records(records_json: str) -> str:
    """Reshape flat records into long/tidy format.

    Accepts the JSON array produced by :func:`parse_search_json` and returns a
    JSON array with one row *per field per source record*.  Each output row
    contains:

    - Identity columns present in the source record (``taxon_id``,
      ``scientific_name``, ``taxon_rank``, ``assembly_id``, ``sample_id``).
    - ``"field"`` — the bare field name (e.g. ``"genome_size"``).
    - ``"value"`` — the representative value for that field.
    - ``"source"`` — aggregation source (``"direct"``, ``"ancestor"``,
      ``"descendant"``, or ``null``).

    Explicitly-requested modifier columns (from ``field:modifier`` requests,
    e.g. ``assembly_span__min``) are emitted as separate rows with ``"field"``
    set to ``"{bare}:{modifier}"`` and ``"source"`` as ``null``.

    This matches the shape of the GoaT API's ``tidydata`` TSV format and is
    the natural input for ``pandas.melt`` or R's ``tidyr::pivot_longer``.

    Args:
        records_json: JSON array string from :func:`parse_search_json`.

    Returns:
        Compact JSON array string in tidy (long) format.
    """
    ...

def parse_search_with_lineage_summary(raw: str, config_json: str) -> str:
    """Parse a raw genomehubs ``/search`` JSON response and join lineage summary
    aggregations as extra flat columns on every record.

    ``raw`` must be the full API response from a query that included
    ``lineage_rank_summary``.  The ``lineage_summary`` block produced by the
    API is automatically located and joined against ``result.ranks`` for each
    row.

    ``config_json`` is a JSON object controlling how each field's distribution
    is reduced to one or more flat columns:

    .. code-block:: json

        {
          "genus": {
            "assembly_level": "top",
            "genome_size": "stats",
            "assembly_date": ["min", "max"]
          }
        }

    Supported modes:

    - ``"top"`` — most common keyword value (``null`` for numeric/date)
    - ``"top_n:<N>"`` — top-N keyword values as a JSON array
    - ``"all"`` — full distribution object
    - ``"count"`` — distinct value count
    - ``"min"`` / ``"max"`` / ``"avg"`` — individual stats fields
    - ``"stats"`` — shorthand for all four stats (four columns)

    Column naming:

    - ``top`` / ``top_n`` / ``all`` → ``{rank}_{field}``
    - ``count`` → ``{rank}_{field}__count``
    - ``min`` / ``max`` / ``avg`` → ``{rank}_{field}__min`` etc.
    - ``stats`` → ``{rank}_{field}__min``, ``__max``, ``__avg``, ``__count``

    A missing ancestor or a field with no data always produces ``null``.

    Args:
        raw: Raw JSON string from the ``/search`` endpoint (full response).
        config_json: JSON object specifying rank → field → mode(s).

    Returns:
        Compact JSON array string of flat records with lineage summary columns
        appended.
    """
    ...

def parse_paginated_json(raw: str) -> str:
    """Parse one page from a ``/searchPaginated`` API response.

    Returns a JSON object:

    .. code-block:: json

        {
          "records": [...],
          "hasMore": true,
          "searchAfter": [...],
          "totalHits": 5000
        }

    ``records`` contains flat records in the same format as
    :func:`parse_search_json`.  Pass ``searchAfter`` as the cursor for the
    next request.  ``hasMore`` is ``false`` on the final page.

    Args:
        raw: Raw JSON string from the ``/searchPaginated`` endpoint.

    Returns:
        JSON object string with ``records``, ``hasMore``, ``searchAfter``,
        and ``totalHits``.
    """
    ...

def parse_batch_json(raw: str) -> str:
    """Parse a raw batch search (``/msearch``) response into per-query flat record lists.

    The genomehubs ``/msearch`` endpoint accepts multiple queries in a single POST
    and returns results grouped by query.  This function parses that envelope.

    Returns a JSON object:

    .. code-block:: json

        {
          "results": [
            {"records": [...], "total": 5200, "error": null},
            {"records": [...], "total": 7300, "error": null}
          ],
          "totalHits": 12500
        }

    Each ``records`` array is in the same flat format as :func:`parse_search_json`
    output.  Results are in the same order as the request's ``searches`` array.

    Args:
        raw: Raw JSON string from the ``/msearch`` endpoint.

    Returns:
        JSON object string with ``results`` (array of per-query objects each
        containing ``records``, ``total``, and ``error``) and ``totalHits``.
    """
    ...

def parse_busco_tsv(assembly_id: str, content: str) -> str:
    """Parse a BUSCO ``full_table.tsv`` file into a JSON-encoded ``LocalFeatureSet``.

    Only ``Complete`` and ``Duplicated`` entries are included.  For ``Duplicated``
    genes, the instance with the highest score is kept.  ``Fragmented`` and
    ``Missing`` entries are discarded.

    The returned object has ``sequence_lengths`` unpopulated.  Call
    :func:`parse_fai` or :func:`parse_lengths_tsv` to populate it, or pass
    the object directly to :func:`positional_from_features` (which will call
    ``derive_lengths()`` automatically as a fallback).

    Args:
        assembly_id: User-supplied label for this assembly (e.g. ``"my_assembly"``).
        content:     Full text of the BUSCO ``full_table.tsv`` file.

    Returns:
        JSON string (``LocalFeatureSet`` object) on success,
        or ``{"error":"<message>"}`` on parse failure.
    """
    ...

def parse_cat_file(content: str) -> str:
    """Parse a two-column name\u2192category mapping file.

    Each line should be ``feature_name<TAB>category_label``.  Leading ``#``
    comment lines and blank lines are skipped.  Useful for overriding the
    ``cat`` field on features after parsing a BUSCO or feature TSV.

    Args:
        content: Full text of the two-column mapping file.

    Returns:
        JSON object string mapping ``feature_name \u2192 category`` on success,
        or ``{"error":"<message>"}`` on failure.
    """
    ...

def parse_fai(content: str) -> str:
    """Parse a samtools ``.fai`` FASTA index and return a JSON sequence-length map.

    Only the first two columns (``NAME``, ``LENGTH``) are used; the remaining
    three offset columns are ignored.

    Typical usage::

        import json
        from cli_generator import parse_busco_tsv, parse_fai

        feature_set = json.loads(parse_busco_tsv("my_asm", open("full_table.tsv").read()))
        feature_set["sequence_lengths"] = json.loads(parse_fai(open("genome.fa.fai").read()))
        feature_set["lengths_derived"] = False

    Args:
        content: Full text of the ``.fai`` file.

    Returns:
        JSON object string mapping ``sequence_id → length`` on success,
        or ``{"error":"<message>"}`` on parse failure.
    """
    ...

def parse_lengths_tsv(content: str) -> str:
    """Parse a two-column ``sequence_id<TAB>length`` TSV file.

    Blank lines and ``#`` comment lines are skipped.  This is the fallback
    length source when a ``.fai`` file is not available.

    Args:
        content: Full text of the lengths TSV file.

    Returns:
        JSON object string mapping ``sequence_id → length`` on success,
        or ``{"error":"<message>"}`` on parse failure.
    """
    ...

def positional_from_features(
    feature_sets_json: str,
    report_type: str,
    reorient: bool = True,
    cat_field: str = "",
    window_size: int = 0,
    max_connections_per_group: int = 0,
    regions_json: str = "",
) -> str:
    """Compute a positional report from local feature sets (no API call required).

    Builds an Oxford dot-plot, ribbon, or painting diagram from one or more
    ``LocalFeatureSet`` objects parsed from local files.  No HTTP request is made.

    Sequence lengths are populated automatically via ``derive_lengths()`` when
    the ``sequence_lengths`` map is empty, setting ``lengthsDerived: true``
    in the output assembly metadata and making axis proportions approximate.

    Args:
        feature_sets_json:         JSON array of ``LocalFeatureSet`` objects.
                                   The first element is the reference assembly.
        report_type:               One of ``"oxford"``, ``"ribbon"``, or ``"painting"``.
        reorient:                  Auto-orient comparison sequences (default ``True``).
        cat_field:                 Category field name for colour coding (``""`` for none).
        window_size:               Bin size in bp for painting (``0`` for individual positions).
        max_connections_per_group: Cap on M:N connections (``0`` → default 25).

    Returns:
        JSON string shaped like the positional API ``report`` field on success,
        or ``{"error":"<message>"}`` on failure.
    """
    ...

def hybrid_positional(
    remote_report_json: str,
    local_feature_sets_json: str,
    reorient: bool = True,
    max_connections_per_group: int = 0,
) -> str:
    """Combine a remote positional API report with one or more local feature sets.

    Takes the ``report`` field from a ``POST /api/v3/positional`` response and
    extends it with layout-computed positions for local assemblies.  The result
    has the same shape as a fully-remote positional report.

    The remote report's ``points`` entries must include a ``"group"`` field
    (which the API emits by default for 1:1 oxford/ribbon points).

    Args:
        remote_report_json:        The ``report`` JSON from a positional API response.
        local_feature_sets_json:   JSON array of ``LocalFeatureSet`` objects.
        reorient:                  Auto-orient comparison sequences (default ``True``).
        max_connections_per_group: Cap on M:N connections (``0`` → default 25).

    Returns:
        JSON string shaped like the positional API ``report`` field on success,
        or ``{"error":"<message>"}`` on failure.
    """
    ...

def parse_record_json(raw: str) -> str:
    """Parse the ``records`` array from a raw ``/record`` API response.

    Extracts the ``records`` array from the API envelope and flattens each
    record by merging top-level fields (``recordId``, ``result``) with the
    nested ``record`` object fields. Returns a JSON array string of flat
    record dictionaries.

    Args:
        raw: Raw JSON string from the ``/record`` API endpoint.

    Returns:
        JSON array string where each element is a flat record dictionary with
        all envelope and nested fields merged at the top level.

    Raises:
        ValueError: If the input is not valid JSON.
    """
    ...

def parse_lookup_json(raw: str) -> str:
    """Parse the ``results`` array from a raw ``/lookup`` API response.

    Extracts the ``results`` array from the API envelope and normalises each
    result to a simple candidate dictionary with ``id``, ``name``, ``rank``,
    and ``reason`` fields. Supports field fallbacks for v2 compatibility
    (e.g., ``taxon_id`` → ``id``, ``scientific_name`` → ``name``).

    Args:
        raw: Raw JSON string from the ``/lookup`` API endpoint.

    Returns:
        JSON array string where each element is a candidate dictionary with
        ``id``, ``name``, ``rank``, and ``reason`` fields.

    Raises:
        ValueError: If the input is not valid JSON.
    """
    ...

def parse_phylopic_json(raw: str) -> str:
    """Extract the ``phylopic`` record from a raw ``/phylopic`` API response.

    Returns the ``phylopic`` object as a JSON string, or ``"null"`` when the
    taxon has no silhouette registered in PhyloPic.

    Args:
        raw: Raw JSON string from the ``/phylopic`` API endpoint.

    Returns:
        JSON string of the silhouette record, or ``"null"``.

    Raises:
        ValueError: If the input is not valid JSON.
    """
    ...

def parse_phylopic_batch_json(raw: str) -> str:
    """Flatten the ``results`` map from a raw ``/phylopic/batch`` API response.

    Converts the ``results`` object (keyed by taxon ID) into a JSON array where
    each element is the silhouette record with an added ``taxon_id`` field.
    Taxa that returned no silhouette are omitted.

    Args:
        raw: Raw JSON string from the ``/phylopic/batch`` API endpoint.

    Returns:
        JSON array string where each element is a silhouette record dictionary
        with all response fields plus a ``taxon_id`` key.

    Raises:
        ValueError: If the input is not valid JSON or ``results`` is absent.
    """
    ...

def parse_histogram_json(raw: str) -> str:
    """Extract histogram buckets from a raw ``/report`` JSON response.

    Returns a compact JSON array of bucket objects.  Categorised histograms
    retain their ``by_cat`` entries on each bucket.

    Args:
        raw: Raw JSON string from the ``/report`` API endpoint.

    Returns:
        JSON array string of bucket objects, or ``{"error":"..."}`` on failure.

    Raises:
        ValueError: If the input is not valid JSON or ``report.buckets`` is absent.
    """
    ...

def parse_tree_json(raw: str) -> str:
    """Flatten a tree report's ``treeNodes`` map into a JSON array.

    Each element contains ``taxon_id``, ``scientific_name``, ``taxon_rank``,
    ``count``, ``descendant_count`` (null when absent), ``status``, ``cat``,
    ``children`` (sorted taxon_id array), and ``fields``.

    Args:
        raw: Raw JSON string from the ``/report`` API endpoint (tree report).

    Returns:
        JSON array string, one element per node, or ``{"error":"..."}`` on failure.

    Raises:
        ValueError: If the input is not valid JSON or ``report.treeNodes`` is absent.
    """
    ...

def parse_plot_spec_json(raw: str) -> str:
    """Extract the ``plot_spec`` field from a raw genomehubs ``/report`` API response.

    Returns the ``plot_spec`` object as a JSON string, or ``"null"`` when the
    response contains no plot spec (i.e. ``include_plot_spec`` was not set in
    the request and no ``display`` field was provided).

    Args:
        raw: Raw JSON string from the ``/report`` API endpoint.

    Returns:
        JSON string of the ``plot_spec`` object, or ``"null"`` if absent.
        Returns ``{"error":"..."}`` if the input is not valid JSON.
    """
    ...

def plot_spec_to_vega_lite_json(input: str) -> str:
    """Convert a PlotSpec JSON string (or full ``/report`` response) to a Vega-Lite v5 specification.

    Accepts the full ``/report`` response envelope (extracts ``plot_spec`` automatically)
    or a bare ``PlotSpec`` object.

    Args:
        input: JSON string — either a full ``/report`` API response or a bare ``PlotSpec`` dict.

    Returns:
        Vega-Lite v5 specification as a JSON string.
        Returns ``{"error":"..."}`` on failure.
    """
    ...

def local_plot_spec_json(
    content: str,
    report_type_str: str,
    column_map_json: str,
    display_json: str,
    delimiter_str: str,
) -> str:
    """Build a PlotSpec from local delimited data and return it as JSON.

    Reads TSV/CSV content in-memory — no API call required.

    Args:
        content: Full text of the delimited file.
        report_type_str: One of ``"histogram"``, ``"scatter"``, or ``"bar"``.
        column_map_json: JSON object mapping axis roles to column names, e.g.
            ``'{"x":"genome_size","y":"c_value"}'``.  Pass ``"{}"`` for
            positional defaults.
        display_json: Serialised DisplaySpec; pass ``"{}"`` for defaults.
        delimiter_str: Field separator — ``"\\t"`` for TSV, ``","`` for CSV.
            Pass ``""`` to default to ``"\\t"``.

    Returns:
        Serialised PlotSpec JSON string on success, or ``{"error":"..."}``
        on failure.
    """
    ...

def validate_query_json(
    query_yaml: str,
    field_metadata_json: str,
    validation_config_json: str,
    synonyms_json: str,
) -> str:
    """Validate a query against field metadata and site configuration.

    Accepts YAML for the query and JSON for the field metadata, validation
    config, and synonym map.  Returns a JSON array of error strings — an empty
    array (``"[]"``) means the query is valid.

    Args:
        query_yaml: YAML-serialised ``SearchQuery``.
        field_metadata_json: JSON object mapping field names to metadata
            (same shape as the API ``resultFields`` response).  Pass ``"{}"``
            when no metadata is available.
        validation_config_json: JSON-serialised ``ValidationConfig`` (prefix
            rules, allowed name classes, etc.).  Pass ``"{}"`` for defaults.
        synonyms_json: JSON object mapping synonym names to canonical field
            names.  Pass ``"{}"`` for no synonym expansion.

    Returns:
        JSON array of error strings, e.g. ``"[]"`` for a valid query or
        ``'["unknown field: foo"]'`` for an invalid one.  Returns
        ``'["error: ..."]'`` if parsing fails.
    """
    ...

def validate_report_yaml(report_yaml: str, field_meta_json: str) -> str:
    """Validate a report YAML string against known report type rules.

    Returns a JSON array of error strings — an empty array (``"[]"``) means
    the report configuration is valid.

    Checks that the ``report`` key is present and names a known type, all
    required axis fields are present, numeric ranges are in bounds, and axis
    field names are valid when ``field_meta_json`` is non-empty.

    Args:
        report_yaml: YAML-serialised report configuration, e.g.
            ``"report: histogram\\nx: genome_size\\n"``.
        field_meta_json: JSON object mapping field names to metadata.  Pass
            ``"{}"`` to skip field-name validation.

    Returns:
        JSON array of error strings.
    """
    ...

def query_yaml_from_url_params(url: str) -> tuple[str, str]:
    """Parse a v2 API or UI URL into ``(query_yaml, params_yaml)``.

    Reconstructs a ``SearchQuery`` and ``QueryParams`` from the URL query
    string.  Handles both structured params (``tax_name=``, ``fields=``,
    ``result=``, …) and the composite ``query=`` fragment form.

    Args:
        url: A full v2 API URL, e.g.
            ``"https://goat.genomehubs.org/api/v2/search?tax_name=Primates&fields=genome_size"``
            or a UI URL such as
            ``"https://goat.genomehubs.org/search?tax_name=Primates"``.

    Returns:
        A ``(query_yaml, params_yaml)`` tuple of YAML strings.

    Raises:
        ValueError: On YAML serialisation failure (extremely unlikely).
    """
    ...

def report_yaml_from_url_params(url: str) -> tuple[str, str, str]:
    """Parse a v2 report URL into ``(query_yaml, params_yaml, report_yaml)``.

    Handles both API report URLs (``/api/v2/report?…``) and UI report URLs
    (``/report?…``).

    Args:
        url: A full v2 report URL, e.g.
            ``"https://goat.genomehubs.org/api/v2/report?report=histogram&x=genome_size&result=taxon"``.

    Returns:
        A ``(query_yaml, params_yaml, report_yaml)`` triple of YAML strings.

    Raises:
        ValueError: When the URL does not contain a ``report=`` parameter,
            or on serialisation failure.
    """
    ...
