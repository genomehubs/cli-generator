"""Generic CLI generator for genomehubs instances

This package exposes the core Rust library as a Python module. The compiled
Rust extension is imported here; see ``cli_generator.pyi`` for type signatures
of all exported symbols.
"""

try:
    from .cli_generator import (  # type: ignore[import-not-found]
        annotate_source_labels,
        annotated_values,
        build_url,
        describe_query,
        parse_paginated_json,
        parse_response_status,
        parse_search_json,
        render_snippet,
        split_source_columns,
        to_tidy_records,
        values_only,
        version,
    )
except ImportError:
    # Rust extension not yet compiled; type stubs will be used for mypy/pyright
    pass  # type: ignore[unreachable]

from .query import QueryBuilder

__all__ = [
    "annotate_source_labels",
    "annotated_values",
    "build_url",
    "describe_query",
    "parse_paginated_json",
    "parse_response_status",
    "parse_search_json",
    "QueryBuilder",
    "render_snippet",
    "split_source_columns",
    "to_tidy_records",
    "values_only",
    "version",
]
