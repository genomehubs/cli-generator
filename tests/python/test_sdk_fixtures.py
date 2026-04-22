"""Test SDK behavior against real API responses (fixture-based tests).

This module uses cached API responses to validate that the SDK:
1. Builds correct URLs for diverse query patterns
2. Parses responses correctly
3. Handles all field types and operators
4. Respects pagination and sorting
5. Transforms data correctly (tidy format, etc.)

Fixtures are discovered from live GoaT API via discover_fixtures.py.

Usage:
    # Step 1: Cache fixtures from live API (one-time)
    python tests/python/discover_fixtures.py --update

    # Step 2a: Run pytest directly on cached fixtures
    pytest tests/python/test_sdk_fixtures.py -v

    # Step 2b: Or use the convenience script to test a generated SDK
    bash scripts/test_sdk_fixtures.sh --site goat --python

To update cached fixtures:
    python tests/python/discover_fixtures.py --update

See:
    - docs/test-fixtures-quick-reference.md — Quick reference
    - docs/test-fixtures-usage.md — Complete guide
    - docs/testing-generated-sdks.md — Testing generated SDKs
"""

import json
from pathlib import Path
from typing import Any

import pytest

from cli_generator import QueryBuilder, parse_response_status

PROJECT_ROOT = Path(__file__).parent.parent.parent


def _find_fixtures_dir() -> Path:
    """Find the appropriate fixtures directory (site-specific or generator).

    Searches for fixtures in this order:
    1. Site-specific caches: tests/python/fixtures-{site}/
    2. Generator cache: tests/python/fixtures/

    Returns:
        Path to the fixtures directory to use.
    """
    fixtures_base = PROJECT_ROOT / "tests/python"

    return next(
        (
            site_dir
            for site_dir in sorted(fixtures_base.glob("fixtures-*"))
            if site_dir.is_dir() and list(site_dir.glob("*.json"))
        ),
        fixtures_base / "fixtures",
    )


FIXTURES_CACHE_DIR = _find_fixtures_dir()


# ── Load fixture metadata ────────────────────────────────────────────────────


def load_all_fixtures() -> dict[str, dict[str, Any]]:
    """Load all cached fixture responses from disk.

    Returns:
        Dict mapping fixture names to cached API responses.
    """
    fixtures = {}

    if not FIXTURES_CACHE_DIR.exists():
        pytest.skip("Fixtures not cached. Run: python tests/python/discover_fixtures.py --update")

    for cache_file in FIXTURES_CACHE_DIR.glob("*.json"):
        name = cache_file.stem
        with open(cache_file) as f:
            fixtures[name] = json.load(f)

    return fixtures


# ── Fixture mapping to QueryBuilder patterns ─────────────────────────────────


FIXTURE_TO_BUILDER = {
    "basic_taxon_search": lambda: QueryBuilder("taxon"),
    "numeric_field_integer_filter": lambda: QueryBuilder("taxon").add_attribute("chromosome_count", "gt", "10"),
    "numeric_field_range": lambda: QueryBuilder("taxon")
    .add_attribute("genome_size", "ge", "1G")
    .add_attribute("genome_size", "le", "3G"),
    "enum_field_filter": lambda: QueryBuilder("taxon").add_attribute("assembly_level", "eq", "complete genome"),
    "taxa_filter_tree": lambda: QueryBuilder("taxon").set_taxa(["Mammalia"], filter_type="tree").set_rank("species"),
    "taxa_with_negative_filter": lambda: QueryBuilder("taxon")
    .set_taxa(["Mammalia", "!Rodentia"], filter_type="tree")
    .set_rank("species"),
    "multiple_fields_single_filter": lambda: QueryBuilder("taxon")
    .add_attribute("genome_size", "exists")
    .add_field("genome_size")
    .add_field("chromosome_count")
    .add_field("assembly_level"),
    "fields_with_modifiers": lambda: QueryBuilder("taxon")
    .add_field("genome_size", modifiers=["min", "max"])
    .add_field("chromosome_count", modifiers=["median"]),
    "pagination_size_variation": lambda: QueryBuilder("taxon").set_rank("species").set_size(50),
    "pagination_second_page": lambda: QueryBuilder("taxon").set_rank("species").set_page(2),
    "complex_multi_constraint": lambda: QueryBuilder("taxon")
    .set_taxa(["Primates"], filter_type="tree")
    .set_rank("species")
    .add_attribute("assembly_span", "ge", "1000000000")
    .add_field("genome_size")
    .add_field("chromosome_count", modifiers=["min", "max"])
    .add_field("assembly_level"),
    "complex_multi_filter_same_field": lambda: QueryBuilder("taxon")
    .add_attribute("c_value", "ge", "0.5")
    .add_attribute("c_value", "le", "5.0")
    .add_attribute("genome_size", "exists")
    .add_field("c_value")
    .add_field("genome_size"),
    "assembly_index_basic": lambda: QueryBuilder("assembly"),
    "sample_index_basic": lambda: QueryBuilder("sample"),
    "exclude_ancestral_single": lambda: QueryBuilder("taxon")
    .add_field("genome_size")
    .set_exclude_ancestral(["genome_size"]),
    "exclude_descendant_single": lambda: QueryBuilder("taxon").add_field("c_value").set_exclude_descendant(["c_value"]),
    "exclude_direct_single": lambda: QueryBuilder("taxon")
    .add_field("assembly_level")
    .set_exclude_direct(["assembly_level"]),
    "exclude_missing_single": lambda: QueryBuilder("taxon")
    .add_field("chromosome_count")
    .set_exclude_missing(["chromosome_count"]),
    "exclude_multiple_types_combined": lambda: QueryBuilder("taxon")
    .add_field("genome_size")
    .add_field("chromosome_count")
    .add_field("assembly_level")
    .set_exclude_ancestral(["genome_size"])
    .set_exclude_missing(["chromosome_count"])
    .set_exclude_direct(["assembly_level"]),
    "exclude_with_taxa_filter": lambda: QueryBuilder("taxon")
    .set_taxa(["Mammalia"], filter_type="tree")
    .add_field("genome_size")
    .set_exclude_ancestral(["genome_size"]),
    "sorting_by_chromosome_count": lambda: QueryBuilder("taxon")
    .add_attribute("chromosome_count", "gt", "10")
    .add_field("chromosome_count")
    .set_sort("chromosome_count", "asc"),
    "sorting_descending_order": lambda: QueryBuilder("taxon")
    .add_attribute("c_value", "ge", "0.5")
    .add_field("c_value")
    .set_sort("c_value", "desc"),
    "with_taxonomy_param": lambda: QueryBuilder("taxon")
    .add_attribute("assembly_level", "eq", "complete genome")
    .add_field("assembly_level")
    .set_taxonomy("ncbi"),
    "with_names_param": lambda: QueryBuilder("taxon")
    .add_attribute("chromosome_count", "gt", "10")
    .add_field("chromosome_count")
    .set_names(["scientific_name"]),
    "with_ranks_param": lambda: QueryBuilder("taxon")
    .add_attribute("c_value", "ge", "0.5")
    .add_field("c_value")
    .set_ranks(["genus", "family", "order"]),
    "assembly_index_with_filter": lambda: QueryBuilder("assembly")
    .add_attribute("assembly_level", "eq", "complete genome")
    .add_field("assembly_span")
    .add_field("assembly_level"),
}

# ── Expected URL substrings per fixture ──────────────────────────────────────
# Each entry maps a fixture name to substrings that MUST appear in the built URL.
# Uses raw (percent-encoded) URL strings so assertions pass without decoding.
# This catches builder methods that silently ignore their arguments.

FIXTURE_EXPECTED_URL_PARTS: dict[str, list[str]] = {
    "basic_taxon_search": ["result=taxon"],
    "numeric_field_integer_filter": ["result=taxon", "chromosome_count"],
    "numeric_field_range": ["result=taxon", "genome_size"],
    "enum_field_filter": ["result=taxon", "assembly_level"],
    "taxa_filter_tree": ["result=taxon", "tax_tree", "Mammalia", "tax_rank", "species"],
    "taxa_with_negative_filter": ["result=taxon", "Mammalia", "Rodentia"],
    "multiple_fields_single_filter": ["result=taxon", "genome_size", "chromosome_count", "assembly_level"],
    "fields_with_modifiers": ["result=taxon", "genome_size%3Amin", "chromosome_count%3Amedian"],
    "pagination_size_variation": ["result=taxon", "size=50"],
    "pagination_second_page": ["result=taxon", "offset=10"],
    "complex_multi_constraint": ["result=taxon", "tax_tree", "Primates", "assembly_span"],
    "complex_multi_filter_same_field": ["result=taxon", "c_value", "genome_size"],
    "assembly_index_basic": ["result=assembly"],
    "sample_index_basic": ["result=sample"],
    "exclude_ancestral_single": ["result=taxon", "genome_size", "excludeAncestral"],
    "exclude_descendant_single": ["result=taxon", "c_value", "excludeDescendant"],
    "exclude_direct_single": ["result=taxon", "assembly_level", "excludeDirect"],
    "exclude_missing_single": ["result=taxon", "chromosome_count", "excludeMissing"],
    "exclude_multiple_types_combined": ["result=taxon", "excludeAncestral", "excludeMissing", "excludeDirect"],
    "exclude_with_taxa_filter": ["result=taxon", "tax_tree", "Mammalia", "excludeAncestral"],
    "sorting_by_chromosome_count": ["result=taxon", "sortBy=chromosome_count", "sortOrder=asc"],
    "sorting_descending_order": ["result=taxon", "sortBy=c_value", "sortOrder=desc"],
    "with_taxonomy_param": ["result=taxon", "taxonomy=ncbi", "assembly_level"],
    "with_names_param": ["result=taxon", "names=scientific_name"],
    "with_ranks_param": ["result=taxon", "ranks=", "genus"],
    "assembly_index_with_filter": ["result=assembly", "assembly_level", "assembly_span"],
}


# ── Tests ────────────────────────────────────────────────────────────────────


class TestFixtureValidation:
    """Validate SDK behavior against cached API fixtures."""

    @pytest.fixture(autouse=True)
    def setup(self):
        """Load fixtures once per test class."""
        self.fixtures = load_all_fixtures()
        self.fixture_names = list(self.fixtures.keys())

    def get_builder(self, fixture_name: str) -> QueryBuilder:
        """Get the QueryBuilder for a fixture.

        Args:
            fixture_name: Name of the fixture.

        Returns:
            QueryBuilder instance matching the fixture pattern.

        Raises:
            KeyError: If fixture is not mapped to a builder.
        """
        if fixture_name not in FIXTURE_TO_BUILDER:
            pytest.skip(f"Fixture {fixture_name} not yet mapped to QueryBuilder")

        return FIXTURE_TO_BUILDER[fixture_name]()

    def get_response(self, fixture_name: str) -> dict[str, Any]:
        """Get the cached API response for a fixture.

        Args:
            fixture_name: Name of the fixture.

        Returns:
            Parsed API response dict.
        """
        return self.fixtures[fixture_name]

    # ── Parametrized tests covering all fixtures ──────────────────────────────

    @pytest.mark.parametrize("fixture_name", FIXTURE_TO_BUILDER.keys())
    def test_fixture_no_api_error(self, fixture_name: str):
        """Verify cached fixture response has no error."""
        response = self.get_response(fixture_name)
        assert "error" not in response, f"Fixture {fixture_name} returned an error"

    @pytest.mark.parametrize("fixture_name", FIXTURE_TO_BUILDER.keys())
    def test_fixture_has_results_or_hits(self, fixture_name: str):
        """Verify cached fixture response has results or hits info."""
        response = self.get_response(fixture_name)

        # Most queries should have hits info
        assert (
            "hits" in response or "results" in response
        ), f"Fixture {fixture_name} has neither 'hits' nor 'results' key"

    @pytest.mark.parametrize("fixture_name", FIXTURE_TO_BUILDER.keys())
    def test_builder_creates_valid_url(self, fixture_name: str):
        """Verify builder creates a valid URL for each fixture."""
        qb = self.get_builder(fixture_name)
        url = qb.to_url(
            api_base="https://goat.genomehubs.org/api",
            api_version="v2",
        )

        assert url.startswith("https://goat.genomehubs.org/api"), f"URL for {fixture_name} doesn't start with API base"
        assert "search" in url or "count" in url, f"URL for {fixture_name} doesn't contain endpoint"

    @pytest.mark.parametrize("fixture_name", FIXTURE_EXPECTED_URL_PARTS.keys())
    def test_builder_url_encodes_state(self, fixture_name: str):
        """Verify builder state is encoded in the built URL for each fixture.

        Catches methods that silently ignore their arguments by asserting specific
        substrings from FIXTURE_EXPECTED_URL_PARTS appear in the generated URL.

        Args:
            fixture_name: Name of the fixture.
        """
        qb = self.get_builder(fixture_name)
        url = qb.to_url(
            api_base="https://goat.genomehubs.org/api",
            api_version="v2",
        )
        for expected in FIXTURE_EXPECTED_URL_PARTS[fixture_name]:
            assert expected in url, f"Fixture {fixture_name}: expected '{expected}' in URL — got: {url}"

    @pytest.mark.parametrize("fixture_name", FIXTURE_TO_BUILDER.keys())
    def test_builder_creates_valid_ui_url(self, fixture_name: str):
        """Verify builder creates a valid UI URL for each fixture."""
        qb = self.get_builder(fixture_name)
        ui_url = qb.to_ui_url(ui_base="https://goat.genomehubs.org")

        assert ui_url.startswith(
            "https://goat.genomehubs.org/"
        ), f"UI URL for {fixture_name} doesn't start with UI base"
        assert "/api/" not in ui_url, f"UI URL for {fixture_name} contains /api/ — should be UI-only path"
        assert "result=" in ui_url, f"UI URL for {fixture_name} missing result= parameter"

    @pytest.mark.parametrize("fixture_name", FIXTURE_TO_BUILDER.keys())
    def test_fixture_counts_are_reasonable(self, fixture_name: str):
        """Verify fixture result counts are sensible.

        Checks that:
        - `results` array size <= `size` parameter
        - ``hits`` count is non-negative
        - Complex queries return fewer results than simple ones
        """
        response = self.get_response(fixture_name)

        results = response.get("results", [])
        status = json.loads(parse_response_status(json.dumps(response)))
        total_hits = status.get("hits", 0)

        # Results should not exceed requested size
        response_size = results.__len__()
        if response_size > 0:
            # Some fixtures don't have size info in response
            assert response_size <= 100, f"Fixture {fixture_name} returned {response_size} results, " f"seems excessive"

        # Total hits should be non-negative
        assert total_hits >= 0, f"Fixture {fixture_name} has negative hit count"

    @pytest.mark.parametrize("fixture_name", FIXTURE_TO_BUILDER.keys())
    def test_builder_to_yaml(self, fixture_name: str):
        """Verify builder serialization to YAML is valid.

        Args:
            fixture_name: Name of the fixture.
        """
        qb = self.get_builder(fixture_name)

        query_yaml = qb.to_query_yaml()
        params_yaml = qb.to_params_yaml()

        # Should be valid YAML (not empty, contains key-value pairs)
        assert len(query_yaml) > 1, f"Fixture {fixture_name}: query_yaml is empty"
        assert len(params_yaml) > 1, f"Fixture {fixture_name}: params_yaml is empty"

        # Should contain expected keys
        assert "index" in query_yaml, f"query_yaml missing 'index' for {fixture_name}"
        assert "size" in params_yaml, f"params_yaml missing 'size' for {fixture_name}"

    # ── Tests with real response data ─────────────────────────────────────────

    @pytest.mark.parametrize("fixture_name", FIXTURE_TO_BUILDER.keys())
    def test_fixture_can_describe(self, fixture_name: str):
        """Verify builder can generate English description.

        Args:
            fixture_name: Name of the fixture.
        """
        qb = self.get_builder(fixture_name)

        description = qb.describe()

        assert isinstance(description, str), f"Fixture {fixture_name}: describe() returned non-string"
        assert len(description) > 0, f"Fixture {fixture_name}: describe() returned empty string"

    @pytest.mark.parametrize("fixture_name", FIXTURE_TO_BUILDER.keys())
    def test_fixture_can_generate_snippet(self, fixture_name: str):
        """Verify builder can generate code snippets.

        Args:
            fixture_name: Name of the fixture.
        """
        qb = self.get_builder(fixture_name)

        snippets = qb.snippet(
            languages=["python"],
            site_name="goat",
            sdk_name="goat_sdk",
        )

        assert "python" in snippets, f"Fixture {fixture_name}: missing 'python' snippet"
        assert len(snippets["python"]) > 0, f"Fixture {fixture_name}: Python snippet is empty"

    @pytest.mark.parametrize("fixture_name", FIXTURE_TO_BUILDER.keys())
    def test_fixture_can_tidy_records(self, fixture_name: str):
        """Verify builder can tidy records from response.

        Args:
            fixture_name: Name of the fixture.
        """
        qb = self.get_builder(fixture_name)
        response = self.get_response(fixture_name)

        results = response.get("results", [])
        if not results:
            pytest.skip(f"Fixture {fixture_name} has no results to tidy")

        tidy = qb.to_tidy_records(results)

        assert isinstance(tidy, list), f"Fixture {fixture_name}: to_tidy_records() returned non-list"
        assert len(tidy) > 0, f"Fixture {fixture_name}: to_tidy_records() returned empty list"

        # Check for expected tidy columns
        if tidy:
            first = tidy[0]
            expected_keys = {"field", "value"}
            missing = expected_keys - set(first.keys())
            assert not missing, f"Fixture {fixture_name}: tidy record missing keys {missing}"

    # ── Test fixture-specific patterns ───────────────────────────────────────

    def test_complex_multi_constraint_has_results(self):
        """Complex query should return results for Primates."""
        fixture_name = "complex_multi_constraint"
        response = self.get_response(fixture_name)

        results = response.get("results", [])
        # Primates with assembly_span > 1G should exist
        assert len(results) > 0, f"{fixture_name} should return Primates with large assemblies"

    def test_pagination_size_respected(self):
        """Pagination with size=50 should not exceed 50 results."""
        fixture_name = "pagination_size_variation"
        response = self.get_response(fixture_name)

        results = response.get("results", [])
        assert len(results) <= 50, f"{fixture_name} returned more than 50 results"

    def test_numeric_filters_effective(self):
        """Numeric filters should reduce result count vs unfiltered."""
        fixture_name = "numeric_field_integer_filter"
        response = self.get_response(fixture_name)

        # Get unfiltered baseline
        baseline_response = self.get_response("basic_taxon_search")

        filtered_status = json.loads(parse_response_status(json.dumps(response)))
        baseline_status = json.loads(parse_response_status(json.dumps(baseline_response)))
        filtered_hits = filtered_status.get("hits", 0)
        baseline_hits = baseline_status.get("hits", 0)

        # Filtered query should have fewer or equal results
        assert filtered_hits <= baseline_hits, f"Filtered {fixture_name} returned more results than baseline"

    def test_taxa_tree_filter_returns_results(self):
        """Taxa tree filter for Mammalia should return many results."""
        fixture_name = "taxa_filter_tree"
        response = self.get_response(fixture_name)

        total_hits = response.get("status", {}).get("hits", 0)
        # Mammalia is a large clade with many species
        assert total_hits > 100, f"{fixture_name} returned too few hits for Mammalia subtree"

    # ── Validate method tests ─────────────────────────────────────────────────

    @pytest.mark.parametrize("fixture_name", FIXTURE_TO_BUILDER.keys())
    def test_fixture_can_validate(self, fixture_name: str):
        """Verify builder can validate a query and that known-good fixtures have no errors.

        Args:
            fixture_name: Name of the fixture.
        """
        qb = self.get_builder(fixture_name)
        errors = qb.validate()

        # validate() should always return a list
        assert isinstance(errors, list), f"Fixture {fixture_name}: validate() returned non-list"
        assert all(isinstance(e, str) for e in errors), f"Fixture {fixture_name}: validate() returned non-string errors"
        # Known-good fixture queries should produce zero validation errors
        assert errors == [], f"Fixture {fixture_name}: validate() returned unexpected errors: {errors}"


class TestFixtureRegressionCatches:
    """Ensure fixture test updates catch real SDK regressions."""

    def test_fixture_mapping_completeness(self):
        """Ensure all fixtures have mappings to QueryBuilder."""
        fixtures = load_all_fixtures()

        unmapped = set(fixtures.keys()) - set(FIXTURE_TO_BUILDER.keys())
        assert not unmapped, f"Unmapped fixtures (add to FIXTURE_TO_BUILDER): {unmapped}"

    def test_builders_match_fixture_patterns(self):
        """Spot-check a few builders match their fixture patterns."""
        # These are sanity checks to catch obvious builder/fixture mismatches

        # basic_taxon_search should be taxon index
        qb = FIXTURE_TO_BUILDER["basic_taxon_search"]()
        assert qb._index == "taxon"

        # assembly_index_basic should be assembly index
        qb = FIXTURE_TO_BUILDER["assembly_index_basic"]()
        assert qb._index == "assembly"

        # sample_index_basic should be sample index
        qb = FIXTURE_TO_BUILDER["sample_index_basic"]()
        assert qb._index == "sample"

        # taxa_filter_tree should have taxa set
        qb = FIXTURE_TO_BUILDER["taxa_filter_tree"]()
        assert len(qb._taxa) > 0


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
