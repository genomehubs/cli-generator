"""Generic CLI generator for genomehubs instances

This package exposes the core Rust library as a Python module. The compiled
Rust extension is imported here; see ``cli_generator.pyi`` for type signatures
of all exported symbols.
"""

try:
    from .cli_generator import (  # type: ignore[import-not-found]
        build_url,
        describe_query,
        parse_response_status,
        render_snippet,
        version,
    )
except ImportError:
    # Rust extension not yet compiled; type stubs will be used for mypy/pyright
    pass  # type: ignore[unreachable]

from .query import QueryBuilder

__all__ = ["build_url", "describe_query", "parse_response_status", "QueryBuilder", "render_snippet", "version"]
    # Rust extension not yet compiled; type stubs will be used for mypy/pyright
    pass  # type: ignore[unreachable]

from .query import QueryBuilder

__all__ = ["build_url", "describe_query", "parse_response_status", "QueryBuilder", "render_snippet", "version"]
