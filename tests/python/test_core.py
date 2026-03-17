"""Tests for the cli_generator Python extension.

Covers:
- Unit tests  — verify the `version()` function returns a well-formed string.
- Property tests — invariants that must hold for any version string.
- Smoke tests — `build_url` and `QueryBuilder` round-trip.
"""

import re

from cli_generator import QueryBuilder, build_url, version

# ── Unit tests ────────────────────────────────────────────────────────────────


def test_version_returns_a_string() -> None:
    assert isinstance(version(), str)


def test_version_is_non_empty() -> None:
    assert len(version()) > 0


def test_version_matches_semver_pattern() -> None:
    # Accepts "MAJOR.MINOR.PATCH" with an optional pre-release suffix.
    pattern = re.compile(r"^\d+\.\d+\.\d+")
    assert pattern.match(version()), f"Unexpected version string: {version()!r}"


def test_version_is_stable_across_calls() -> None:
    assert version() == version()


# ── build_url smoke tests ─────────────────────────────────────────────────────


def test_build_url_returns_string() -> None:
    url = build_url(
        "index: taxon\ntaxa: [Mammalia]\n",
        "size: 10\npage: 1\n",
        "https://goat.genomehubs.org/api",
        "v2",
        "search",
    )
    assert isinstance(url, str)


def test_build_url_contains_api_base() -> None:
    url = build_url(
        "index: taxon\ntaxa: [Mammalia]\n",
        "size: 10\npage: 1\n",
        "https://goat.genomehubs.org/api",
        "v2",
        "search",
    )
    assert url.startswith("https://goat.genomehubs.org/api/v2/search")


def test_build_url_raises_on_bad_yaml() -> None:
    import pytest

    with pytest.raises(ValueError):
        build_url("index: [invalid: yaml: {", "", "https://example.com", "v2", "search")


# ── QueryBuilder smoke tests ─────────────────────────────────────────────────


def test_query_builder_produces_valid_yaml() -> None:
    import yaml

    q = QueryBuilder("taxon")
    q.set_taxa(["Mammalia"], filter_type="tree").set_rank("species")
    doc = yaml.safe_load(q.to_query_yaml())
    assert doc["index"] == "taxon"
    assert "Mammalia" in doc["taxa"]
    assert doc["rank"] == "species"


def test_query_builder_chaining_returns_self() -> None:
    q = QueryBuilder("assembly")
    result = q.set_taxa(["Homo sapiens"]).set_size(50).set_page(2)
    assert result is q


def test_query_builder_build_url_integration() -> None:
    q = (
        QueryBuilder("taxon")
        .set_taxa(["Insecta"], filter_type="tree")
        .add_attribute("genome_size", operator="lt", value="1000000000")
        .add_field("genome_size")
        .set_names(["scientific_name"])
    )
    url = build_url(
        q.to_query_yaml(),
        q.to_params_yaml(),
        "https://goat.genomehubs.org/api",
        "v2",
        "search",
    )
    assert "result=taxon" in url
    assert "tax_tree" in url
    assert "genome_size" in url
    assert "scientific_name" in url


def test_query_builder_reset_clears_taxa() -> None:
    q = QueryBuilder("taxon")
    q.set_taxa(["Mammalia"])
    q.reset()
    import yaml

    doc = yaml.safe_load(q.to_query_yaml())
    assert "taxa" not in doc


# ── QueryBuilder.merge / combine tests ───────────────────────────────────────


def test_merge_combines_parallel_builders() -> None:
    """Identifiers and attributes built in parallel can be merged."""
    import yaml

    id_builder = QueryBuilder("taxon").set_taxa(["Mammalia"], filter_type="tree").set_rank("species")
    attr_builder = (
        QueryBuilder("taxon")
        .add_attribute("genome_size", operator="lt", value="3000000000")
        .add_field("genome_size")
        .set_names(["scientific_name"])
    )

    q = QueryBuilder.combine(id_builder, attr_builder)
    doc = yaml.safe_load(q.to_query_yaml())

    assert doc["taxa"] == ["Mammalia"]
    assert doc["rank"] == "species"
    assert doc["attributes"][0]["name"] == "genome_size"
    assert doc["fields"][0]["name"] == "genome_size"
    assert doc["names"] == ["scientific_name"]


def test_merge_raises_on_index_mismatch() -> None:
    import pytest

    a = QueryBuilder("taxon").set_taxa(["Mammalia"])
    b = QueryBuilder("assembly").add_field("contig_n50")
    with pytest.raises(ValueError, match="different indexes"):
        a.merge(b)


def test_combine_requires_at_least_one_builder() -> None:
    import pytest

    with pytest.raises(ValueError):
        QueryBuilder.combine()


def test_merge_scalar_default_not_overwritten() -> None:
    """A builder with default size=10 should not overwrite a custom size."""
    base = QueryBuilder("taxon").set_size(50)
    other = QueryBuilder("taxon")  # default size=10
    base.merge(other)
    import yaml

    params = yaml.safe_load(base.to_params_yaml())
    assert params["size"] == 50
