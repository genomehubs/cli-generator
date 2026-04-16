"""Generic CLI generator for genomehubs instances

This package exposes the core Rust library as a Python module. The compiled
Rust extension is imported here; see ``cli_generator.pyi`` for type signatures
of all exported symbols.
"""

try:
    from .cli_generator import build_url, describe_query, render_snippet, version  # type: ignore[import-not-found]
except ImportError:
    # Rust extension not yet compiled; type stubs will be used for mypy/pyright
    pass  # type: ignore[unreachable]

from .query import QueryBuilder

__all__ = ["build_url", "describe_query", "QueryBuilder", "render_snippet", "version"]
