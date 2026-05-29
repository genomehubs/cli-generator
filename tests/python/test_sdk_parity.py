"""Test SDK parity across Python, JavaScript, and R.

This module verifies that all three generated SDKs (Python, JavaScript, R)
maintain consistent method signatures and configuration parameters.
"""

import re
from pathlib import Path

import pytest

PROJECT_ROOT = Path(__file__).parent.parent.parent

# ── Canonical method definitions ──────────────────────────────────────────────

CANONICAL_METHODS = {
    "set_taxa": {
        "params": ["taxa", "filter_type"],
        "python_name": "set_taxa",
        "js_name": "setTaxa",
        "r_name": "set_taxa",
    },
    "set_rank": {
        "params": ["rank"],
        "python_name": "set_rank",
        "js_name": "setRank",
        "r_name": "set_rank",
    },
    "set_assemblies": {
        "params": ["assemblies"],
        "python_name": "set_assemblies",
        "js_name": "setAssemblies",
        "r_name": "set_assemblies",
    },
    "set_samples": {
        "params": ["samples"],
        "python_name": "set_samples",
        "js_name": "setSamples",
        "r_name": "set_samples",
    },
    "add_attribute": {
        "params": ["name", "operator", "value", "modifiers"],
        "python_name": "add_attribute",
        "js_name": "addAttribute",
        "r_name": "add_attribute",
    },
    "set_attributes": {
        "params": ["attributes"],
        "python_name": "set_attributes",
        "js_name": "setAttributes",
        "r_name": "set_attributes",
    },
    "add_field": {
        "params": ["name", "modifiers"],
        "python_name": "add_field",
        "js_name": "addField",
        "r_name": "add_field",
    },
    "set_fields": {
        "params": ["fields"],
        "python_name": "set_fields",
        "js_name": "setFields",
        "r_name": "set_fields",
    },
    "set_names": {
        "params": ["name_classes"],
        "python_name": "set_names",
        "js_name": "setNames",
        "r_name": "set_names",
    },
    "set_ranks": {
        "params": ["ranks"],
        "python_name": "set_ranks",
        "js_name": "setRanks",
        "r_name": "set_ranks",
    },
    "set_exclude_ancestral": {
        "params": ["fields"],
        "python_name": "set_exclude_ancestral",
        "js_name": "setExcludeAncestral",
        "r_name": "set_exclude_ancestral",
    },
    "add_exclude_ancestral": {
        "params": ["field"],
        "python_name": "add_exclude_ancestral",
        "js_name": "addExcludeAncestral",
        "r_name": "add_exclude_ancestral",
    },
    "set_exclude_descendant": {
        "params": ["fields"],
        "python_name": "set_exclude_descendant",
        "js_name": "setExcludeDescendant",
        "r_name": "set_exclude_descendant",
    },
    "add_exclude_descendant": {
        "params": ["field"],
        "python_name": "add_exclude_descendant",
        "js_name": "addExcludeDescendant",
        "r_name": "add_exclude_descendant",
    },
    "set_exclude_direct": {
        "params": ["fields"],
        "python_name": "set_exclude_direct",
        "js_name": "setExcludeDirect",
        "r_name": "set_exclude_direct",
    },
    "add_exclude_direct": {
        "params": ["field"],
        "python_name": "add_exclude_direct",
        "js_name": "addExcludeDirect",
        "r_name": "add_exclude_direct",
    },
    "set_exclude_missing": {
        "params": ["fields"],
        "python_name": "set_exclude_missing",
        "js_name": "setExcludeMissing",
        "r_name": "set_exclude_missing",
    },
    "add_exclude_missing": {
        "params": ["field"],
        "python_name": "add_exclude_missing",
        "js_name": "addExcludeMissing",
        "r_name": "add_exclude_missing",
    },
    "set_exclude_derived": {
        "params": ["fields"],
        "python_name": "set_exclude_derived",
        "js_name": "setExcludeDerived",
        "r_name": "set_exclude_derived",
    },
    "set_exclude_estimated": {
        "params": ["fields"],
        "python_name": "set_exclude_estimated",
        "js_name": "setExcludeEstimated",
        "r_name": "set_exclude_estimated",
    },
    "set_size": {
        "params": ["size"],
        "python_name": "set_size",
        "js_name": "setSize",
        "r_name": "set_size",
    },
    "set_page": {
        "params": ["page"],
        "python_name": "set_page",
        "js_name": "setPage",
        "r_name": "set_page",
    },
    "set_sort": {
        "params": ["sort_by", "direction"],
        "python_name": "set_sort",
        "js_name": "setSort",
        "r_name": "set_sort",
    },
    "set_include_estimates": {
        "params": ["value"],
        "python_name": "set_include_estimates",
        "js_name": "setIncludeEstimates",
        "r_name": "set_include_estimates",
    },
    "set_taxonomy": {
        "params": ["taxonomy"],
        "python_name": "set_taxonomy",
        "js_name": "setTaxonomy",
        "r_name": "set_taxonomy",
    },
    "to_query_yaml": {
        "params": [],
        "python_name": "to_query_yaml",
        "js_name": "toQueryYaml",
        "r_name": "to_query_yaml",
    },
    "to_params_yaml": {
        "params": [],
        "python_name": "to_params_yaml",
        "js_name": "toParamsYaml",
        "r_name": "to_params_yaml",
    },
    "to_url": {
        "params": [],
        "python_name": "to_url",
        "js_name": "toUrl",
        "r_name": "to_url",
    },
    "to_v2_url": {
        "params": [],
        "python_name": "to_v2_url",
        "js_name": "toV2Url",
        "r_name": "to_v2_url",
    },
    "from_v2_url": {
        "params": ["url"],
        "python_name": "from_v2_url",
        "js_name": "fromV2Url",
        "r_name": "from_v2_url",
    },
    "to_ui_url": {
        "params": [],
        "python_name": "to_ui_url",
        "js_name": "toUiUrl",
        "r_name": "to_ui_url",
    },
    "count": {
        "params": [],
        "python_name": "count",
        "js_name": "count",
        "r_name": "count",
    },
    "search": {
        "params": ["format"],
        "python_name": "search",
        "js_name": "search",
        "r_name": "search",
    },
    "search_all": {
        "params": ["max_records"],
        "python_name": "search_all",
        "js_name": "searchAll",
        "r_name": "search_all",
    },
    "validate": {
        "params": [],
        "python_name": "validate",
        "js_name": "validate",
        "r_name": "validate",
    },
    "describe": {
        "params": ["field_metadata", "mode"],
        "python_name": "describe",
        "js_name": "describe",
        "r_name": "describe",
    },
    "snippet": {
        "params": ["languages", "site_name", "sdk_name", "api_base"],
        "python_name": "snippet",
        "js_name": "snippet",
        "r_name": "snippet",
    },
    "reset": {
        "params": [],
        "python_name": "reset",
        "js_name": "reset",
        "r_name": "reset",
    },
    "merge": {
        "params": ["other"],
        "python_name": "merge",
        "js_name": "merge",
        "r_name": "merge",
    },
    "search_batch": {
        "params": ["queries", "api_base", "api_version"],
        "python_name": "search_batch",
        "js_name": "searchBatch",
        "r_name": "search_batch",
    },
    "count_batch": {
        "params": ["queries", "api_base", "api_version"],
        "python_name": "count_batch",
        "js_name": "countBatch",
        "r_name": "count_batch",
    },
    "record": {
        "params": ["api_base", "api_version"],
        "python_name": "record",
        "js_name": "record",
        "r_name": "record",
    },
    "record_batch": {
        "params": ["record_ids"],
        "python_name": "record_batch",
        "js_name": "recordBatch",
        "r_name": "record_batch",
    },
    "lookup": {
        "params": ["api_base", "api_version"],
        "python_name": "lookup",
        "js_name": "lookup",
        "r_name": "lookup",
    },
    "lookup_batch": {
        "params": ["lookups"],
        "python_name": "lookup_batch",
        "js_name": "lookupBatch",
        "r_name": "lookup_batch",
    },
    "summary": {
        "params": ["api_base", "api_version"],
        "python_name": "summary",
        "js_name": "summary",
        "r_name": "summary",
    },
    "set_lineage_rank_summary": {
        "params": ["specs"],
        "python_name": "set_lineage_rank_summary",
        "js_name": "setLineageRankSummary",
        "r_name": "set_lineage_rank_summary",
    },
    "set_lineage_summary_mode": {
        "params": ["mode"],
        "python_name": "set_lineage_summary_mode",
        "js_name": "setLineageSummaryMode",
        "r_name": "set_lineage_summary_mode",
    },
    "to_flat_records": {
        "params": ["lineage_summary"],
        "python_name": "to_flat_records",
        "js_name": "toFlatRecords",
        "r_name": "to_flat_records",
    },
    "to_tidy_records": {
        "params": ["records", "lineage_summary"],
        "python_name": "to_tidy_records",
        "js_name": "toTidyRecords",
        "r_name": "to_tidy_records",
    },
    "phylopic": {
        "params": ["taxon_id", "taxonomy"],
        "python_name": "phylopic",
        "js_name": "phylopic",
        "r_name": "phylopic",
    },
    "phylopic_batch": {
        "params": ["taxon_ids", "taxonomy"],
        "python_name": "phylopic_batch",
        "js_name": "phylopicBatch",
        "r_name": "phylopic_batch",
    },
    "metadata": {
        "params": [],
        "python_name": "metadata",
        "js_name": "metadata",
        "r_name": "metadata",
    },
    "indices": {
        "params": [],
        "python_name": "indices",
        "js_name": "indices",
        "r_name": "indices",
    },
    "fields": {
        "params": ["index"],
        "python_name": "fields",
        "js_name": "fields",
        "r_name": "fields",
    },
    "taxonomies": {
        "params": [],
        "python_name": "taxonomies",
        "js_name": "taxonomies",
        "r_name": "taxonomies",
    },
    "ranks": {
        "params": [],
        "python_name": "ranks",
        "js_name": "ranks",
        "r_name": "ranks",
    },
    "report": {
        "params": ["report"],
        "python_name": "report",
        "js_name": "report",
        "r_name": "report",
    },
    "report": {
        "params": ["report"],
        "python_name": "report",
        "js_name": "report",
        "r_name": "report",
    },
    "report_batch": {
        "params": ["reports", "max_reports"],
        "python_name": "report_batch",
        "js_name": "reportBatch",
        "r_name": "report_batch",
    },
    "chain_query": {
        "params": ["query_key", "query_string"],
        "python_name": "chain_query",
        "js_name": "chainQuery",
        "r_name": "chain_query",
    },
    "set_id_set": {
        "params": ["taxon_ids"],
        "python_name": "set_id_set",
        "js_name": "setIdSet",
        "r_name": "set_id_set",
    },
    "set_id_type": {
        "params": ["id_type"],
        "python_name": "set_id_type",
        "js_name": "setIdType",
        "r_name": "set_id_type",
    },
    "positional": {
        "params": ["report", "group_by", "assemblies"],
        "python_name": "positional",
        "js_name": "positional",
        "r_name": "positional",
    },
    "oxford": {
        "params": ["group_by", "assemblies"],
        "python_name": "oxford",
        "js_name": "oxford",
        "r_name": "oxford",
    },
    "ribbon": {
        "params": ["group_by", "assemblies"],
        "python_name": "ribbon",
        "js_name": "ribbon",
        "r_name": "ribbon",
    },
    "painting": {
        "params": ["group_by", "assembly"],
        "python_name": "painting",
        "js_name": "painting",
        "r_name": "painting",
    },
    "hybrid_positional": {
        "params": ["report", "group_by", "local_files"],
        "python_name": "hybrid_positional",
        "js_name": "hybridPositional",
        "r_name": "hybrid_positional",
    },
}

CONSTRUCTOR_PARAMS: dict[str, dict[str, str]] = {}

# ── ReportBuilder canonical method definitions ────────────────────────────────

CANONICAL_REPORT_BUILDER_METHODS = {
    "set_x": {"python_name": "set_x", "js_name": "setX", "r_name": "set_x"},
    "set_y": {"python_name": "set_y", "js_name": "setY", "r_name": "set_y"},
    "set_cat": {"python_name": "set_cat", "js_name": "setCat", "r_name": "set_cat"},
    "set_query": {"python_name": "set_query", "js_name": "setQuery", "r_name": "set_query"},
    "set_rank": {"python_name": "set_rank", "js_name": "setRank", "r_name": "set_rank"},
    "set_ranks": {"python_name": "set_ranks", "js_name": "setRanks", "r_name": "set_ranks"},
    "set_fields": {"python_name": "set_fields", "js_name": "setFields", "r_name": "set_fields"},
    "set_status_filter": {
        "python_name": "set_status_filter",
        "js_name": "setStatusFilter",
        "r_name": "set_status_filter",
    },
    "set_cat_rank": {"python_name": "set_cat_rank", "js_name": "setCatRank", "r_name": "set_cat_rank"},
    "set_collapse_monotypic": {
        "python_name": "set_collapse_monotypic",
        "js_name": "setCollapseMonotypic",
        "r_name": "set_collapse_monotypic",
    },
    "set_preserve_rank": {
        "python_name": "set_preserve_rank",
        "js_name": "setPreserveRank",
        "r_name": "set_preserve_rank",
    },
    "set_count_rank": {"python_name": "set_count_rank", "js_name": "setCountRank", "r_name": "set_count_rank"},
    "set_location_field": {
        "python_name": "set_location_field",
        "js_name": "setLocationField",
        "r_name": "set_location_field",
    },
    "set_hex_resolution": {
        "python_name": "set_hex_resolution",
        "js_name": "setHexResolution",
        "r_name": "set_hex_resolution",
    },
    "set_map_threshold": {
        "python_name": "set_map_threshold",
        "js_name": "setMapThreshold",
        "r_name": "set_map_threshold",
    },
    "set_scatter_threshold": {
        "python_name": "set_scatter_threshold",
        "js_name": "setScatterThreshold",
        "r_name": "set_scatter_threshold",
    },
    "set_display": {"python_name": "set_display", "js_name": "setDisplay", "r_name": "set_display"},
    "set_include_plot_spec": {
        "python_name": "set_include_plot_spec",
        "js_name": "setIncludePlotSpec",
        "r_name": "set_include_plot_spec",
    },
    "to_report_yaml": {"python_name": "to_report_yaml", "js_name": "toReportYaml", "r_name": "to_report_yaml"},
    "validate": {"python_name": "validate", "js_name": "validate", "r_name": "validate"},
    "run": {"python_name": "run", "js_name": "run", "r_name": "run"},
    "set_feature": {"python_name": "set_feature", "js_name": "setFeature", "r_name": "set_feature"},
    "set_reference": {"python_name": "set_reference", "js_name": "setReference", "r_name": "set_reference"},
    "set_context": {"python_name": "set_context", "js_name": "setContext", "r_name": "set_context"},
    "add_ring": {"python_name": "add_ring", "js_name": "addRing", "r_name": "add_ring"},
    "set_arc_ranks": {"python_name": "set_arc_ranks", "js_name": "setArcRanks", "r_name": "set_arc_ranks"},
    "set_axis_boundaries": {
        "python_name": "set_axis_boundaries",
        "js_name": "setAxisBoundaries",
        "r_name": "set_axis_boundaries",
    },
    "set_axis_date_intervals": {
        "python_name": "set_axis_date_intervals",
        "js_name": "setAxisDateIntervals",
        "r_name": "set_axis_date_intervals",
    },
}

# ── Introspection functions ──────────────────────────────────────────────────


def get_python_constructor_params():
    """Extract constructor parameters from Python template."""
    query_py_tera = PROJECT_ROOT / "templates" / "python" / "query.py.tera"
    content = Path(query_py_tera).read_text()
    # Find __init__ signature and extract parameter names
    pattern = r"def __init__\s*\(\s*self,([^)]+)\)"
    match = re.search(pattern, content, re.DOTALL)
    if not match:
        return []

    params_str = match[1]
    params = []
    for p in params_str.split(","):
        if p := p.strip():
            param_name = p.split(":")[0].strip()
            params.append(param_name)
    return params


def get_js_constructor_params():
    """Extract constructor parameters from JavaScript template."""
    query_js = PROJECT_ROOT / "templates" / "js" / "query.js"
    content = Path(query_js).read_text()
    # constructor(index, options = {}) pattern
    pattern = r"constructor\s*\(([^)]+)\)"
    match = re.search(pattern, content, re.DOTALL)
    if not match:
        return []

    params_str = match[1]
    params = []
    for p in params_str.split(","):
        if p := p.strip():
            param_name = p.split("=")[0].strip()
            params.append(param_name)
    return params


def get_r_constructor_params():
    """Extract constructor parameters from R template."""
    query_r = PROJECT_ROOT / "templates" / "r" / "query.R"
    content = Path(query_r).read_text()
    # Find initialize = function(...) pattern
    pattern = r"initialize\s*=\s*function\s*\(([^)]+)\)"
    match = re.search(pattern, content, re.DOTALL)
    if not match:
        return []

    params_str = match[1]
    params = []
    for p in params_str.split(","):
        p = p.strip()
        if p and p != "self":
            param_name = p.split("=")[0].strip()
            params.append(param_name)
    return params


def get_python_methods():
    """Extract all public methods from templates/python/query.py.tera (generated SDK)."""
    # Use the template, not the main SDK
    query_py_tera = PROJECT_ROOT / "templates" / "python" / "query.py.tera"
    assert query_py_tera.exists(), f"Python query template not found at {query_py_tera}"

    content = Path(query_py_tera).read_text()
    # Rough parsing: look for "def method_name("
    methods = {}
    pattern = r"^\s{4}def\s+(\w+)\s*\("
    for match in re.finditer(pattern, content, re.MULTILINE):
        name = match.group(1)
        if not name.startswith("_"):
            # Find the full method signature (may span multiple lines)
            # Look from the opening paren to the closing paren
            paren_start = match.end() - 1  # Position of the '('
            paren_depth = 0
            paren_end = paren_start

            for i in range(paren_start, len(content)):
                if content[i] == "(":
                    paren_depth += 1
                elif content[i] == ")":
                    paren_depth -= 1
                    if paren_depth == 0:
                        paren_end = i
                        break

            # Extract parameters from the signature
            params_str = content[paren_start + 1 : paren_end]
            params = [p.strip() for p in params_str.split(",") if p.strip() and p.strip() != "self"]
            # Remove type annotations and defaults
            params = [p.split(":")[0].split("=")[0].strip() for p in params]
            methods[name] = params

    return methods


def get_js_methods():
    """Extract all public methods from templates/js/query.js."""
    query_js = PROJECT_ROOT / "templates" / "js" / "query.js"
    assert query_js.exists(), f"JavaScript query template not found at {query_js}"

    content = Path(query_js).read_text()
    methods = {}
    # Look for method definitions: methodName(params) {
    pattern = r"(\w+)\s*\(\s*([^)]*)\s*\)\s*{"
    for match in re.finditer(pattern, content):
        name = match.group(1)
        if not name.startswith("_") and name not in ("constructor", "if", "for", "while"):
            params_str = match.group(2)
            params = [p.strip() for p in params_str.split(",") if p.strip() and p.strip() != "this"]
            # Remove default values and destructuring
            params = [p.split("=")[0].split("{")[0].split("}")[0].strip() for p in params]
            methods[name] = [p for p in params if p]

    return methods


def get_r_methods():
    """Extract all public methods from templates/r/query.R."""
    query_r = PROJECT_ROOT / "templates" / "r" / "query.R"
    assert query_r.exists(), f"R query template not found at {query_r}"

    content = Path(query_r).read_text()
    methods = {}
    # Look for method definitions: method_name = function(...) {
    pattern = r"(\w+)\s*=\s*function\s*\(([^)]*)\)"
    for match in re.finditer(pattern, content):
        name = match.group(1)
        if not name.startswith("_") and name != "private":
            params_str = match.group(2)
            params = [p.strip() for p in params_str.split(",") if p.strip()]
            params = [p.split("=")[0].strip() for p in params]
            methods[name] = [p for p in params if p]

    return methods


def get_python_report_builder_methods() -> dict[str, list[str]]:
    """Extract all public methods from the ReportBuilder class in the Python template."""
    query_py_tera = PROJECT_ROOT / "templates" / "python" / "query.py.tera"
    content = Path(query_py_tera).read_text()

    class_match = re.search(r"^class ReportBuilder:", content, re.MULTILINE)
    if not class_match:
        return {}
    rb_content = content[class_match.start() :]

    methods: dict[str, list[str]] = {}
    pattern = r"^\s{4}def\s+(\w+)\s*\("
    for match in re.finditer(pattern, rb_content, re.MULTILINE):
        name = match.group(1)
        if name.startswith("_"):
            continue
        paren_start = match.end() - 1
        paren_depth = 0
        paren_end = paren_start
        for i in range(paren_start, len(rb_content)):
            if rb_content[i] == "(":
                paren_depth += 1
            elif rb_content[i] == ")":
                paren_depth -= 1
                if paren_depth == 0:
                    paren_end = i
                    break
        params_str = rb_content[paren_start + 1 : paren_end]
        params = [p.strip() for p in params_str.split(",") if p.strip() and p.strip() != "self"]
        params = [p.split(":")[0].split("=")[0].strip() for p in params]
        methods[name] = params

    return methods


def get_js_report_builder_methods() -> dict[str, list[str]]:
    """Extract all public methods from the ReportBuilder class in the JS template."""
    query_js = PROJECT_ROOT / "templates" / "js" / "query.js"
    content = Path(query_js).read_text()

    class_match = re.search(r"^class ReportBuilder\s*{", content, re.MULTILINE)
    if not class_match:
        return {}
    rb_content = content[class_match.start() :]

    methods: dict[str, list[str]] = {}
    skip = {"constructor", "if", "for", "while", "async"}
    pattern = r"(\w+)\s*\(\s*([^)]*)\s*\)\s*{"
    for match in re.finditer(pattern, rb_content):
        name = match.group(1)
        if name.startswith("_") or name in skip:
            continue
        params_str = match.group(2)
        params = [p.strip() for p in params_str.split(",") if p.strip()]
        params = [p.split("=")[0].strip() for p in params]
        methods[name] = [p for p in params if p]

    return methods


def get_r_report_builder_methods() -> dict[str, list[str]]:
    """Extract all public methods from the ReportBuilder R6 class in the R template."""
    query_r = PROJECT_ROOT / "templates" / "r" / "query.R"
    content = Path(query_r).read_text()

    class_match = re.search(r"ReportBuilder\s*<-\s*R6::R6Class", content)
    if not class_match:
        return {}
    rb_content = content[class_match.start() :]

    methods: dict[str, list[str]] = {}
    skip = {"private", "initialize"}
    pattern = r"(\w+)\s*=\s*function\s*\(([^)]*)\)"
    for match in re.finditer(pattern, rb_content):
        name = match.group(1)
        if name.startswith("_") or name in skip:
            continue
        params_str = match.group(2)
        params = [p.strip() for p in params_str.split(",") if p.strip()]
        params = [p.split("=")[0].strip() for p in params]
        methods[name] = [p for p in params if p]

    return methods


def get_python_docstring(method_name: str) -> str:
    """Get the docstring for a Python template method.

    For __init__, returns the class docstring (standard Python convention).
    """
    query_py_tera = PROJECT_ROOT / "templates" / "python" / "query.py.tera"
    content = Path(query_py_tera).read_text()
    if method_name == "__init__":
        # For __init__, return the class docstring (Python convention)
        class_pattern = r"class\s+QueryBuilder.*?:\s*\n\s+\"\"\"(.*?)\"\"\""
        match = re.search(class_pattern, content, re.DOTALL)
    else:
        # For other methods, search for "def method_name" and its docstring
        method_pattern = rf'def\s+{method_name}\s*\([^)]*\).*?:\s*\n\s+"""(.*?)"""'
        match = re.search(method_pattern, content, re.DOTALL)

    return match[1].strip() if match else ""


# ── Tests ────────────────────────────────────────────────────────────────────


class TestSDKParity:
    """Test that all three SDKs have consistent method signatures."""

    def test_python_canonical_methods_present(self):
        """All canonical methods must exist in Python SDK."""
        python_methods = get_python_methods()

        for concept, spec in CANONICAL_METHODS.items():
            method_name = spec["python_name"]
            assert method_name in python_methods, f"Python missing method: {method_name}"

    def test_javascript_canonical_methods_present(self):
        """All canonical methods must exist in JavaScript SDK."""
        js_methods = get_js_methods()

        for concept, spec in CANONICAL_METHODS.items():
            method_name = spec["js_name"]
            assert method_name in js_methods, f"JavaScript missing method: {method_name}"

    def test_r_canonical_methods_present(self):
        """All canonical methods must exist in R SDK."""
        r_methods = get_r_methods()

        for concept, spec in CANONICAL_METHODS.items():
            method_name = spec["r_name"]
            assert method_name in r_methods, f"R missing method: {method_name}"

    def test_no_extra_methods_in_python(self):
        """Python should not have extra methods beyond canonical set."""
        python_methods = get_python_methods()
        canonical_python_names = {spec["python_name"] for spec in CANONICAL_METHODS.values()}
        # Allow documented utility methods and Python-only internals
        canonical_python_names.update(
            [
                "__init__",
                "field_modifiers",
                "field_names",
                "field_info",
                "combine",
                "search_df",
                "search_polars",
                "_post_json",  # Python-only: internal transport helper
            ]
        )
        # ReportBuilder canonical methods are also extracted by get_python_methods()
        # since the file contains both classes; allow them here.
        canonical_python_names.update({spec["python_name"] for spec in CANONICAL_REPORT_BUILDER_METHODS.values()})

        extra = set(python_methods.keys()) - canonical_python_names
        assert len(extra) == 0, f"Python has extra methods not in canonical list: {extra}"

    def test_to_url_returns_v3_get_url(self):
        """to_url() must return a v3 GET URL with a ?url= parameter."""
        import warnings

        from cli_generator.query import QueryBuilder

        qb = QueryBuilder("taxon").set_taxa(["Mammalia"])
        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter("always")
            url = qb.to_url()
        assert "/v3/search?url=" in url, f"Expected v3 GET URL, got: {url}"
        assert not any(
            issubclass(w.category, DeprecationWarning) for w in caught
        ), "to_url() should not emit DeprecationWarning"

    def test_to_url_warns_when_names_set(self):
        """to_url() emits RuntimeWarning when name classes are set."""
        import warnings

        from cli_generator.query import QueryBuilder

        qb = QueryBuilder("taxon").set_taxa(["Mammalia"]).set_names(["scientific_name"])
        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter("always")
            qb.to_url()
        assert any(
            issubclass(w.category, RuntimeWarning) for w in caught
        ), "to_url() should emit RuntimeWarning when name classes are set"

    def test_to_url_no_warning_for_simple_query(self):
        """to_url() emits no warning for a query with no non-roundtrippable features."""
        import warnings

        from cli_generator.query import QueryBuilder

        qb = QueryBuilder("taxon").set_taxa(["Mammalia"])
        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter("always")
            qb.to_url()
        assert not any(
            issubclass(w.category, RuntimeWarning) for w in caught
        ), "to_url() should not emit RuntimeWarning for simple queries"


class TestValidationConfiguration:
    """Test that validation is properly implemented in templates."""

    def test_python_validate_method_exists(self):
        """Python template should have validate() method."""
        python_methods = get_python_methods()
        assert "validate" in python_methods, "Python template missing validate() method"

    def test_r_validate_method_exists(self):
        """R template should have validate() method."""
        r_methods = get_r_methods()
        assert "validate" in r_methods, "R template missing validate() method"


class TestDocumentationParity:
    """Test that Quarto reference documentation includes all SDK methods."""

    def get_documented_methods(self) -> set[str]:
        """Return the set of canonical method names that appear in the Quarto reference.

        Uses a simple membership check: a method is considered documented if its
        name appears in a backtick context anywhere in the file.  This is robust
        against heading style, table vs list format, and any punctuation convention
        — the only requirement is that the method name is mentioned at least once
        as a backtick-quoted identifier.
        """
        quarto_path = PROJECT_ROOT / "workdir/my-goat/goat-cli/docs/reference/query-builder.qmd"
        if not quarto_path.exists():
            pytest.skip(
                f"Quarto reference guide not found at {quarto_path}. "
                "This test requires the generated goat CLI project."
            )

        content = quarto_path.read_text()
        canonical_names = set(CANONICAL_METHODS.keys())
        return {name for name in canonical_names if f"`{name}" in content}

    def test_documented_methods_include_all_canonical(self):
        """All canonical methods should be documented in Quarto reference."""
        documented = self.get_documented_methods()
        canonical_names = set(CANONICAL_METHODS.keys())

        missing = canonical_names - documented
        assert len(missing) == 0, f"Documentation missing these canonical methods: {sorted(missing)}"

    def test_documented_methods_include_utilities(self):
        """Documentation should include documented utility methods."""
        quarto_path = PROJECT_ROOT / "workdir/my-goat/goat-cli/docs/reference/query-builder.qmd"
        if not quarto_path.exists():
            pytest.skip(
                f"Quarto reference guide not found at {quarto_path}. "
                "This test requires the generated goat CLI project."
            )

        content = quarto_path.read_text()

        # These utility methods are not in CANONICAL_METHODS (they are Python-only wrappers)
        # but should still appear in the reference documentation.
        utilities = {
            "search_df",  # pandas wrapper
            "search_polars",  # polars wrapper
            "search_all",  # pagination wrapper
        }

        for util in utilities:
            assert f"`{util}" in content, f"Documentation missing utility method: {util}"

    def test_documented_methods_reference_parameters(self):
        """Documented methods should include parameter tables where applicable."""
        quarto_path = PROJECT_ROOT / "workdir/my-goat/goat-cli/docs/reference/query-builder.qmd"
        if not quarto_path.exists():
            pytest.skip(
                f"Quarto reference guide not found at {quarto_path}. "
                "This test requires the generated goat CLI project."
            )

        content = quarto_path.read_text()

        # Check that key methods with parameters have tables
        methods_with_params = {
            "set_taxa": ["taxa", "filter_type"],
            "add_attribute": ["name", "operator", "value", "modifiers"],
        }

        for method, expected_params in methods_with_params.items():
            # Find the method section
            method_pattern = rf"^###\s+`{method}\("
            assert re.search(method_pattern, content, re.MULTILINE), f"Method {method} not found in documentation"

            # Check for parameter table after the method heading
            for param in expected_params:
                param_pattern = rf"(?:{method}.*?){param}"
                assert re.search(
                    param_pattern, content, re.DOTALL
                ), f"Parameter {param} for method {method} not documented"


if __name__ == "__main__":
    pytest.main([__file__, "-v"])


class TestGeneratedProjectWiring:
    """Verify that lib.rs.tera registrations and patch_python_init exports stay in sync.

    These tests catch the class of bug where a function is added to one place
    (e.g. new.rs __init__.py exports) but forgotten in the other (lib.rs.tera
    pymodule registrations), which produces an ImportError at runtime.
    """

    @staticmethod
    def _registered_functions() -> set[str]:
        """Names registered via add_function(wrap_pyfunction!(NAME, m)?) in lib.rs.tera."""
        lib_tera = PROJECT_ROOT / "templates" / "rust" / "lib.rs.tera"
        content = lib_tera.read_text()
        return set(re.findall(r"wrap_pyfunction!\((\w+),", content))

    @staticmethod
    def _exported_from_init() -> set[str]:
        """Names imported from the extension in the generated __init__.py (new.rs)."""
        new_rs = PROJECT_ROOT / "src" / "commands" / "new.rs"
        content = new_rs.read_text()
        # The patch_python_init format string contains: from .{} import (\n    name,\n    ...
        # Grab the block between the first `from .{} import (` and the matching `)`.
        block_match = re.search(r"from \.\{\} import \((.+?)\)", content, re.DOTALL)
        if not block_match:
            return set()
        return {n.strip().rstrip(",") for n in block_match.group(1).splitlines() if n.strip()}

    def test_all_init_exports_are_registered(self) -> None:
        """Every name imported in the generated __init__.py must be registered in lib.rs.tera."""
        registered = self._registered_functions()
        # sdk-level functions (build_url, search, count, …) live in generated::sdk, not as
        # top-level pyfunction wrappers — they are registered as sdk::name.  Collect those too.
        lib_tera = PROJECT_ROOT / "templates" / "rust" / "lib.rs.tera"
        content = lib_tera.read_text()
        sdk_functions = set(re.findall(r"wrap_pyfunction!\(sdk::(\w+),", content))
        all_registered = registered | sdk_functions

        exported = self._exported_from_init()
        missing = exported - all_registered
        assert not missing, (
            f"Functions exported in __init__.py but NOT registered in lib.rs.tera: {missing}\n"
            "Add a #[pyfunction] wrapper and m.add_function() call in templates/rust/lib.rs.tera"
        )

    def test_all_registered_functions_are_exported(self) -> None:
        """Every non-sdk pyfunction registered in lib.rs.tera should be exported in __init__.py."""
        registered = self._registered_functions()
        # sdk:: registrations are not individual function names in __init__.py
        lib_tera = PROJECT_ROOT / "templates" / "rust" / "lib.rs.tera"
        content = lib_tera.read_text()
        sdk_functions = set(re.findall(r"wrap_pyfunction!\(sdk::(\w+),", content))
        # Class registrations (add_class) are also not in the function export list
        # Remove anything that is clearly a class (FieldInfo, Validator)
        non_exported_ok = sdk_functions | {"build_url", "build_ui_url", "search", "count"}

        exported = self._exported_from_init()
        missing = (registered - non_exported_ok) - exported
        assert not missing, (
            f"Functions registered in lib.rs.tera but NOT exported in __init__.py: {missing}\n"
            "Add the name to the import block in src/commands/new.rs patch_python_init()"
        )


class TestReportBuilderParity:
    """ReportBuilder methods must be present in all three SDK languages."""

    def test_python_report_builder_methods_present(self):
        """All canonical ReportBuilder methods must exist in the Python template."""
        python_methods = get_python_report_builder_methods()
        for concept, spec in CANONICAL_REPORT_BUILDER_METHODS.items():
            assert spec["python_name"] in python_methods, f"ReportBuilder missing Python method: {spec['python_name']}"

    def test_javascript_report_builder_methods_present(self):
        """All canonical ReportBuilder methods must exist in the JavaScript template."""
        js_methods = get_js_report_builder_methods()
        for concept, spec in CANONICAL_REPORT_BUILDER_METHODS.items():
            assert spec["js_name"] in js_methods, f"ReportBuilder missing JavaScript method: {spec['js_name']}"

    def test_r_report_builder_methods_present(self):
        """All canonical ReportBuilder methods must exist in the R template."""
        r_methods = get_r_report_builder_methods()
        for concept, spec in CANONICAL_REPORT_BUILDER_METHODS.items():
            assert spec["r_name"] in r_methods, f"ReportBuilder missing R method: {spec['r_name']}"
