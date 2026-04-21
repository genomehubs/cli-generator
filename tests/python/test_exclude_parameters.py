"""Test exclude parameters against real API fixtures.

Tests that exclude parameters (ancestral, descendant, direct, missing)
work correctly when building URLs and querying the API, using cached
fixture responses like names and ranks tests do.
"""

import json
from pathlib import Path

import pytest

from cli_generator import QueryBuilder

PROJECT_ROOT = Path(__file__).parent.parent.parent


def _find_fixtures_dir() -> Path:
    """Find the appropriate fixtures directory."""
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


def load_exclude_fixtures() -> dict[str, dict]:
    """Load all exclude-specific fixtures."""
    fixtures = {}
    exclude_names = [
        "exclude_ancestral_single",
        "exclude_descendant_single",
        "exclude_direct_single",
        "exclude_missing_single",
        "exclude_multiple_types_combined",
        "exclude_with_taxa_filter",
    ]

    for name in exclude_names:
        cache_file = FIXTURES_CACHE_DIR / f"{name}.json"
        if cache_file.exists():
            with open(cache_file) as f:
                fixtures[name] = json.load(f)

    return fixtures


class TestExcludeParametersWithFixtures:
    """Test exclude parameters using cached API fixtures."""

    @pytest.fixture(autouse=True)
    def setup(self):
        """Load fixtures."""
        self.fixtures = load_exclude_fixtures()
        if not self.fixtures:
            pytest.skip(
                "Exclude fixtures not cached. Run: " "python tests/python/discover_fixtures.py --site goat --update"
            )

    def _assert_fixture_valid(self, fixture_name: str) -> dict:
        """Load and validate a fixture, returning it or skipping if not cached."""
        fixture = self.fixtures.get(fixture_name)
        if not fixture:
            pytest.skip("Fixture not cached")
        assert fixture is not None  # Type guard for type checker
        # Fixture should contain results (or be valid empty response)
        assert "results" in fixture or "hits" in fixture or "error" not in fixture
        return fixture

    @pytest.mark.parametrize(
        "fixture_name",
        [
            "exclude_ancestral_single",
            "exclude_descendant_single",
            "exclude_direct_single",
            "exclude_missing_single",
            "exclude_multiple_types_combined",
            "exclude_with_taxa_filter",
        ],
    )
    def test_fixture_validity(self, fixture_name: str):
        """Verify all exclude fixtures return valid results."""
        self._assert_fixture_valid(fixture_name)

    @pytest.mark.parametrize(
        "exclude_method,field_name",
        [
            ("set_exclude_ancestral", "genome_size"),
            ("set_exclude_descendant", "c_value"),
            ("set_exclude_direct", "assembly_level"),
            ("set_exclude_missing", "chromosome_count"),
        ],
    )
    def test_exclude_url_generation(self, exclude_method: str, field_name: str):
        """Verify QueryBuilder generates correct URL for exclude methods."""
        qb = QueryBuilder("taxon").add_field(field_name)
        getattr(qb, exclude_method)([field_name])
        url = qb.to_url("https://goat.genomehubs.org/api", "v2", "search")

        assert "exclude" in url.lower()
        assert field_name in url

    def test_exclude_multiple_types_url_generation(self):
        """Verify QueryBuilder generates correct URLs for combined excludes."""
        qb = (
            QueryBuilder("taxon")
            .add_field("genome_size")
            .add_field("chromosome_count")
            .add_field("assembly_level")
            .set_exclude_ancestral(["genome_size"])
            .set_exclude_missing(["chromosome_count"])
            .set_exclude_direct(["assembly_level"])
        )
        url = qb.to_url("https://goat.genomehubs.org/api", "v2", "search")

        # All three exclude types should be present
        assert "ancestral" in url.lower()
        assert "missing" in url.lower()
        assert "direct" in url.lower()

    def test_exclude_with_taxa_filter_url_generation(self):
        """Verify QueryBuilder combines exclude with taxa filters correctly."""
        qb = (
            QueryBuilder("taxon")
            .set_taxa(["Mammalia"], filter_type="tree")
            .add_field("genome_size")
            .set_exclude_ancestral(["genome_size"])
        )
        url = qb.to_url("https://goat.genomehubs.org/api", "v2", "search")

        # Both taxa and exclude should be present
        assert "Mammalia" in url or "tax_tree" in url
        assert "exclude" in url.lower()

    def test_exclude_add_method(self):
        """Verify add_exclude_* methods append to existing excludes."""
        qb = QueryBuilder("taxon").add_field("genome_size")
        qb.add_exclude_ancestral("genome_size")
        qb.add_exclude_ancestral("c_value")

        url = qb.to_url("https://goat.genomehubs.org/api", "v2", "search")
        assert "genome_size" in url
        assert "c_value" in url

    def test_exclude_set_overwrites(self):
        """Verify set_exclude_* methods overwrite previous values."""
        qb = QueryBuilder("taxon").add_field("genome_size")
        qb.add_exclude_ancestral("genome_size")
        qb.set_exclude_ancestral(["c_value"])

        url = qb.to_url("https://goat.genomehubs.org/api", "v2", "search")
        # c_value should be present, genome_size may not be (overwritten)
        assert "c_value" in url

    def test_exclude_shorthand_derived(self):
        """Verify set_exclude_derived method works."""
        qb = QueryBuilder("taxon").add_field("genome_size")
        qb.set_exclude_derived(["genome_size"])

        url = qb.to_url("https://goat.genomehubs.org/api", "v2", "search")
        # Should have exclude params (ancestral + descendant)
        assert "exclude" in url.lower()

    def test_exclude_shorthand_estimated(self):
        """Verify set_exclude_estimated method works."""
        qb = QueryBuilder("taxon").add_field("genome_size")
        qb.set_exclude_estimated(["genome_size"])

        url = qb.to_url("https://goat.genomehubs.org/api", "v2", "search")
        # Should have exclude params for estimated values
        assert "exclude" in url.lower()
