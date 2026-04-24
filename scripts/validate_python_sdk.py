#!/usr/bin/env python3
"""Basic validation logic for the Python SDK.

This script assumes the package under test is installed in the current
Python environment. The shell wrapper `validate_python_sdk.sh` should
create and activate a venv and install the wheel before invoking this.
"""
import json
import sys

from goat_sdk import QueryBuilder, parse_search_json


def main() -> None:
    print("\n== Basic Validation: Python SDK ==\n")

    print("Test 1: Import and instantiate")
    qb = QueryBuilder("taxon")
    assert hasattr(qb, "to_url"), "QueryBuilder missing methods"
    print("  ✓ Import and instantiation works")

    print("Test 2: Builder methods")
    qb = QueryBuilder("taxon").set_taxa(["Mammalia"], filter_type="tree").add_field("genome_size")
    assert qb._taxa and qb._fields, "Builder methods didn't populate state"
    print("  ✓ Builder methods work")

    print("Test 3: URL generation")
    url = qb.to_url()
    assert "api" in url, f"Unexpected URL: {url}"
    print(f"  ✓ URL generation works: {url}")

    print("Test 4: Validation API")
    errs = qb.validate()
    assert isinstance(errs, list), "validate() should return list"
    print(f"  ✓ validate() works: {len(errs)} errors returned")

    print("Test 5: search() returns TSV by default and parse_search_json works")
    raw = qb.search()
    records = json.loads(parse_search_json(raw))
    assert isinstance(records, list), "parse_search_json should return list"
    print(f"  ✓ search() + parse_search_json returned {len(records)} records")

    print("\n✓ Basic Python SDK validation passed\n")


if __name__ == "__main__":
    try:
        main()
    except AssertionError as e:
        print("✗ Test failed:", e)
        sys.exit(1)
    except Exception as e:
        print("✗ Unexpected error:", e)
        raise
