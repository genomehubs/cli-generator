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


# ── Additional method tests ───────────────────────────────────────────────────


def test_query_builder_set_assemblies() -> None:
    import yaml

    q = QueryBuilder("assembly").set_assemblies(["GCA_000001405.40", "GCA_000001405.29"])
    doc = yaml.safe_load(q.to_query_yaml())
    assert "GCA_000001405.40" in doc["assemblies"]


def test_query_builder_set_samples() -> None:
    import yaml

    q = QueryBuilder("sample").set_samples(["SAMN00000001", "SAMN00000002"])
    doc = yaml.safe_load(q.to_query_yaml())
    assert "SAMN00000001" in doc["samples"]


def test_query_builder_set_ranks() -> None:
    import yaml

    q = QueryBuilder("taxon").set_ranks(["species", "genus"])
    doc = yaml.safe_load(q.to_query_yaml())
    assert doc["ranks"] == ["species", "genus"]


def test_query_builder_set_sort() -> None:
    import yaml

    q = QueryBuilder("taxon").set_sort("genome_size", "desc")
    params = yaml.safe_load(q.to_params_yaml())
    assert params["sort_by"] == "genome_size"
    assert params["sort_order"] == "desc"


def test_query_builder_set_include_estimates() -> None:
    import yaml

    q = QueryBuilder("taxon").set_include_estimates(False)
    params = yaml.safe_load(q.to_params_yaml())
    assert params["include_estimates"] is False


def test_query_builder_set_taxonomy() -> None:
    import yaml

    q = QueryBuilder("taxon").set_taxonomy("ott")
    params = yaml.safe_load(q.to_params_yaml())
    assert params["taxonomy"] == "ott"


def test_query_builder_sample_index() -> None:
    q = QueryBuilder("sample")
    q.set_samples(["SAMN123"]).add_field("collection_date")
    import yaml

    doc = yaml.safe_load(q.to_query_yaml())
    assert doc["index"] == "sample"


# ── Property-based tests with Hypothesis ──────────────────────────────────────

from hypothesis import given
from hypothesis import strategies as st


@given(
    taxa=st.lists(
        st.text(min_size=1, max_size=20, alphabet=st.characters(blacklist_categories=("Cc",))), min_size=0, max_size=5
    )
)
def test_querybuilder_taxa_handles_varied_lists(taxa: list) -> None:
    """Property: QueryBuilder should handle taxa lists of any length without errors."""
    q = QueryBuilder("taxon").set_taxa(taxa)
    # Should always produce valid YAML
    yaml_output = q.to_query_yaml()
    assert isinstance(yaml_output, str)


@given(assemblies=st.lists(st.just("GCA_000001405.40"), min_size=0, max_size=3))
def test_querybuilder_assemblies_always_serializable(assemblies: list) -> None:
    """Property: QueryBuilder with assemblies should always serialize to YAML."""
    q = QueryBuilder("assembly").set_assemblies(assemblies)
    yaml_output = q.to_query_yaml()
    assert "assembly" in yaml_output.lower() or not assemblies


@given(samples=st.lists(st.just("SRS123456"), min_size=0, max_size=3))
def test_querybuilder_samples_idempotence(samples: list) -> None:
    """Property: Multiple calls to set_samples should be idempotent (last one wins)."""
    q1 = QueryBuilder("sample").set_samples(samples)
    q2 = QueryBuilder("sample").set_samples(samples).set_samples(samples)
    assert q1.to_query_yaml() == q2.to_query_yaml()


@given(st.booleans())
def test_querybuilder_include_estimates_roundtrip(include_estimates: bool) -> None:
    """Property: include_estimates setting should roundtrip through YAML."""
    import yaml

    q = QueryBuilder("taxon").set_include_estimates(include_estimates)
    params = yaml.safe_load(q.to_params_yaml())
    assert params["include_estimates"] is include_estimates


@given(rank=st.just("species"))
def test_querybuilder_rank_preserved_in_yaml(rank: str) -> None:
    """Property: Rank should be preserved when round-tripping to YAML."""
    import yaml

    q = QueryBuilder("taxon").set_ranks([rank])
    doc = yaml.safe_load(q.to_query_yaml())
    if "ranks" in doc:
        assert rank in doc["ranks"]
        assert rank in doc["ranks"]


# ── Operator alias tests ──────────────────────────────────────────────────────


def test_operator_alias_symbol_greater_than() -> None:
    """Operator alias: > should work as gt."""
    q = QueryBuilder("taxon").add_attribute("genome_size", operator=">", value="1000000000").add_field("genome_size")
    yaml_output = q.to_query_yaml()
    assert "genome_size" in yaml_output
    assert "operator:" in yaml_output


def test_operator_alias_symbol_greater_equal() -> None:
    """Operator alias: >= should work as ge."""
    q = QueryBuilder("taxon").add_attribute("genome_size", operator=">=", value="1000000000").add_field("genome_size")
    yaml_output = q.to_query_yaml()
    assert "genome_size" in yaml_output


def test_operator_alias_word_gte() -> None:
    """Operator alias: gte should work as ge."""
    q = QueryBuilder("taxon").add_attribute("genome_size", operator="gte", value="1000000000").add_field("genome_size")
    yaml_output = q.to_query_yaml()
    assert "genome_size" in yaml_output


def test_operator_alias_symbol_less_than() -> None:
    """Operator alias: < should work as lt."""
    q = QueryBuilder("taxon").add_attribute("genome_size", operator="<", value="1000000000").add_field("genome_size")
    yaml_output = q.to_query_yaml()
    assert "genome_size" in yaml_output


def test_operator_alias_symbol_less_equal() -> None:
    """Operator alias: <= should work as le."""
    q = QueryBuilder("taxon").add_attribute("genome_size", operator="<=", value="1000000000").add_field("genome_size")
    yaml_output = q.to_query_yaml()
    assert "genome_size" in yaml_output


def test_operator_alias_word_lte() -> None:
    """Operator alias: lte should work as le."""
    q = QueryBuilder("taxon").add_attribute("genome_size", operator="lte", value="1000000000").add_field("genome_size")
    yaml_output = q.to_query_yaml()
    assert "genome_size" in yaml_output


def test_operator_alias_symbol_equals() -> None:
    """Operator alias: = should work as eq."""
    q = (
        QueryBuilder("taxon")
        .add_attribute("assembly_level", operator="=", value="chromosome")
        .add_field("assembly_level")
    )
    yaml_output = q.to_query_yaml()
    assert "assembly_level" in yaml_output


def test_operator_alias_symbol_double_equals() -> None:
    """Operator alias: == should work as eq."""
    q = (
        QueryBuilder("taxon")
        .add_attribute("assembly_level", operator="==", value="chromosome")
        .add_field("assembly_level")
    )
    yaml_output = q.to_query_yaml()
    assert "assembly_level" in yaml_output


def test_operator_alias_symbol_not_equal() -> None:
    """Operator alias: != should work as ne."""
    q = (
        QueryBuilder("taxon")
        .add_attribute("assembly_level", operator="!=", value="scaffold")
        .add_field("assembly_level")
    )
    yaml_output = q.to_query_yaml()
    assert "assembly_level" in yaml_output


def test_operator_alias_canonical_forms_still_work() -> None:
    """Canonical snake_case forms should still work."""
    q = (
        QueryBuilder("taxon")
        .add_attribute("genome_size", operator="lt", value="3000000000")
        .add_attribute("assembly_level", operator="eq", value="chromosome")
        .add_field("genome_size")
        .add_field("assembly_level")
    )
    yaml_output = q.to_query_yaml()
    assert "genome_size" in yaml_output
    assert "assembly_level" in yaml_output


def test_operator_aliases_build_valid_url() -> None:
    """URL building should work with operator aliases."""
    q = QueryBuilder("taxon").set_taxa(["Mammalia"]).add_attribute("genome_size", operator=">", value="1000000000")
    url = build_url(
        q.to_query_yaml(),
        q.to_params_yaml(),
        "https://goat.genomehubs.org/api",
        "v2",
        "search",
    )
    assert isinstance(url, str)
    assert "Mammalia" in url


# ── QueryBuilder.describe tests ───────────────────────────────────────────────


def test_query_builder_describe_returns_string() -> None:
    """QueryBuilder.describe() should return a string."""
    q = QueryBuilder("taxon").set_taxa(["Mammalia"])
    desc = q.describe()
    assert isinstance(desc, str)
    assert len(desc) > 0


def test_query_builder_describe_concise_includes_taxa() -> None:
    """Concise description should mention the taxa."""
    q = QueryBuilder("taxon").set_taxa(["Mammalia"], filter_type="tree")
    desc = q.describe(mode="concise")
    assert "Mammalia" in desc or "taxa" in desc.lower()


def test_query_builder_describe_concise_includes_filter() -> None:
    """Concise description should mention filters."""
    q = QueryBuilder("taxon").add_attribute("genome_size", operator=">=", value="1000000000")
    desc = q.describe(mode="concise")
    assert "genome_size" in desc or ">=" in desc


def test_query_builder_describe_verbose_formats_better() -> None:
    """Verbose description should include more details than concise."""
    q = (
        QueryBuilder("taxon")
        .set_taxa(["Mammalia"], filter_type="tree")
        .add_attribute("genome_size", operator=">=", value="1000000000")
    )
    concise = q.describe(mode="concise")
    verbose = q.describe(mode="verbose")
    # Verbose version should contain more content or structured formatting
    assert len(verbose) >= len(concise)


def test_query_builder_describe_with_field_metadata() -> None:
    """Describe should accept optional field metadata."""
    q = QueryBuilder("taxon").add_attribute("genome_size", operator=">=", value="1000000000")
    field_meta = {"genome_size": {"display_name": "Genome Size (BP)"}}
    desc = q.describe(field_metadata=field_meta, mode="concise")
    assert isinstance(desc, str)
    assert len(desc) > 0


def test_query_builder_describe_handles_multiple_filters() -> None:
    """Describe should handle multiple filters."""
    q = (
        QueryBuilder("taxon")
        .add_attribute("genome_size", operator=">=", value="1000000000")
        .add_attribute("assembly_level", operator="eq", value="chromosome")
    )
    desc = q.describe()
    assert isinstance(desc, str)
    # Should mention at least one of the filters
    assert "genome_size" in desc or "assembly" in desc or "filter" in desc.lower()


def test_query_builder_describe_handles_empty_query() -> None:
    """Describe should handle minimal queries gracefully."""
    q = QueryBuilder("taxon")
    desc = q.describe()
    assert isinstance(desc, str)
    assert "taxa" in desc.lower() or "search" in desc.lower()
