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
