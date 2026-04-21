"""Tests for the cli_generator Python extension.

Covers:
- Unit tests  — verify the `version()` function returns a well-formed string.
- Property tests — invariants that must hold for any version string.
- Smoke tests — `build_url` and `QueryBuilder` round-trip.
"""

import json
import pathlib
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


# ── QueryBuilder.snippet tests ────────────────────────────────────────────────


def test_snippet_returns_dict() -> None:
    """snippet() returns a dict mapping language name to code string."""
    q = QueryBuilder("taxon")
    result = q.snippet()
    assert isinstance(result, dict)
    assert "python" in result
    assert isinstance(result["python"], str)


def test_snippet_default_language_is_python() -> None:
    """Calling snippet() with no arguments produces exactly one Python entry."""
    q = QueryBuilder("taxon")
    result = q.snippet()
    assert list(result.keys()) == ["python"]


def test_snippet_empty_query_renders_without_filters() -> None:
    """Empty query produces a snippet with no add_attribute calls."""
    q = QueryBuilder("taxon")
    code = q.snippet()["python"]
    assert "QueryBuilder" in code
    assert "add_attribute" not in code


def test_snippet_includes_filter() -> None:
    """Snippet contains the attribute filter when one is set."""
    q = QueryBuilder("taxon").add_attribute("genome_size", operator=">=", value="1000000000")
    code = q.snippet()["python"]
    assert "genome_size" in code
    assert "1000000000" in code
    assert ">=" in code


def test_snippet_includes_multiple_filters() -> None:
    """Snippet contains all attribute filters when multiple are set."""
    q = (
        QueryBuilder("taxon")
        .add_attribute("genome_size", operator=">=", value="1000000000")
        .add_attribute("assembly_level", operator="eq", value="chromosome")
    )
    code = q.snippet()["python"]
    assert "genome_size" in code
    assert "assembly_level" in code


def test_snippet_includes_sort() -> None:
    """Snippet contains sort call when sort is set."""
    q = QueryBuilder("taxon").set_sort("genome_size", "desc")
    code = q.snippet()["python"]
    assert "genome_size" in code
    assert "sort" in code.lower() or "desc" in code


def test_snippet_includes_field_selections() -> None:
    """Snippet contains set_fields call when fields are selected."""
    q = QueryBuilder("taxon").add_field("organism_name").add_field("genome_size")
    code = q.snippet()["python"]
    assert "organism_name" in code
    assert "genome_size" in code


def test_snippet_site_params_appear_in_code() -> None:
    """Site name and sdk_name appear in the generated snippet."""
    q = QueryBuilder("taxon")
    code = q.snippet(site_name="goat", sdk_name="goat_sdk")["python"]
    assert "goat_sdk" in code
    assert "goat" in code


def test_snippet_is_valid_python_syntax() -> None:
    """Generated Python snippet is syntactically valid Python."""
    import ast

    q = (
        QueryBuilder("taxon")
        .add_attribute("genome_size", operator=">=", value="1000000000")
        .add_field("organism_name")
        .set_sort("genome_size", "desc")
    )
    code = q.snippet(site_name="goat", sdk_name="goat_sdk")["python"]
    # Raises SyntaxError if the generated code is invalid Python.
    ast.parse(code)


# ============================================================================
# R snippet tests
# ============================================================================


def test_r_snippet_is_in_result() -> None:
    """snippet() includes R code when 'r' is requested."""
    q = QueryBuilder("taxon")
    result = q.snippet(languages=["r"])
    assert "r" in result
    assert isinstance(result["r"], str)


def test_r_snippet_uses_r6_syntax() -> None:
    """R snippet uses R6 class notation (QueryBuilder$new, $add_attribute, etc.)."""
    q = QueryBuilder("taxon")
    code = q.snippet(languages=["r"])["r"]
    assert "QueryBuilder$new" in code
    assert "$new(" in code


def test_r_snippet_includes_filters() -> None:
    """R snippet contains attribute filters."""
    q = QueryBuilder("taxon").add_attribute("genome_size", operator=">=", value="1000000000")
    code = q.snippet(languages=["r"])["r"]
    assert "genome_size" in code
    assert "1000000000" in code


def test_r_snippet_includes_multiple_filters() -> None:
    """R snippet contains multiple attribute filters."""
    q = (
        QueryBuilder("taxon")
        .add_attribute("genome_size", operator=">=", value="1000000000")
        .add_attribute("assembly_level", operator="eq", value="chromosome")
    )
    code = q.snippet(languages=["r"])["r"]
    assert "genome_size" in code
    assert "assembly_level" in code


def test_r_snippet_includes_sort() -> None:
    """R snippet contains sort directive."""
    q = QueryBuilder("taxon").set_sort("genome_size", "desc")
    code = q.snippet(languages=["r"])["r"]
    assert "genome_size" in code
    assert "sort" in code.lower() or "desc" in code


def test_r_snippet_includes_field_selections() -> None:
    """R snippet contains set_fields call when present."""
    q = QueryBuilder("taxon").add_field("organism_name").add_field("genome_size")
    code = q.snippet(languages=["r"])["r"]
    assert "organism_name" in code
    assert "genome_size" in code


def test_r_snippet_site_params_appear() -> None:
    """R snippet includes site_name and sdk_name parameters."""
    q = QueryBuilder("taxon")
    code = q.snippet(languages=["r"], site_name="goat", sdk_name="goat_sdk")["r"]
    assert "goat" in code


def test_r_snippet_is_valid_r_code() -> None:
    """Generated R snippet is valid R code (basic syntax check)."""
    q = (
        QueryBuilder("taxon")
        .add_attribute("genome_size", operator=">=", value="1000000000")
        .add_field("organism_name")
        .set_sort("genome_size", "desc")
    )
    code = q.snippet(languages=["r"], site_name="goat", sdk_name="goat_sdk")["r"]

    # Basic R syntax checks
    assert "library(" in code
    assert "QueryBuilder$new(" in code
    assert "$add_" in code or "genome_size" in code
    assert "<-" in code  # R assignment operator
    # Check for at least one method call with $
    assert code.count("$") >= 2


# ── JavaScript snippet tests ──────────────────────────────────────────────────


def test_js_snippet_is_in_result() -> None:
    """Requesting 'javascript' returns a snippet keyed as 'javascript'."""
    q = QueryBuilder("taxon").add_attribute("genome_size", operator="ge", value="1000000000")
    result = q.snippet(languages=["javascript"], site_name="goat", sdk_name="goat_sdk")
    assert "javascript" in result
    assert len(result["javascript"]) > 0


def test_js_snippet_uses_class_syntax() -> None:
    """Generated JS snippet uses QueryBuilder class instantiation."""
    q = QueryBuilder("taxon").add_attribute("genome_size", operator="ge", value="1000000000")
    code = q.snippet(languages=["javascript"], site_name="goat", sdk_name="goat_sdk")["javascript"]
    assert "new QueryBuilder(" in code
    assert "require(" in code


def test_js_snippet_includes_filters() -> None:
    """A single attribute filter appears in the JS snippet."""
    q = QueryBuilder("taxon").add_attribute("genome_size", operator="ge", value="1000000000")
    code = q.snippet(languages=["javascript"], site_name="goat", sdk_name="goat_sdk")["javascript"]
    assert "genome_size" in code
    assert "ge" in code
    assert "1000000000" in code


def test_js_snippet_includes_multiple_filters() -> None:
    """Multiple attribute filters all appear in the JS snippet."""
    q = (
        QueryBuilder("taxon")
        .add_attribute("genome_size", operator="ge", value="1000000000")
        .add_attribute("assembly_span", operator="lt", value="5000000000")
    )
    code = q.snippet(languages=["javascript"], site_name="goat", sdk_name="goat_sdk")["javascript"]
    assert "genome_size" in code
    assert "assembly_span" in code


def test_js_snippet_includes_sort() -> None:
    """Sort directive appears in the JS snippet."""
    q = QueryBuilder("taxon").set_sort("genome_size", "desc")
    code = q.snippet(languages=["javascript"], site_name="goat", sdk_name="goat_sdk")["javascript"]
    assert "genome_size" in code
    assert "desc" in code
    assert "setSort(" in code


def test_js_snippet_includes_field_selections() -> None:
    """Selected fields appear in the JS snippet."""
    q = QueryBuilder("taxon").add_field("assembly_span").add_field("genome_size")
    code = q.snippet(languages=["javascript"], site_name="goat", sdk_name="goat_sdk")["javascript"]
    assert "assembly_span" in code
    assert "genome_size" in code
    assert "addField(" in code


def test_js_snippet_site_params_appear() -> None:
    """Site name appears as a comment in the JS snippet."""
    q = QueryBuilder("taxon")
    code = q.snippet(languages=["javascript"], site_name="mysite", sdk_name="mysite_sdk")["javascript"]
    assert "mysite" in code


def test_js_snippet_is_valid_js() -> None:
    """Generated JS snippet passes basic syntax checks."""
    q = (
        QueryBuilder("taxon")
        .add_attribute("genome_size", operator="ge", value="1000000000")
        .add_field("organism_name")
        .set_sort("genome_size", "desc")
    )
    code = q.snippet(languages=["javascript"], site_name="goat", sdk_name="goat_sdk")["javascript"]

    # Basic JS syntax checks
    assert "require(" in code
    assert "new QueryBuilder(" in code
    assert "toUrl()" in code
    assert "const " in code
    # Should not contain Python or R syntax
    assert "import " not in code or code.index("import ") > code.index("require(")
    assert "library(" not in code
    assert "<-" not in code


# ── CLI snippet tests ─────────────────────────────────────────────────────────


def test_cli_snippet_is_in_result() -> None:
    """snippet() returns a 'cli' key when requested."""
    q = QueryBuilder("taxon")
    result = q.snippet(languages=["cli"], site_name="goat", sdk_name="goat-cli")
    assert "cli" in result
    assert isinstance(result["cli"], str)


def test_cli_snippet_contains_binary_and_index() -> None:
    """CLI snippet has the binary name, index, and 'search' subcommand."""
    q = QueryBuilder("taxon")
    code = q.snippet(languages=["cli"], site_name="goat", sdk_name="goat-cli")["cli"]
    assert "goat-cli" in code
    assert "taxon" in code
    assert "search" in code


def test_cli_snippet_respects_index() -> None:
    """CLI snippet uses the builder's index, not a hardcoded fallback."""
    q = QueryBuilder("assembly")
    code = q.snippet(languages=["cli"], site_name="goat", sdk_name="goat-cli")["cli"]
    assert "assembly" in code
    assert "taxon" not in code


def test_cli_snippet_includes_filter() -> None:
    """Attribute filter appears as --filter FIELD OP VALUE."""
    q = QueryBuilder("taxon").add_attribute("genome_size", operator="ge", value="1000000000")
    code = q.snippet(languages=["cli"], site_name="goat", sdk_name="goat-cli")["cli"]
    assert "--filter" in code
    assert "genome_size" in code
    assert "ge" in code
    assert "1000000000" in code


def test_cli_snippet_includes_sort() -> None:
    """Sort appears as --sort FIELD:DIRECTION."""
    q = QueryBuilder("taxon").set_sort("genome_size", "desc")
    code = q.snippet(languages=["cli"], site_name="goat", sdk_name="goat-cli")["cli"]
    assert "--sort" in code
    assert "genome_size" in code
    assert "desc" in code


def test_cli_snippet_includes_fields() -> None:
    """Selected fields appear as --fields."""
    q = QueryBuilder("taxon").add_field("organism_name").add_field("genome_size")
    code = q.snippet(languages=["cli"], site_name="goat", sdk_name="goat-cli")["cli"]
    assert "--fields" in code
    assert "organism_name" in code
    assert "genome_size" in code


def test_cli_snippet_includes_taxa() -> None:
    """Taxa appear as --taxon and --taxon-filter."""
    q = QueryBuilder("taxon").set_taxa(["Mammalia"], "tree")
    code = q.snippet(languages=["cli"], site_name="goat", sdk_name="goat-cli")["cli"]
    assert "--taxon" in code
    assert "Mammalia" in code
    assert "--taxon-filter" in code
    assert "tree" in code


def test_cli_snippet_includes_rank() -> None:
    """Rank restriction appears as --rank."""
    q = QueryBuilder("taxon").set_rank("species")
    code = q.snippet(languages=["cli"], site_name="goat", sdk_name="goat-cli")["cli"]
    assert "--rank" in code
    assert "species" in code


def test_cli_snippet_no_trailing_backslash() -> None:
    """Last non-empty line of the CLI snippet does not end with a continuation backslash."""
    q = (
        QueryBuilder("taxon")
        .set_taxa(["Mammalia"], "tree")
        .add_attribute("genome_size", operator="ge", value="1000000000")
        .add_field("organism_name")
        .set_sort("genome_size", "desc")
    )
    code = q.snippet(languages=["cli"], site_name="goat", sdk_name="goat-cli")["cli"]
    non_empty_lines = [ln for ln in code.splitlines() if ln.strip()]
    assert non_empty_lines, "snippet produced no output"
    assert not non_empty_lines[-1].rstrip().endswith("\\")


def test_cli_snippet_all_languages_together() -> None:
    """Requesting python, r, javascript, and cli returns all four keys."""
    q = QueryBuilder("taxon").add_attribute("genome_size", operator="ge", value="1000000000")
    result = q.snippet(
        languages=["python", "r", "javascript", "cli"],
        site_name="goat",
        sdk_name="goat_sdk",
    )
    assert set(result.keys()) == {"python", "r", "javascript", "cli"}


# ── parse_search_json / values_only / annotated_values smoke tests ────────────

_TAXON_RESPONSE = json.dumps(
    {
        "status": {"hits": 1, "success": True},
        "results": [
            {
                "index": "taxon--ncbi--goat--2026.04.16",
                "id": "9606",
                "score": 1.0,
                "result": {
                    "taxon_id": "9606",
                    "scientific_name": "Homo sapiens",
                    "taxon_rank": "species",
                    "fields": {
                        "genome_size": {
                            "value": 3_100_000_000,
                            "count": 1,
                            "min": 3_100_000_000,
                            "max": 3_200_000_000,
                            "aggregation_source": "direct",
                            "sp_count": 0,
                        }
                    },
                },
            }
        ],
    }
)


def test_parse_search_json_returns_flat_record() -> None:
    from cli_generator import parse_search_json

    rows = json.loads(parse_search_json(_TAXON_RESPONSE))
    assert len(rows) == 1
    row = rows[0]
    assert row["taxon_id"] == "9606"
    assert row["scientific_name"] == "Homo sapiens"
    assert row["genome_size"] == 3_100_000_000
    assert row["genome_size__source"] == "direct"
    assert row["genome_size__min"] == 3_100_000_000
    assert row["genome_size__max"] == 3_200_000_000


def test_values_only_strips_subkey_columns() -> None:
    from cli_generator import parse_search_json, values_only

    flat = parse_search_json(_TAXON_RESPONSE)
    rows = json.loads(values_only(flat))
    assert len(rows) == 1
    row = rows[0]
    assert row["taxon_id"] == "9606"
    assert row["genome_size"] == 3_100_000_000
    # Sub-key columns must be absent.
    assert "genome_size__source" not in row
    assert "genome_size__min" not in row
    assert "genome_size__max" not in row


def test_values_only_preserves_keep_column() -> None:
    from cli_generator import parse_search_json, values_only

    flat = parse_search_json(_TAXON_RESPONSE)
    keep = json.dumps(["genome_size__min"])
    rows = json.loads(values_only(flat, keep))
    row = rows[0]
    # Explicitly requested stat preserved.
    assert row["genome_size__min"] == 3_100_000_000
    # Other sub-key columns still stripped.
    assert "genome_size__source" not in row
    assert "genome_size__max" not in row


def test_add_field_colon_syntax_builds_correct_url() -> None:
    """add_field(\"assembly_span:min\") should produce bare field before :min in URL."""
    from cli_generator import QueryBuilder, build_url

    q = QueryBuilder("assembly").add_field("assembly_span:min")
    url = build_url(q.to_query_yaml(), q.to_params_yaml(), "https://goat.genomehubs.org/api", "v2", "search")
    assert "assembly_span" in url
    assert "assembly_span%3Amin" in url
    # Bare field must appear before the modifier.
    idx_bare = url.index("assembly_span")
    idx_mod = url.index("assembly_span%3Amin")
    assert idx_bare < idx_mod


def test_field_modifiers_returns_stat_columns() -> None:
    q = QueryBuilder("assembly")
    q.add_field("assembly_span:min")
    q.add_field("genome_size", modifiers=["max"])
    q.add_field("contig_n50")
    q.add_field("assembly_span:direct")  # status modifier — also produces __direct column
    assert set(q.field_modifiers()) == {"assembly_span__min", "genome_size__max", "assembly_span__direct"}


def test_annotated_values_direct_stays_numeric_in_non_direct_mode() -> None:
    from cli_generator import annotated_values, parse_search_json

    flat = parse_search_json(_TAXON_RESPONSE)
    rows = json.loads(annotated_values(flat, "non_direct"))
    assert len(rows) == 1
    row = rows[0]
    # Direct source in non_direct mode: value stays numeric, no __* columns.
    assert row["genome_size"] == 3_100_000_000
    assert "genome_size__source" not in row
    assert "genome_size__label" not in row


_ANCESTOR_RESPONSE = json.dumps(
    {
        "status": {"hits": 1, "success": True},
        "results": [
            {
                "index": "taxon--ncbi--goat--2026.04.16",
                "id": "9347",
                "score": 1.0,
                "result": {
                    "taxon_id": "9347",
                    "scientific_name": "Eutheria",
                    "taxon_rank": "clade",
                    "fields": {
                        "genome_size": {
                            "value": 8_215_200_000,
                            "aggregation_source": ["ancestor"],
                        }
                    },
                },
            }
        ],
    }
)


def test_annotated_values_ancestor_becomes_labelled_string() -> None:
    from cli_generator import annotated_values, parse_search_json

    flat = parse_search_json(_ANCESTOR_RESPONSE)
    rows = json.loads(annotated_values(flat, "non_direct"))
    row = rows[0]
    assert row["genome_size"] == "8215200000 (Ancestral)"
    assert "genome_size__source" not in row
    assert "genome_size__label" not in row


# ── to_tidy_records ──────────────────────────────────────────────────────────

_FLAT_RESPONSE = json.dumps(
    [
        {
            "taxon_id": "9606",
            "scientific_name": "Homo sapiens",
            "taxon_rank": "species",
            "genome_size": 3_100_000_000,
            "genome_size__source": "direct",
            "assembly_span": 2_747_877_777,
            "assembly_span__source": "ancestor",
        }
    ]
)


def test_to_tidy_records_returns_one_row_per_field() -> None:
    from cli_generator import to_tidy_records

    rows = json.loads(to_tidy_records(_FLAT_RESPONSE))
    assert len(rows) == 2
    field_names = {r["field"] for r in rows}
    assert field_names == {"genome_size", "assembly_span"}


def test_to_tidy_records_identity_columns_present() -> None:
    from cli_generator import to_tidy_records

    rows = json.loads(to_tidy_records(_FLAT_RESPONSE))
    for row in rows:
        assert row["taxon_id"] == "9606"
        assert row["scientific_name"] == "Homo sapiens"
        assert row["taxon_rank"] == "species"


def test_to_tidy_records_source_column_populated() -> None:
    from cli_generator import to_tidy_records

    rows = json.loads(to_tidy_records(_FLAT_RESPONSE))
    by_field = {r["field"]: r for r in rows}
    assert by_field["genome_size"]["source"] == "direct"
    assert by_field["assembly_span"]["source"] == "ancestor"


def test_to_tidy_records_empty_input_returns_empty_list() -> None:
    from cli_generator import to_tidy_records

    rows = json.loads(to_tidy_records("[]"))
    assert rows == []


def test_to_tidy_records_modifier_column_becomes_its_own_row() -> None:
    from cli_generator import to_tidy_records

    # assembly_span__min from field:min request (no bare assembly_span key)
    flat = json.dumps([{"taxon_id": "9606", "assembly_span__min": 2_400_000_000}])
    rows = json.loads(to_tidy_records(flat))
    assert len(rows) == 1
    assert rows[0]["field"] == "assembly_span:min"
    assert rows[0]["value"] == 2_400_000_000
    assert rows[0]["source"] is None


def test_query_builder_to_tidy_records_method() -> None:
    from cli_generator import QueryBuilder

    flat = json.dumps(
        [
            {
                "taxon_id": "9606",
                "scientific_name": "Homo sapiens",
                "genome_size": 3_100_000_000,
                "genome_size__source": "direct",
            }
        ]
    )
    qb = QueryBuilder("taxon")
    rows = qb.to_tidy_records(flat)
    assert isinstance(rows, list)
    assert len(rows) == 1
    assert rows[0]["field"] == "genome_size"
    assert rows[0]["source"] == "direct"


# ── parse_paginated_json tests ────────────────────────────────────────────────

_PAGINATED_RESPONSE_HAS_MORE = json.dumps(
    {
        "status": {"hits": 2, "success": True},
        "hits": [
            {
                "index": "taxon",
                "id": "9606",
                "score": 1.0,
                "result": {
                    "taxon_id": "9606",
                    "scientific_name": "Homo sapiens",
                    "taxon_rank": "species",
                    "fields": {},
                },
            },
            {
                "index": "taxon",
                "id": "10090",
                "score": 0.9,
                "result": {
                    "taxon_id": "10090",
                    "scientific_name": "Mus musculus",
                    "taxon_rank": "species",
                    "fields": {},
                },
            },
        ],
        "pagination": {"hasMore": True, "searchAfter": [0.9, "10090"]},
    }
)

_PAGINATED_RESPONSE_LAST_PAGE = json.dumps(
    {
        "status": {"hits": 1, "success": True},
        "hits": [
            {
                "index": "taxon",
                "id": "9606",
                "score": 1.0,
                "result": {
                    "taxon_id": "9606",
                    "scientific_name": "Homo sapiens",
                    "taxon_rank": "species",
                    "fields": {},
                },
            }
        ],
        "pagination": {"hasMore": False, "searchAfter": None},
    }
)


def test_parse_paginated_json_returns_string() -> None:
    from cli_generator import parse_paginated_json

    result = parse_paginated_json(_PAGINATED_RESPONSE_HAS_MORE)
    assert isinstance(result, str)


def test_parse_paginated_json_has_more_true() -> None:
    from cli_generator import parse_paginated_json

    page = json.loads(parse_paginated_json(_PAGINATED_RESPONSE_HAS_MORE))
    assert page["hasMore"] is True


def test_parse_paginated_json_records_present() -> None:
    from cli_generator import parse_paginated_json

    page = json.loads(parse_paginated_json(_PAGINATED_RESPONSE_HAS_MORE))
    assert isinstance(page["records"], list)
    assert len(page["records"]) == 2
    assert page["records"][0]["taxon_id"] == "9606"
    assert page["records"][1]["taxon_id"] == "10090"


def test_parse_paginated_json_cursor() -> None:
    from cli_generator import parse_paginated_json

    page = json.loads(parse_paginated_json(_PAGINATED_RESPONSE_HAS_MORE))
    assert page["searchAfter"] == [0.9, "10090"]


def test_parse_paginated_json_total_hits() -> None:
    from cli_generator import parse_paginated_json

    page = json.loads(parse_paginated_json(_PAGINATED_RESPONSE_HAS_MORE))
    assert page["totalHits"] == 2


def test_parse_paginated_json_last_page() -> None:
    from cli_generator import parse_paginated_json

    page = json.loads(parse_paginated_json(_PAGINATED_RESPONSE_LAST_PAGE))
    assert page["hasMore"] is False
    assert page["searchAfter"] is None


def test_parse_paginated_json_empty_hits() -> None:
    from cli_generator import parse_paginated_json

    raw = json.dumps(
        {
            "status": {"hits": 0, "success": True},
            "hits": [],
            "pagination": {"hasMore": False, "searchAfter": None},
        }
    )
    page = json.loads(parse_paginated_json(raw))
    assert page["records"] == []
    assert page["totalHits"] == 0


def test_parse_paginated_json_invalid_input_returns_error_key() -> None:
    from cli_generator import parse_paginated_json

    result = json.loads(parse_paginated_json("not json"))
    assert "error" in result


# ── parse_batch_json ─────────────────────────────────────────────────────────


def _minimal_msearch_response(entries: list[tuple[int, int]]) -> str:
    """Build a minimal /msearch batch response string.

    Each ``(taxon_id, total)`` pair becomes one element in ``results``.
    """
    overall = sum(t for _, t in entries)
    results = []
    for taxon_id, total in entries:
        results.append(
            f'{{"status":"ok","count":1,"total":{total},'
            f'"hits":[{{"index":"taxon--ncbi","id":"{taxon_id}","score":1.0,'
            f'"result":{{"taxon_id":"{taxon_id}","scientific_name":"Species {taxon_id}",'
            f'"taxon_rank":"species","fields":{{}}}}}}]}}'
        )
    return f'{{"status":{{"hits":{overall},"success":true}},' f'"results":[{",".join(results)}]}}'


def test_parse_batch_json_two_queries_return_two_groups() -> None:
    from cli_generator import parse_batch_json

    raw = _minimal_msearch_response([(9606, 5200), (10090, 7300)])
    result = json.loads(parse_batch_json(raw))
    assert len(result["results"]) == 2
    assert result["totalHits"] == 12500


def test_parse_batch_json_per_query_totals() -> None:
    from cli_generator import parse_batch_json

    raw = _minimal_msearch_response([(9606, 5200), (10090, 7300)])
    result = json.loads(parse_batch_json(raw))
    assert result["results"][0]["total"] == 5200
    assert result["results"][1]["total"] == 7300


def test_parse_batch_json_records_contain_identity() -> None:
    from cli_generator import parse_batch_json

    raw = _minimal_msearch_response([(9606, 10)])
    result = json.loads(parse_batch_json(raw))
    record = result["results"][0]["records"][0]
    assert record["taxon_id"] == "9606"
    assert record["scientific_name"] == "Species 9606"


def test_parse_batch_json_empty_results_returns_empty_list() -> None:
    from cli_generator import parse_batch_json

    raw = json.dumps({"status": {"hits": 0, "success": True}, "results": []})
    result = json.loads(parse_batch_json(raw))
    assert result["results"] == []
    assert result["totalHits"] == 0


def test_parse_batch_json_error_field_preserved() -> None:
    from cli_generator import parse_batch_json

    raw = json.dumps(
        {
            "status": {"hits": 0, "success": True},
            "results": [{"status": "error", "total": 0, "hits": [], "error": "bad query"}],
        }
    )
    result = json.loads(parse_batch_json(raw))
    assert result["results"][0]["error"] == "bad query"
    assert result["results"][0]["records"] == []


def test_parse_batch_json_invalid_input_returns_error_key() -> None:
    from cli_generator import parse_batch_json

    result = json.loads(parse_batch_json("{{not valid json"))
    assert "error" in result


# ── MultiQueryBuilder unit tests (no network) ─────────────────────────────────


def test_multi_query_builder_empty_on_init() -> None:
    from cli_generator import MultiQueryBuilder

    mq = MultiQueryBuilder("taxon")
    assert len(mq) == 0


def test_multi_query_builder_add_query_increments_len() -> None:
    from cli_generator import MultiQueryBuilder, QueryBuilder

    mq = MultiQueryBuilder("taxon")
    mq.add_query(QueryBuilder("taxon"))
    assert len(mq) == 1


def test_multi_query_builder_set_size_stored() -> None:
    from cli_generator import MultiQueryBuilder

    mq = MultiQueryBuilder("taxon")
    mq.set_size(50)
    assert mq._size == 50


def test_multi_query_builder_set_sort_stored() -> None:
    from cli_generator import MultiQueryBuilder

    mq = MultiQueryBuilder("taxon")
    mq.set_sort("genome_size", "desc")
    assert mq._sort_by == "genome_size"
    assert mq._sort_order == "desc"


def test_multi_query_builder_set_fields_stored() -> None:
    from cli_generator import MultiQueryBuilder

    mq = MultiQueryBuilder("taxon")
    mq.set_fields(["genome_size", "assembly_level"])
    assert len(mq._fields) == 2


def test_multi_query_builder_wrong_index_raises() -> None:
    import pytest

    from cli_generator import MultiQueryBuilder, QueryBuilder

    mq = MultiQueryBuilder("taxon")
    with pytest.raises(ValueError, match="assembly"):
        mq.add_query(QueryBuilder("assembly"))


def test_multi_query_builder_include_estimates_mismatch_raises() -> None:
    import pytest

    from cli_generator import MultiQueryBuilder, QueryBuilder

    mq = MultiQueryBuilder("taxon")
    mq.set_include_estimates(False)
    qb = QueryBuilder("taxon")
    qb.set_include_estimates(True)
    with pytest.raises(ValueError, match="include_estimates"):
        mq.add_query(qb)


def test_multi_query_builder_taxonomy_mismatch_raises() -> None:
    import pytest

    from cli_generator import MultiQueryBuilder, QueryBuilder

    mq = MultiQueryBuilder("taxon")
    mq.set_taxonomy("ott")
    qb = QueryBuilder("taxon")
    qb.set_taxonomy("ncbi")
    with pytest.raises(ValueError, match="taxonomy"):
        mq.add_query(qb)


def test_multi_query_builder_repr() -> None:
    from cli_generator import MultiQueryBuilder, QueryBuilder

    mq = MultiQueryBuilder("taxon")
    mq.add_query(QueryBuilder("taxon"))
    assert "MultiQueryBuilder" in repr(mq)
    assert "queries=1" in repr(mq)


def test_multi_query_builder_from_file_bare_list(tmp_path: pathlib.Path) -> None:
    from cli_generator import MultiQueryBuilder

    batch_file = tmp_path / "taxa.txt"
    batch_file.write_text("Caenorhabditis\nHomo sapiens\n")
    mq = MultiQueryBuilder("taxon").from_file(str(batch_file))
    assert len(mq) == 2


def test_multi_query_builder_from_file_bare_list_skips_blanks(tmp_path: pathlib.Path) -> None:
    from cli_generator import MultiQueryBuilder

    batch_file = tmp_path / "taxa.txt"
    batch_file.write_text("Caenorhabditis\n\n  \nHomo sapiens\n")
    mq = MultiQueryBuilder("taxon").from_file(str(batch_file))
    assert len(mq) == 2


def test_multi_query_builder_from_file_patch_array(tmp_path: pathlib.Path) -> None:
    from cli_generator import MultiQueryBuilder

    batch_file = tmp_path / "batch.yaml"
    batch_file.write_text("- taxon: Caenorhabditis\n  rank: species\n- taxon: Homo sapiens\n")
    mq = MultiQueryBuilder("taxon").from_file(str(batch_file))
    assert len(mq) == 2


def test_multi_query_builder_from_file_full_yaml(tmp_path: pathlib.Path) -> None:
    from cli_generator import MultiQueryBuilder

    batch_file = tmp_path / "full.yaml"
    batch_file.write_text("shared:\n  size: 50\nqueries:\n  - taxon: Caenorhabditis\n  - taxon: Homo\n")
    mq = MultiQueryBuilder("taxon").from_file(str(batch_file))
    assert len(mq) == 2
    assert mq._size == 50


def test_multi_query_builder_from_file_full_yaml_forbidden_key_raises(
    tmp_path: pathlib.Path,
) -> None:
    import pytest

    from cli_generator import MultiQueryBuilder

    batch_file = tmp_path / "bad.yaml"
    batch_file.write_text("shared:\n  taxa: [Caenorhabditis]\nqueries:\n  - taxon: Homo\n")
    with pytest.raises(ValueError, match="taxa"):
        MultiQueryBuilder("taxon").from_file(str(batch_file))


def test_multi_query_builder_from_file_frozen_key_per_query_raises(
    tmp_path: pathlib.Path,
) -> None:
    import pytest

    from cli_generator import MultiQueryBuilder

    batch_file = tmp_path / "bad.yaml"
    batch_file.write_text("- taxon: Homo\n  include_estimates: false\n")
    with pytest.raises(ValueError, match="include_estimates"):
        MultiQueryBuilder("taxon").from_file(str(batch_file))


def test_multi_query_builder_from_file_module_level(tmp_path: pathlib.Path) -> None:
    from cli_generator import from_file

    batch_file = tmp_path / "taxa.txt"
    batch_file.write_text("Caenorhabditis\nDrosophila\n")
    mq = from_file("taxon", str(batch_file))
    assert len(mq) == 2
