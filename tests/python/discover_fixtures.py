"""Discover and generate test fixtures from a site's API.

This script discovers and caches API responses for testing a specific SDK
against its corresponding API. This is designed to be copied into each
generated SDK's test directory.

Usage (in a generated SDK):
    # Cache fixtures from the site's API
    python tests/discover_fixtures.py --api-base https://your-site.org/api

Usage (in cli-generator, for any site):
    # Cache fixtures for a specific site
    python tests/python/discover_fixtures.py \\
        --site goat \\
        --api-base https://goat.genomehubs.org/api

See:
    - ../../docs/testing-generated-sdks.md (if in generated SDK)
    - docs/testing-generated-sdks.md (if in cli-generator)
"""

import argparse
import json
import os
import urllib.request
from pathlib import Path
from typing import Any

import pytest

# Detect context: are we in a generated SDK or in the generator?
# Also parse --site argument to determine cache directory
CURRENT_FILE = Path(__file__).resolve()

_parsed_site = next(
    (
        __import__("sys").argv[i + 1]
        for i, arg in enumerate(__import__("sys").argv)
        if arg == "--site" and i + 1 < len(__import__("sys").argv)
    ),
    None,
)
if "workdir" in str(CURRENT_FILE):
    # In a generated SDK: workdir/my-<site>/<site>-cli/tests/
    PROJECT_ROOT = CURRENT_FILE.parent.parent  # Up to <site>-cli/
    SITE_NAME = PROJECT_ROOT.name.replace("-cli", "")
    DEFAULT_API_BASE = f"https://{SITE_NAME}.genomehubs.org/api"
    FIXTURES_CACHE_DIR = CURRENT_FILE.parent / "fixtures"
else:
    # In cli-generator: tests/python/
    PROJECT_ROOT = Path(__file__).parent.parent.parent  # Up to cli-generator/
    SITE_NAME = _parsed_site or "goat"
    DEFAULT_API_BASE = f"https://{SITE_NAME}.genomehubs.org/api"
    # Use site-specific fixtures directory when in generator
    FIXTURES_CACHE_DIR = CURRENT_FILE.parent / f"fixtures-{SITE_NAME}"

FIXTURES_CACHE_DIR.mkdir(parents=True, exist_ok=True)
API_VERSION = "v2"


def query_api(query_dict: dict[str, Any], api_base: str) -> dict[str, Any]:
    """Execute a query against a site's API.

    Args:
        query_dict: Query state dict (index, filters, etc.)
        api_base: Base URL for the API (e.g., https://site.org/api)

    Returns:
        Parsed JSON response from the API.
    """
    from cli_generator import build_url, parse_response_status

    # Process fields: handle both simple strings and objects with modifiers
    processed_fields = []
    for field in query_dict.get("fields", []):
        if isinstance(field, dict):
            # Field with modifiers: {"name": "...", "modifier": [...]}
            processed_fields.append(field)
        elif isinstance(field, str):
            # Simple field name string -> convert to object
            processed_fields.append({"name": field})
        else:
            # Fallback for unexpected types
            processed_fields.append({"name": str(field)})

    query_yaml_dict = {
        "index": query_dict["index"],
        "taxa": query_dict.get("taxa", []),
        "rank": query_dict.get("rank"),
        "attributes": query_dict.get("attributes", []),
        "fields": processed_fields,
    }
    # Add optional query parameters
    if query_dict.get("names"):
        query_yaml_dict["names"] = query_dict["names"]
    if query_dict.get("ranks"):
        query_yaml_dict["ranks"] = query_dict["ranks"]
    if query_dict.get("assemblies"):
        query_yaml_dict["assemblies"] = query_dict["assemblies"]
    if query_dict.get("samples"):
        query_yaml_dict["samples"] = query_dict["samples"]
    if "taxon_filter_type" in query_dict:
        query_yaml_dict["taxon_filter_type"] = query_dict["taxon_filter_type"]
    if query_dict.get("exclude_ancestral"):
        query_yaml_dict["exclude_ancestral"] = query_dict["exclude_ancestral"]
    if query_dict.get("exclude_descendant"):
        query_yaml_dict["exclude_descendant"] = query_dict["exclude_descendant"]
    if query_dict.get("exclude_direct"):
        query_yaml_dict["exclude_direct"] = query_dict["exclude_direct"]
    if query_dict.get("exclude_missing"):
        query_yaml_dict["exclude_missing"] = query_dict["exclude_missing"]

    query_yaml = json.dumps(query_yaml_dict)

    params_yaml_dict = {
        "size": query_dict.get("size", 10),
        "page": query_dict.get("page", 1),
        "include_estimates": query_dict.get("include_estimates", True),
    }
    # Add optional params
    if query_dict.get("sort_by"):
        params_yaml_dict["sort_by"] = query_dict["sort_by"]
    if query_dict.get("sort_order"):
        params_yaml_dict["sort_order"] = query_dict["sort_order"]
    if query_dict.get("taxonomy"):
        params_yaml_dict["taxonomy"] = query_dict["taxonomy"]
    if query_dict.get("tidy"):
        params_yaml_dict["tidy"] = query_dict["tidy"]

    params_yaml = json.dumps(params_yaml_dict)

    url = build_url(query_yaml, params_yaml, api_base, API_VERSION, "search")

    try:
        with urllib.request.urlopen(url, timeout=30) as resp:
            return json.loads(resp.read().decode())
    except Exception as e:
        print(f"API query failed: {e}")
        return {"error": str(e)}


# ── Fixture definitions ──────────────────────────────────────────────────────


FIXTURE_DEFINITIONS = [
    # ── Basic single-parameter queries ───────────────────────────────────────
    {
        "name": "basic_taxon_search",
        "label": "Basic taxon search (10 results)",
        "query_builder": lambda: {
            "index": "taxon",
            "rank": "genus",
            "fields": ["genome_size"],
            "size": 10,
        },
        "validate_response": lambda r: len(r.get("results", [])) > 0,
    },
    {
        "name": "numeric_field_integer_filter",
        "label": "Filter by integer field (chromosome_count > 10)",
        "query_builder": lambda: {
            "index": "taxon",
            "attributes": [{"name": "chromosome_count", "operator": "gt", "value": "10"}],
            "fields": ["chromosome_count"],
            "size": 20,
        },
        "validate_response": lambda r: len(r.get("results", [])) > 0,
    },
    {
        "name": "numeric_field_range",
        "label": "Filter by numeric range (genome_size 1000000000-3000000000)",
        "query_builder": lambda: {
            "index": "taxon",
            "attributes": [
                {"name": "genome_size", "operator": "ge", "value": "1000000000"},
                {"name": "genome_size", "operator": "le", "value": "3000000000"},
            ],
            "fields": ["genome_size"],
            "size": 15,
        },
        "validate_response": lambda r: len(r.get("results", [])) >= 0,
    },
    {
        "name": "enum_field_filter",
        "label": "Filter by enum field (assembly_level = 'complete genome')",
        "query_builder": lambda: {
            "index": "taxon",
            "attributes": [{"name": "assembly_level", "operator": "eq", "value": "complete genome"}],
            "fields": ["assembly_level"],
            "size": 25,
        },
        "validate_response": lambda r: len(r.get("results", [])) > 0,
    },
    # ── Taxonomic constraints ────────────────────────────────────────────────
    {
        "name": "taxa_filter_tree",
        "label": "Taxa filter with tree traversal (Mammalia subtree)",
        "query_builder": lambda: {
            "index": "taxon",
            "taxa": ["Mammalia"],
            "taxon_filter_type": "tree",
            "rank": "species",
            "fields": ["genome_size"],
            "size": 30,
        },
        "validate_response": lambda r: len(r.get("results", [])) > 0,
    },
    {
        "name": "taxa_with_negative_filter",
        "label": "Taxa filter with exclusion (Mammalia excluding Rodentia)",
        "query_builder": lambda: {
            "index": "taxon",
            "taxa": ["Mammalia", "!Rodentia"],
            "taxon_filter_type": "tree",
            "rank": "species",
            "fields": ["genome_size"],
            "size": 20,
        },
        "validate_response": lambda r: len(r.get("results", [])) > 0,
    },
    # ── Multi-field selections ───────────────────────────────────────────────
    {
        "name": "multiple_fields_single_filter",
        "label": "Multiple fields with single filter",
        "query_builder": lambda: {
            "index": "taxon",
            "attributes": [{"name": "genome_size", "operator": "exists"}],
            "fields": ["genome_size", "chromosome_count", "assembly_level"],
            "size": 15,
        },
        "validate_response": lambda r: len(r.get("results", [])) > 0,
    },
    {
        "name": "fields_with_modifiers",
        "label": "Fields with summary modifiers (genome_size:min, genome_size:max)",
        "query_builder": lambda: {
            "index": "taxon",
            "taxa": ["Mammalia"],
            "taxon_filter_type": "tree",
            "fields": [
                {"name": "genome_size", "modifier": ["min", "max"]},
                {"name": "chromosome_count", "modifier": ["median"]},
            ],
            "size": 20,
        },
        "validate_response": lambda r: len(r.get("results", [])) > 0,
    },
    # ── Exclude parameters (field-level exclusions) ──────────────────────────
    {
        "name": "exclude_ancestral_single",
        "label": "Exclude single field from ancestral values (genome_size)",
        "query_builder": lambda: {
            "index": "taxon",
            "taxa": ["Mammalia"],
            "taxon_filter_type": "tree",
            "fields": ["genome_size"],
            "exclude_ancestral": ["genome_size"],
            "size": 20,
        },
        "validate_response": lambda r: len(r.get("results", [])) > 0,
    },
    {
        "name": "exclude_descendant_single",
        "label": "Exclude single field from descendant values (c_value)",
        "query_builder": lambda: {
            "index": "taxon",
            "taxa": ["Actinopterygii"],
            "taxon_filter_type": "tree",
            "fields": ["c_value"],
            "exclude_descendant": ["c_value"],
            "size": 20,
        },
        "validate_response": lambda r: len(r.get("results", [])) > 0,
    },
    {
        "name": "exclude_direct_single",
        "label": "Exclude single field with directly estimated values (assembly_level)",
        "query_builder": lambda: {
            "index": "taxon",
            "taxa": ["Aves"],
            "taxon_filter_type": "tree",
            "fields": ["assembly_level"],
            "exclude_direct": ["assembly_level"],
            "size": 20,
        },
        "validate_response": lambda r: len(r.get("results", [])) > 0,
    },
    {
        "name": "exclude_missing_single",
        "label": "Exclude single field with missing values (chromosome_count)",
        "query_builder": lambda: {
            "index": "taxon",
            "taxa": ["Insecta"],
            "taxon_filter_type": "tree",
            "fields": ["chromosome_count"],
            "exclude_missing": ["chromosome_count"],
            "size": 20,
        },
        "validate_response": lambda r: len(r.get("results", [])) > 0,
    },
    {
        "name": "exclude_multiple_types_combined",
        "label": "Exclude multiple types combined (ancestral, missing, direct)",
        "query_builder": lambda: {
            "index": "taxon",
            "taxa": ["Amphibia"],
            "taxon_filter_type": "tree",
            "fields": ["genome_size", "chromosome_count", "assembly_level"],
            "exclude_ancestral": ["genome_size"],
            "exclude_missing": ["chromosome_count"],
            "exclude_direct": ["assembly_level"],
            "size": 20,
        },
        "validate_response": lambda r: len(r.get("results", [])) > 0,
    },
    {
        "name": "exclude_with_taxa_filter",
        "label": "Exclude parameters combined with taxa filter (Mammalia + exclude ancestral)",
        "query_builder": lambda: {
            "index": "taxon",
            "taxa": ["Mammalia"],
            "taxon_filter_type": "tree",
            "fields": ["genome_size"],
            "exclude_ancestral": ["genome_size"],
            "size": 15,
        },
        "validate_response": lambda r: len(r.get("results", [])) > 0,
    },
    # ── Pagination ───────────────────────────────────────────────────────────
    {
        "name": "pagination_size_variation",
        "label": "Different page sizes (size=50)",
        "query_builder": lambda: {
            "index": "taxon",
            "rank": "species",
            "size": 50,
            "page": 1,
        },
        "validate_response": lambda r: len(r.get("results", [])) <= 50,
    },
    {
        "name": "pagination_second_page",
        "label": "Second page of results",
        "query_builder": lambda: {
            "index": "taxon",
            "rank": "species",
            "size": 10,
            "page": 2,
        },
        "validate_response": lambda r: len(r.get("results", [])) > 0,
    },
    # ── Complex multi-filter queries ─────────────────────────────────────────
    {
        "name": "complex_multi_constraint",
        "label": "Multiple constraints (taxa + rank + numeric filter + field modifiers)",
        "query_builder": lambda: {
            "index": "taxon",
            "taxa": ["Primates"],
            "taxon_filter_type": "tree",
            "rank": "species",
            "attributes": [{"name": "assembly_span", "operator": "ge", "value": "1000000000"}],
            "fields": [
                "genome_size",
                {"name": "chromosome_count", "modifier": ["min", "max"]},
                "assembly_level",
            ],
            "size": 15,
        },
        "validate_response": lambda r: len(r.get("results", [])) > 0,
    },
    {
        "name": "complex_multi_filter_same_field",
        "label": "Multiple filters on same field (range with modifiers)",
        "query_builder": lambda: {
            "index": "taxon",
            "attributes": [
                {"name": "c_value", "operator": "ge", "value": "0.5"},
                {"name": "c_value", "operator": "le", "value": "5.0"},
                {"name": "genome_size", "operator": "exists"},
            ],
            "fields": ["c_value", "genome_size"],
            "size": 20,
        },
        "validate_response": lambda r: len(r.get("results", [])) > 0,
    },
    # ── Assembly and Sample indexes ──────────────────────────────────────────
    {
        "name": "assembly_index_basic",
        "label": "Assembly index search",
        "query_builder": lambda: {
            "index": "assembly",
            "rank": "species",
            "fields": ["assembly_span", "assembly_level"],
            "size": 10,
        },
        "validate_response": lambda r: len(r.get("results", [])) >= 0,
    },
    {
        "name": "sample_index_basic",
        "label": "Sample index search",
        "query_builder": lambda: {
            "index": "sample",
            "rank": "species",
            "fields": ["biosample"],
            "size": 10,
        },
        "validate_response": lambda r: len(r.get("results", [])) >= 0,
    },
    # ── SDK method coverage: sort, taxonomy, names, ranks, tidy ──────────────
    {
        "name": "sorting_by_chromosome_count",
        "label": "Sort by chromosome_count ascending (extends numeric_field_integer_filter)",
        "query_builder": lambda: {
            "index": "taxon",
            "attributes": [{"name": "chromosome_count", "operator": "gt", "value": "10"}],
            "fields": ["chromosome_count"],
            "sort_by": "chromosome_count",
            "sort_order": "asc",
            "size": 20,
        },
        "validate_response": lambda r: len(r.get("results", [])) > 0,
    },
    {
        "name": "sorting_descending_order",
        "label": "Sort by arbitrary field in descending order",
        "query_builder": lambda: {
            "index": "taxon",
            "attributes": [{"name": "c_value", "operator": "ge", "value": "0.5"}],
            "fields": ["c_value"],
            "sort_by": "c_value",
            "sort_order": "desc",
            "size": 20,
        },
        "validate_response": lambda r: len(r.get("results", [])) > 0,
    },
    {
        "name": "with_taxonomy_param",
        "label": "Specify taxonomy source explicitly (ncbi or ott)",
        "query_builder": lambda: {
            "index": "taxon",
            "attributes": [{"name": "assembly_level", "operator": "eq", "value": "complete genome"}],
            "fields": ["assembly_level"],
            "taxonomy": "ncbi",
            "size": 20,
        },
        "validate_response": lambda r: len(r.get("results", [])) > 0,
    },
    {
        "name": "with_names_param",
        "label": "Filter to specific name classes",
        "query_builder": lambda: {
            "index": "taxon",
            "attributes": [{"name": "chromosome_count", "operator": "gt", "value": "10"}],
            "names": ["scientific_name"],
            "fields": ["chromosome_count"],
            "size": 10,
        },
        "validate_response": lambda r: len(r.get("results", [])) > 0,
    },
    {
        "name": "with_ranks_param",
        "label": "Control which ranks appear in lineage",
        "query_builder": lambda: {
            "index": "taxon",
            "attributes": [{"name": "c_value", "operator": "ge", "value": "0.5"}],
            "ranks": ["genus", "family", "order"],
            "fields": ["c_value"],
            "size": 15,
        },
        "validate_response": lambda r: len(r.get("results", [])) > 0,
    },
    {
        "name": "assembly_index_with_filter",
        "label": "Query assembly index (extends assembly_index_basic)",
        "query_builder": lambda: {
            "index": "assembly",
            "attributes": [{"name": "assembly_level", "operator": "eq", "value": "complete genome"}],
            "fields": ["assembly_span", "assembly_level"],
            "size": 10,
        },
        "validate_response": lambda r: len(r.get("results", [])) >= 0,
    },
]


def _load_site_metadata(site_name: str) -> tuple[dict[str, Any], dict[str, Any]]:
    """Load field_meta.json and validation_config.json for a site.

    Searches, in order:
    1. workdir/<site_name>-cli/src/generated/ (generator context)
    2. src/generated/ relative to PROJECT_ROOT (generated SDK context)

    Args:
        site_name: Site name, e.g. ``"goat"``.

    Returns:
        Tuple of (field_metadata dict, validation_config dict). Either may be
        empty if the file is not found.
    """
    candidates = [
        PROJECT_ROOT / "workdir" / f"{site_name}-cli" / "src" / "generated",
        PROJECT_ROOT / "workdir" / f"my-{site_name}" / f"{site_name}-cli" / "src" / "generated",
        PROJECT_ROOT / "src" / "generated",
    ]

    field_metadata: dict[str, Any] = {}
    validation_config: dict[str, Any] = {}

    for candidate in candidates:
        field_meta_path = candidate / "field_meta.json"
        if field_meta_path.exists():
            with open(field_meta_path) as f:
                field_metadata = json.load(f)
            config_path = candidate / "validation_config.json"
            if config_path.exists():
                with open(config_path) as f:
                    validation_config = json.load(f)
            break

    return field_metadata, validation_config


def validate_fixture_query(
    fixture_def: dict[str, Any],
    field_metadata: dict[str, Any],
    validation_config: dict[str, Any],
) -> list[str]:
    """Validate a single fixture query definition.

    Args:
        fixture_def: Fixture definition dict with 'name' and 'query_builder' keys.
        field_metadata: Field metadata loaded from the site's generated directory.
        validation_config: Validation config loaded from the site's generated directory.

    Returns:
        List of validation error strings. Empty list means the query is valid.
    """
    from cli_generator import QueryBuilder

    query_dict = fixture_def["query_builder"]()

    # Build QueryBuilder from the dict
    qb = QueryBuilder(query_dict["index"])

    # Set taxa if present
    if query_dict.get("taxa"):
        qb.set_taxa(query_dict["taxa"], filter_type=query_dict.get("taxon_filter_type", "name"))

    # Set rank
    if query_dict.get("rank"):
        qb.set_rank(query_dict["rank"])

    # Set attributes
    for attr in query_dict.get("attributes", []):
        qb.add_attribute(
            attr["name"],
            operator=attr.get("operator"),
            value=attr.get("value"),
            modifiers=attr.get("modifier"),
        )

    # Set fields
    for field in query_dict.get("fields", []):
        if isinstance(field, dict):
            qb.add_field(field.get("name", ""), modifiers=field.get("modifier"))
        else:
            qb.add_field(field)

    # Set other parameters
    if query_dict.get("names"):
        qb.set_names(query_dict["names"])
    if query_dict.get("ranks"):
        qb.set_ranks(query_dict["ranks"])
    if query_dict.get("size"):
        qb.set_size(query_dict["size"])
    if query_dict.get("exclude_ancestral"):
        qb.set_exclude_ancestral(query_dict["exclude_ancestral"])
    if query_dict.get("exclude_descendant"):
        qb.set_exclude_descendant(query_dict["exclude_descendant"])
    if query_dict.get("exclude_direct"):
        qb.set_exclude_direct(query_dict["exclude_direct"])
    if query_dict.get("exclude_missing"):
        qb.set_exclude_missing(query_dict["exclude_missing"])
    if query_dict.get("taxonomy"):
        qb.set_taxonomy(query_dict["taxonomy"])

    # Run validation with site-specific metadata
    return qb.validate(field_metadata=field_metadata, validation_config=validation_config or None)


def discover_and_cache_fixtures(
    api_base: str | None = None, update: bool = False, validate_only: bool = False
) -> dict[str, dict[str, Any]]:
    """Discover fixtures from a site's API and cache responses.

    Args:
        api_base: Base URL for the API. Defaults to the site's standard API.
        update: If True, refresh all cached fixtures from live API.
                If False, load from cache and only fetch missing ones.
        validate_only: If True, only validate queries without fetching from API.

    Returns:
        Dict mapping fixture names to their cached responses (empty if validate_only=True).
    """
    if api_base is None:
        api_base = DEFAULT_API_BASE

    field_metadata, validation_config = _load_site_metadata(SITE_NAME)
    if field_metadata:
        print(f"✓ Loaded field metadata for '{SITE_NAME}' ({len(field_metadata)} fields)")
    else:
        print(f"⚠ No field metadata found for '{SITE_NAME}' — field/attribute name checks skipped")

    print(f"{'Validating' if validate_only else 'Discovering'} fixtures from {api_base}\n")

    cached_fixtures: dict[str, dict[str, Any]] = {}
    FIXTURES_CACHE_DIR.mkdir(parents=True, exist_ok=True)

    # Pre-load all available generator fixtures for fallback
    GENERATOR_FIXTURES_DIR = CURRENT_FILE.parent / "fixtures"
    generator_fixtures: dict[str, dict[str, Any]] = {}
    if GENERATOR_FIXTURES_DIR.exists():
        for gen_file in GENERATOR_FIXTURES_DIR.glob("*.json"):
            with open(gen_file) as f:
                response = json.load(f)
                # Only use valid fixtures from generator
                if "error" not in response:
                    generator_fixtures[gen_file.stem] = response

    for fixture_def in FIXTURE_DEFINITIONS:
        name = fixture_def["name"]
        cache_file = FIXTURES_CACHE_DIR / f"{name}.json"

        if validation_errors := validate_fixture_query(fixture_def, field_metadata, validation_config):
            print(f"→ Validating {name}: {fixture_def['label']}...")
            for error in validation_errors:
                print(f"  ✗ {error}")
            if validate_only:
                continue

        if validate_only:
            print(f"✓ {name}: {fixture_def['label']} — valid")
            continue

        # Step 1: Try to load from site-specific cache first
        if not update and cache_file.exists():
            with open(cache_file) as f:
                cached_fixtures[name] = json.load(f)
                print(f"✓ Loaded {name} from site-specific cache")
                continue

        # Step 2: Try to query live API (if update=True)
        if update:
            print(f"→ Querying API for {name}: {fixture_def['label']}...")
            query_dict = fixture_def["query_builder"]()
            response = query_api(query_dict, api_base)

            # Check if API returned valid response (no error)
            if "error" not in response and fixture_def["validate_response"](response):
                print(f"  ✓ Received {len(response.get('results', []))} results")
                # Cache the valid response
                cached_fixtures[name] = response
                with open(cache_file, "w") as f:
                    json.dump(response, f, indent=2)
                continue
            else:
                # API query failed or invalid response
                print("  ✗ API query failed or invalid response")
                # Fall through to fallback below

        # Step 3: Fallback to valid fixtures from generator cache
        if name in generator_fixtures:
            cached_fixtures[name] = generator_fixtures[name]
            # Write it to site-specific cache
            with open(cache_file, "w") as f:
                json.dump(generator_fixtures[name], f, indent=2)
            print(f"✓ Loaded {name} from generator cache (fallback)")
        else:
            # No fixture available anywhere
            print(f"⚠ Fixture {name} not available (API failed, no generator cache)")

    return cached_fixtures


# ── Pytest fixture export ────────────────────────────────────────────────────


@pytest.fixture(scope="session")
def all_fixtures() -> dict[str, dict[str, Any]]:
    """Provide all cached fixtures for tests.

    Returns:
        Dict mapping fixture names to API responses.
    """
    return discover_and_cache_fixtures(update=False)


@pytest.fixture(scope="session", params=[d["name"] for d in FIXTURE_DEFINITIONS])
def fixture_name(request) -> str:
    """Parametrized fixture providing each fixture name.

    Returns:
        A fixture name string suitable for parametrized tests.
    """
    return request.param


@pytest.fixture(scope="session")
def fixture_response(all_fixtures, fixture_name) -> dict[str, Any]:
    """Provide a single fixture response for parametrized tests.

    Args:
        all_fixtures: All cached fixtures (from all_fixtures fixture)
        fixture_name: Current fixture name (from fixture_name fixture)

    Returns:
        The API response for the named fixture.
    """
    return all_fixtures[fixture_name]


if __name__ == "__main__":
    import sys

    # CLI to discover and cache fixtures
    parser = argparse.ArgumentParser(description="Discover and cache test fixtures from a site's API")
    parser.add_argument(
        "--api-base",
        type=str,
        default=None,
        help=f"Base URL for the API (default: {DEFAULT_API_BASE})",
    )
    parser.add_argument(
        "--site",
        type=str,
        default=None,
        help="Site name (e.g., 'goat'). If provided, constructs API base URL.",
    )
    parser.add_argument(
        "--update",
        action="store_true",
        help="Refresh all fixtures from live API",
    )
    parser.add_argument(
        "--validate-only",
        action="store_true",
        help="Only validate fixture queries (dry-run mode, no API fetch)",
    )

    args = parser.parse_args()

    # Allow --site as shorthand for --api-base
    api_base = args.api_base
    if args.site:
        api_base = f"https://{args.site}.genomehubs.org/api"

    print(f"Discovering fixtures from {api_base or DEFAULT_API_BASE} (update={args.update})...\n")

    fixtures = discover_and_cache_fixtures(api_base=api_base, update=args.update, validate_only=args.validate_only)

    if args.validate_only:
        print(f"\n✓ Validation complete")
    else:
        print(f"\n✓ Cached {len(fixtures)} fixtures in {FIXTURES_CACHE_DIR}")
    print("\nSummary:")
    from cli_generator import parse_response_status

    for name, response in fixtures.items():
        parsed = json.loads(parse_response_status(json.dumps(response)))
        hits = parsed.get("hits", 0)
        results = len(response.get("results", []))
        status = "✓" if results > 0 else "✗"
        print(f"  {status} {name}: {results} results (total hits: {hits})")
