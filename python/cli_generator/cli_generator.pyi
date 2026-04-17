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
