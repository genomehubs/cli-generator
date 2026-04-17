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
