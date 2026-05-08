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
        parse_histogram_json,
        parse_lookup_json,
        parse_paginated_json,
        parse_record_json,
        parse_response_status,
        parse_search_json,
        parse_tree_json,
        query_yaml_from_url_params,
        render_snippet,
        report_yaml_from_url_params,
        split_source_columns,
        to_tidy_records,
        validate_query_json,
        validate_report_yaml,
        values_only,
        version,
    )

from .multi_query_builder import MultiQueryBuilder, from_file
from .query import QueryBuilder, ReportBuilder, probe_api_capability

__all__ = [
    "annotate_source_labels",
    "annotated_values",
    "build_ui_url",
    "build_url",
    "describe_query",
    "from_file",
    "MultiQueryBuilder",
    "parse_batch_json",
    "parse_histogram_json",
    "parse_lookup_json",
    "parse_paginated_json",
    "parse_record_json",
    "parse_response_status",
    "parse_search_json",
    "parse_tree_json",
    "QueryBuilder",
    "ReportBuilder",
    "probe_api_capability",
    "query_yaml_from_url_params",
    "render_snippet",
    "report_yaml_from_url_params",
    "split_source_columns",
    "to_tidy_records",
    "validate_query_json",
    "validate_report_yaml",
    "values_only",
    "version",
]
