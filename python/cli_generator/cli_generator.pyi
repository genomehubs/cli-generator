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
