"""Structural parity assertions for V2 vs V3 report responses.

Validates that V3 responses contain all required data for rendering,
without requiring byte-for-byte identity with V2.
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

    # Skip type check - V3 may have different structure per report type
    # Just ensure v3_report is a dict with content
    assert isinstance(v3_report, dict) and len(v3_report) > 0, (
        f"V3 report missing or empty for {report_type}"
    )

    # Dispatch to type-specific assertions
    if report_type == "histogram":
        assert_histogram_parity(v2_report, v3_report, divergences)
    elif report_type == "scatter":
        assert_scatter_parity(v2_report, v3_report, divergences)
    elif report_type == "arc":
        assert_arc_parity(v2_report, v3_report, divergences)
    elif report_type == "tree":
        assert_tree_parity(v2_report, v3_report, divergences)
    elif report_type == "map":
        assert_map_parity(v2_report, v3_report, divergences)
    elif report_type == "xPerRank":
        assert_xperrank_parity(v2_report, v3_report, divergences)
    elif report_type == "sources":
        assert_sources_parity(v2_report, v3_report, divergences)
    else:
        raise ValueError(f"Unknown report type: {report_type}")


def assert_histogram_parity(
    v2_report: Dict[str, Any],
    v3_report: Dict[str, Any],
    divergences: List[str],
) -> None:
    """Histogram-specific parity assertions."""
    assert v3_report.get("type") == "histogram"
    assert "x" in v3_report or "values" in v3_report, "Missing x/values in histogram"

    # Check for buckets or similar structure
    assert (
        "buckets" in v3_report or "values" in v3_report
    ), "Missing buckets or values"

    # If V2 had data, V3 should have data
    if v2_report.get("values") or v2_report.get("buckets"):
        assert (
            v3_report.get("values") or v3_report.get("buckets")
        ), "V3 histogram empty but V2 had data"

    # Check for categorization if present in V2
    if "by_cat" in v2_report or "cats" in v2_report:
        assert (
            "by_cat" in v3_report
        ), "V2 had by_cat but V3 missing it"


def assert_scatter_parity(
    v2_report: Dict[str, Any],
    v3_report: Dict[str, Any],
    divergences: List[str],
) -> None:
    """Scatter-specific parity assertions."""
    assert v3_report.get("type") == "scatter"
    assert "x" in v3_report or "values" in v3_report, "Missing x/values in scatter"
    assert "y" in v3_report or "yValues" in v3_report, "Missing y/yValues in scatter"

    # If V2 had buckets, V3 should have buckets
    if v2_report.get("buckets") or v2_report.get("values"):
        assert (
            v3_report.get("buckets") or v3_report.get("values")
        ), "V3 scatter empty but V2 had data"

    # Check for categorization if present in V2
    if "by_cat" in v2_report:
        assert (
            "by_cat" in v3_report
        ), "V2 had by_cat but V3 missing it"


def assert_arc_parity(
    v2_report: Dict[str, Any],
    v3_report: Dict[str, Any],
    divergences: List[str],
) -> None:
    """Arc-specific parity assertions."""
    assert v3_report.get("type") == "arc"

    # Check for arc value (fraction or multi-ring array)
    if "rings" in v3_report or isinstance(v3_report.get("arc"), list):
        # Multi-ring arc
        assert isinstance(v3_report.get("arc"), list), "Multi-ring arc should have array"
        assert len(v3_report["arc"]) > 0, "Multi-ring arc array is empty"
        for ring in v3_report["arc"]:
            assert "arc" in ring or isinstance(ring.get("arc"), (int, float))
    else:
        # Single arc
        arc_val = v3_report.get("arc")
        assert arc_val is not None, "Arc report missing 'arc' value"
        assert isinstance(arc_val, (int, float)), f"Arc value should be numeric, got {type(arc_val)}"

    # Check count fields
    assert "feature_count" in v3_report or "x" in v3_report, "Missing count fields"
    assert (
        "reference_count" in v3_report or "y" in v3_report
    ), "Missing reference count"

    # Check field descriptors
    assert "featureTerm" in v3_report or "xTerm" in v3_report, "Missing feature term"
    assert (
        "referenceTerm" in v3_report or "yTerm" in v3_report
    ), "Missing reference term"


def assert_tree_parity(
    v2_report: Dict[str, Any],
    v3_report: Dict[str, Any],
    divergences: List[str],
) -> None:
    """Tree-specific parity assertions."""
    assert v3_report.get("type") == "tree"

    # Check for tree structure (Newick string or node array)
    assert (
        "tree" in v3_report or "nodes" in v3_report or "root" in v3_report
    ), "Missing tree data"

    # If V2 had a tree, V3 should have tree data
    if v2_report.get("tree") or v2_report.get("nodes"):
        assert (
            v3_report.get("tree")
            or v3_report.get("nodes")
            or v3_report.get("root")
        ), "V3 tree empty but V2 had data"


def assert_map_parity(
    v2_report: Dict[str, Any],
    v3_report: Dict[str, Any],
    divergences: List[str],
) -> None:
    """Map-specific parity assertions."""
    assert v3_report.get("type") == "map"

    # Check for geographic data
    assert (
        "map" in v3_report or "locations" in v3_report or "features" in v3_report
    ), "Missing map data"


def assert_xperrank_parity(
    v2_report: Dict[str, Any],
    v3_report: Dict[str, Any],
    divergences: List[str],
) -> None:
    """XPerRank-specific parity assertions."""
    assert v3_report.get("type") == "xPerRank"

    # Check for rank-keyed data
    assert (
        "ranks" in v3_report or "by_rank" in v3_report or isinstance(v3_report.get("values"), dict)
    ), "Missing rank data"


def assert_sources_parity(
    v2_report: Dict[str, Any],
    v3_report: Dict[str, Any],
    divergences: List[str],
) -> None:
    """Sources-specific parity assertions."""
    assert v3_report.get("type") == "sources"

    # Check for sources array
    assert "sources" in v3_report or "data" in v3_report, "Missing sources data"

    # If V2 had sources, V3 should have sources
    if v2_report.get("sources"):
        assert (
            v3_report.get("sources") or v3_report.get("data")
        ), "V3 sources empty but V2 had data"


def assert_counts_plausible(
    v2_report: Dict[str, Any],
    v3_report: Dict[str, Any],
) -> None:
    """Assert that V3 counts are plausible relative to V2 (allow growth over time).

    Counts in V3 should be >= V2 counts (or equal), since the underlying
    dataset may have grown between fixture collection and validation.
    """
    # Extract count fields from both versions
    v2_counts = extract_counts(v2_report)
    v3_counts = extract_counts(v3_report)

    for field, v2_val in v2_counts.items():
        v3_val = v3_counts.get(field)
        if v3_val is None or v2_val is None:
            continue
        # V3 count should be >= V2 count (allowing for data growth)
        assert v3_val >= v2_val, (
            f"Count {field}: V3 ({v3_val}) < V2 ({v2_val}); "
            "expected growth or stability"
        )


def extract_counts(report: Dict[str, Any]) -> Dict[str, int]:
    """Extract all numeric count fields from a report.

    Returns:
        Dict of field -> count value pairs.
    """
    counts = {}

    # Common count field names
    for key in ["x", "y", "z", "feature_count", "reference_count", "context_count"]:
        if key in report and isinstance(report[key], int):
            counts[key] = report[key]

    # Multi-ring arcs
    if isinstance(report.get("arc"), list):
        for i, ring in enumerate(report["arc"]):
            if isinstance(ring, dict):
                for key in ["feature_count", "reference_count"]:
                    if key in ring and isinstance(ring[key], int):
                        counts[f"{key}_ring_{i}"] = ring[key]

    return counts
