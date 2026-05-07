"""Parametrised parity tests comparing V2 and V3 report responses."""

import json
from pathlib import Path
from typing import Generator, List, Tuple
import pytest

from .translate import translate_v2_url_to_v3_body
from .assertions import assert_structural_parity


def collect_v2_fixture_paths() -> List[Path]:
    """Collect all V2 fixture JSON files."""
    fixtures_dir = Path(__file__).parent.parent / "fixtures" / "parity" / "v2"
    if not fixtures_dir.exists():
        return []

    fixture_files = list(fixtures_dir.rglob("*.json"))
    return sorted(fixture_files)


def load_fixture_and_url(fixture_path: Path) -> Tuple[dict, str]:
    """Load fixture JSON and corresponding .url file."""
    fixture = json.loads(fixture_path.read_text())

    url_file = fixture_path.with_suffix(".url")
    url = url_file.read_text().strip() if url_file.exists() else ""

    return fixture, url


@pytest.mark.parametrize(
    "fixture_path",
    collect_v2_fixture_paths(),
    ids=lambda p: f"{p.parent.name}/{p.stem}",
)
def test_v2_fixture_exists(fixture_path: Path) -> None:
    """Sanity check: V2 fixture file exists and is valid JSON."""
    assert fixture_path.exists(), f"Fixture not found: {fixture_path}"

    try:
        fixture = json.loads(fixture_path.read_text())
        assert isinstance(fixture, dict), "Fixture should be a JSON object"
    except json.JSONDecodeError as e:
        pytest.fail(f"Fixture is invalid JSON: {e}")


@pytest.mark.parametrize(
    "fixture_path",
    collect_v2_fixture_paths(),
    ids=lambda p: f"{p.parent.name}/{p.stem}",
)
def test_fixture_has_url(fixture_path: Path) -> None:
    """Check that each fixture has a corresponding .url file."""
    url_file = fixture_path.with_suffix(".url")
    if not url_file.exists():
        pytest.skip(f"No .url file for {fixture_path.name}")

    url = url_file.read_text().strip()
    assert url.startswith("http"), f"URL file should contain a valid HTTP URL, got: {url}"


@pytest.mark.parametrize(
    "fixture_path",
    collect_v2_fixture_paths(),
    ids=lambda p: f"{p.parent.name}/{p.stem}",
)
def test_v2_to_v3_translation(fixture_path: Path) -> None:
    """Test that V2 URL can be translated to V3 request body."""
    url_file = fixture_path.with_suffix(".url")
    if not url_file.exists():
        pytest.skip(f"No .url file for {fixture_path.name}")

    url = url_file.read_text().strip()

    try:
        v3_body = translate_v2_url_to_v3_body(url)
        assert "query_yaml" in v3_body
        assert "params_yaml" in v3_body
        assert "report_yaml" in v3_body
    except Exception as e:
        pytest.fail(f"Translation failed for {url}: {e}")


@pytest.mark.parametrize(
    "fixture_path",
    collect_v2_fixture_paths(),
    ids=lambda p: f"{p.parent.name}/{p.stem}",
)
def test_v2_fixture_structure(fixture_path: Path) -> None:
    """Validate that V2 fixture has expected report structure."""
    fixture = json.loads(fixture_path.read_text())

    # V2 responses should have status and report
    assert "status" in fixture, "V2 response missing 'status'"
    assert "report" in fixture, "V2 response missing 'report'"

    report_type = fixture_path.parent.name
    v2_report = fixture["report"]

    # Basic type check
    assert (
        v2_report.get("type") == report_type or "type" not in v2_report
    ), f"Report type mismatch for {fixture_path.name}"


# Mark as integration tests
pytestmark = pytest.mark.integration


def load_v3_response(report_type: str) -> dict | None:
    """Load pre-collected V3 response for report type."""
    v3_file = Path(__file__).parent.parent / "fixtures" / "parity" / "v3" / f"{report_type}_response.json"
    if not v3_file.exists():
        return None
    with open(v3_file) as f:
        return json.load(f)


@pytest.mark.parametrize(
    "report_type",
    ["histogram", "scatter", "arc", "tree", "xPerRank"],
)
def test_v3_response_exists(report_type: str) -> None:
    """Test that V3 responses have been collected for each report type."""
    v3_response = load_v3_response(report_type)
    assert v3_response is not None, f"No V3 response collected for {report_type}"
    assert v3_response.get("status", {}).get("success"), f"V3 response for {report_type} failed"


@pytest.mark.parametrize(
    "fixture_path",
    collect_v2_fixture_paths(),
    ids=lambda p: f"{p.parent.name}/{p.stem}",
)
def test_v3_parity_with_v2_fixture(fixture_path: Path) -> None:
    """Test that V3 response has structural parity with corresponding V2 fixture type.

    This compares a V2 fixture to a V3 response of the same report type.
    Both should have compatible structure for rendering.
    """
    fixture, _ = load_fixture_and_url(fixture_path)
    if not fixture:
        pytest.skip(f"Could not load {fixture_path.name}")

    report_type = fixture_path.parent.name
    v3_response = load_v3_response(report_type)
    if not v3_response:
        pytest.skip(f"No V3 response collected for {report_type}")

    v2_report = fixture.get("report", {})
    v3_report = v3_response.get("report", {})

    # Assert structural parity
    assert_structural_parity(
        v2_report=v2_report,
        v3_report=v3_report,
        report_type=report_type,
    )
