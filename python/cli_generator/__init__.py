"""Generic CLI generator for genomehubs instances

This package exposes the core Rust library as a Python module. The compiled
Rust extension is imported here; see ``cli_generator.pyi`` for type signatures
of all exported symbols.
"""

import contextlib

with contextlib.suppress(ImportError):
    from .cli_generator import (  # type: ignore[import-not-found]
        annotate_source_labels,
        annotated_values,
        build_ui_url,
        build_url,
        describe_query,
        parse_batch_json,
        parse_paginated_json,
        parse_response_status,
        parse_search_json,
        render_snippet,
        split_source_columns,
        to_tidy_records,
        validate_query_json,
        values_only,
        version,
    )

from .multi_query_builder import MultiQueryBuilder, from_file
from .query import QueryBuilder

__all__ = [
    "annotate_source_labels",
    "annotated_values",
    "build_ui_url",
    "build_url",
    "describe_query",
    "from_file",
    "MultiQueryBuilder",
    "parse_batch_json",
    "parse_paginated_json",
    "parse_response_status",
    "parse_search_json",
    "QueryBuilder",
    "render_snippet",
    "split_source_columns",
    "to_tidy_records",
    "validate_query_json",
    "values_only",
    "version",
]
