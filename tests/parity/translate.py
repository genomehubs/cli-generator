"""Translate V2 GET request parameters to V3 JSON POST body format.

Maps V2 query string parameters to V3 JSON structure and handles type conversions.
"""

from typing import Any, Dict, Optional
from urllib.parse import parse_qs, urlparse


def translate_v2_url_to_v3_body(v2_url: str) -> Dict[str, Any]:
    """Convert a V2 GET URL into a V3 JSON POST body.

    Args:
        v2_url: Full V2 API URL, e.g.
            "https://goat.genomehubs.org/api/v2/report?report=histogram&x=genome_size&..."

    Returns:
        Dict containing query_yaml, params_yaml, and report_yaml for V3 API.
    """
    parsed = urlparse(v2_url)
    params = parse_qs(parsed.query)

    # Flatten single-element lists from parse_qs
    flat_params = {k: v[0] if len(v) == 1 else v for k, v in params.items()}

    # Extract report type (required)
    report_type = flat_params.get("report")
    if not report_type:
        raise ValueError("V2 URL missing required 'report' parameter")

    # Map result type to index
    result_type = flat_params.get("result", "taxon")
    index_map = {
        "taxon": "taxon",
        "assembly": "assembly",
        "sample": "sample",
    }
    index = index_map.get(result_type, result_type)

    # Build query YAML
    query_yaml = f"index: {index}\n"

    # Add taxa/attributes filter
    if "query" in flat_params:
        # V2 query param is a single taxa/attribute filter
        query_yaml += f"taxa: [{flat_params['query']}]\n"

    # Arc uses ranks in the report config (per-rank arc); other types use rank in query
    if "rank" in flat_params and report_type != "arc":
        query_yaml += f"rank: {flat_params['rank']}\n"

    if "fields" in flat_params:
        # fields can be a list or comma-separated string
        fields_val = flat_params["fields"]
        if isinstance(fields_val, list):
            fields_str = ", ".join(fields_val)
        else:
            fields_str = fields_val
        query_yaml += f"fields: [{fields_str}]\n"

    # Detect tree-specific taxa filter
    if "taxa" in flat_params:
        taxa_val = flat_params["taxa"]
        if isinstance(taxa_val, list):
            taxa_str = ", ".join(taxa_val)
        else:
            taxa_str = taxa_val
        query_yaml += f"taxa: [{taxa_str}]\n"

    # For tree reports, use tree taxa filter
    if report_type == "tree" and "taxa" in flat_params:
        query_yaml += "taxon_filter_type: tree\n"

    # Build params YAML
    params_yaml = ""
    if "taxonomy" in flat_params:
        params_yaml += f"taxonomy: {flat_params['taxonomy']}\n"
    if "includeEstimates" in flat_params:
        params_yaml += f"include_estimates: {flat_params['includeEstimates'].lower()}\n"

    if not params_yaml:
        params_yaml = "taxonomy: ncbi\n"

    # Build report YAML
    report_yaml = f"report: {report_type}\n"

    if report_type == "arc":
        # V2 arc: x=field_name, y=optional, z=optional
        # V3 arc: feature/reference/context with filter expressions or bare field names.
        # Bare field names are valid in V3 (Exists filter = "has any value for this attribute").
        # reference is optional in V3 (empty = all taxa in base query).
        if "x" in flat_params:
            report_yaml += f"feature: {flat_params['x']}\n"
        if "y" in flat_params:
            report_yaml += f"reference: {flat_params['y']}\n"
        if "z" in flat_params:
            report_yaml += f"context: {flat_params['z']}\n"
        # Pass ranks as a list so V3 does arcPerRank (matches V2 arcPerRank behaviour)
        if "rank" in flat_params:
            ranks_list = ", ".join(flat_params["rank"].split(","))
            report_yaml += f"ranks: [{ranks_list}]\n"
    else:
        # Map axis parameters (x, y, z) - V3 uses same names as V2 for other report types
        for axis in ("x", "y", "z"):
            if axis in flat_params:
                report_yaml += f"{axis}: {flat_params[axis]}\n"

    # Map axis opts (x_opts, y_opts, z_opts) for non-arc report types
    opts_map = {
        "x_opts": "x_opts",
        "y_opts": "y_opts",
        "z_opts": "z_opts",
    }
    for v2_key, v3_key in opts_map.items():
        if v2_key in flat_params:
            report_yaml += f"{v3_key}: {flat_params[v2_key]}\n"

    # Map categorization parameters
    if "cat" in flat_params:
        report_yaml += f"cat: {flat_params['cat']}\n"
    if "cat_opts" in flat_params:
        report_yaml += f"cat_opts: {flat_params['cat_opts']}\n"

    # Add size limit if present
    if "size" in flat_params:
        try:
            size = int(flat_params["size"])
            report_yaml += f"size: {size}\n"
        except (ValueError, TypeError):
            pass

    return {
        "query_yaml": query_yaml,
        "params_yaml": params_yaml,
        "report_yaml": report_yaml,
    }


def translate_v2_taxon_to_v3(v2_taxa: str) -> str:
    """Convert a V2 taxa string (often with synonyms/assembly names) to V3 taxon filter.

    Args:
        v2_taxa: V2 taxa parameter value, e.g. "Animalia" or "Mammalia--goat"

    Returns:
        V3 taxa value, e.g. "Mammalia"
    """
    # Strip assembly/dataset suffix if present (e.g. "Mammalia--goat" -> "Mammalia")
    if "--" in v2_taxa:
        return v2_taxa.split("--")[0]
    return v2_taxa


def v2_field_opts_to_axis_opts(v2_opts_str: str) -> str:
    """Convert V2 axis opts format to V3 format (identity; both use semicolon separator).

    Args:
        v2_opts_str: V2 opts string, e.g. "min;max;size;scale;sort;interval"

    Returns:
        V3 opts string (same format).
    """
    return v2_opts_str
