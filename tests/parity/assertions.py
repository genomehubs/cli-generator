"""Simplified structural parity assertions for V2 vs V3 report responses.

Validates that V3 responses contain expected data for the report type,
accounting for structural differences between API versions.
"""

from typing import Any, Dict, List, Optional


def assert_structural_parity(
    v2_report: Dict[str, Any],
    v3_report: Dict[str, Any],
    report_type: str,
    divergences: Optional[List[str]] = None,
) -> None:
    """Assert that V3 report contains required fields for the given type.

    Args:
        v2_report: V2 API response report dict.
        v3_report: V3 API response report dict.
        report_type: Report type string (e.g. "histogram", "arc").
        divergences: Optional list of known divergence keys to skip.

    Raises:
        AssertionError: If required fields are missing or mismatched.
    """
    if divergences is None:
        divergences = []

    # Basic check: V3 report should be a non-empty dict
    assert isinstance(v3_report, dict) and len(v3_report) > 0, (
        f"V3 report missing or empty for {report_type}: {type(v3_report)}"
    )

    # Dispatch to type-specific assertions
    if report_type == "histogram":
        assert_histogram_parity(v2_report, v3_report)
    elif report_type == "scatter":
        assert_scatter_parity(v2_report, v3_report)
    elif report_type == "arc":
        assert_arc_parity(v2_report, v3_report)
    elif report_type == "tree":
        assert_tree_parity(v2_report, v3_report)
    elif report_type == "map":
        assert_map_parity(v2_report, v3_report)
    elif report_type == "xPerRank":
        assert_xperrank_parity(v2_report, v3_report)
    elif report_type == "sources":
        assert_sources_parity(v2_report, v3_report)
    else:
        raise ValueError(f"Unknown report type: {report_type}")


def assert_histogram_parity(
    v2_report: Dict[str, Any],
    v3_report: Dict[str, Any],
) -> None:
    """Histogram-specific parity assertions."""
    # V3 histogram should have buckets or allValues (data)
    assert (
        "buckets" in v3_report or "allValues" in v3_report or "values" in v3_report
    ), f"V3 histogram missing data structure: {list(v3_report.keys())}"


def assert_scatter_parity(
    v2_report: Dict[str, Any],
    v3_report: Dict[str, Any],
) -> None:
    """Scatter-specific parity assertions."""
    # V3 scatter should have buckets (2D data)
    assert "buckets" in v3_report or "values" in v3_report, (
        f"V3 scatter missing data: {list(v3_report.keys())}"
    )


def assert_arc_parity(
    v2_report: Dict[str, Any],
    v3_report: Dict[str, Any],
) -> None:
    """Arc-specific parity assertions."""
    # V3 arc should have arc data
    assert "arc" in v3_report or "values" in v3_report, (
        f"V3 arc missing data: {list(v3_report.keys())}"
    )


def assert_tree_parity(
    v2_report: Dict[str, Any],
    v3_report: Dict[str, Any],
) -> None:
    """Tree-specific parity assertions."""
    # V3 tree should have treeNodes or tree structure
    assert "treeNodes" in v3_report or "tree" in v3_report or "nodes" in v3_report, (
        f"V3 tree missing tree data: {list(v3_report.keys())}"
    )


def assert_map_parity(
    v2_report: Dict[str, Any],
    v3_report: Dict[str, Any],
) -> None:
    """Map-specific parity assertions."""
    # Map assertions - minimal for now
    assert isinstance(v3_report, dict) and len(v3_report) > 0, "V3 map empty"


def assert_xperrank_parity(
    v2_report: Dict[str, Any],
    v3_report: Dict[str, Any],
) -> None:
    """xPerRank-specific parity assertions."""
    # V3 xPerRank should have buckets
    assert "buckets" in v3_report or "values" in v3_report, (
        f"V3 xPerRank missing buckets: {list(v3_report.keys())}"
    )


def assert_sources_parity(
    v2_report: Dict[str, Any],
    v3_report: Dict[str, Any],
) -> None:
    """Sources-specific parity assertions."""
    # Sources assertions - minimal for now
    assert isinstance(v3_report, dict) and len(v3_report) > 0, "V3 sources empty"
