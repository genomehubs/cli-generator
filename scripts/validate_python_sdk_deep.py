#!/usr/bin/env python3
"""Deep validation logic for the Python SDK.

This module is intended to be executed from `scripts/validate_python_sdk_deep.sh`
which sets up a temporary virtualenv and installs the built wheel before
invoking this script.
"""

import contextlib
import json
import os
import sys
from pprint import pformat
from typing import Any

from goat_sdk import (
    QueryBuilder,
    annotate_source_labels,
    annotated_values,
    build_ui_url,
    build_url,
    parse_paginated_json,
    parse_response_status,
    parse_search_json,
    split_source_columns,
    to_tidy_records,
    validate_query_json,
    values_only,
)


def main() -> None:
    print("\n== Deep Validation: Python SDK ==\n")

    # Test 1: validate()
    print("Test 1: Validation (.validate())")
    qb = QueryBuilder("taxon").set_taxa(["Mammalia"], filter_type="tree").add_field("genome_size")
    errors = qb.validate()
    assert isinstance(errors, list), "validate() should return list"
    print(f"  ✓ validate() works, returned: {len(errors)} errors")

    # Test 2: count()
    print("Test 2: Count (.count())")
    qb = QueryBuilder("taxon").set_taxa(["Mammalia"], filter_type="tree")
    count = qb.count()
    assert isinstance(count, int), "count() should return int"
    assert count > 0, "Expected count > 0 for Mammalia"
    print(f"  ✓ count() works: {count} records found")

    # Test 3: search()
    print("Test 3: Search (.search())")
    qb = QueryBuilder("taxon").set_taxa(["Mammalia"], filter_type="tree").add_field("genome_size").set_size(10)
    raw = qb.search()
    results = json.loads(parse_search_json(raw))
    assert isinstance(results, list) and len(results) > 0, "search() should return non-empty list"
    print(f"  ✓ search() works: returned {len(results)} results")
    print("    First result (pretty):")
    try:
        print(pformat(results[0]))
    except Exception:
        print(json.dumps(results[0], ensure_ascii=False))

    # Test 4: add_attribute()
    print("Test 4: Attribute filters (.add_attribute())")
    qb = (
        QueryBuilder("taxon")
        .set_taxa(["Mammalia"], filter_type="tree")
        .add_attribute("genome_size", "ge", "1G")
        .add_field("genome_size")
        .set_size(10)
    )
    raw = qb.search()
    results = json.loads(parse_search_json(raw))
    assert all("genome_size" in r for r in results), "All results should have genome_size"
    print(f"  ✓ add_attribute() works: {len(results)} results with genome_size >= 1G")

    # Test 5: multiple attribute filters
    print("Test 5: Multiple attribute filters")
    qb = (
        QueryBuilder("taxon")
        .set_taxa(["Mammalia"], filter_type="tree")
        .add_attribute("genome_size", "ge", "1G")
        .add_attribute("genome_size", "le", "3G")
        .add_field("genome_size")
        .set_size(10)
    )
    raw = qb.search()
    results = json.loads(parse_search_json(raw))
    assert len(results) > 0, "Expected results in 1G-3G range"
    print(f"  ✓ Multiple filters work: {len(results)} results with 1G <= genome_size <= 3G")

    # Test 6: parse_response_status
    print("Test 6: Response parsing (parse_response_status())")
    qb = QueryBuilder("taxon").set_taxa(["Insecta"], filter_type="tree").add_field("genome_size").set_size(5)
    raw = qb.search()
    status_json = json.loads(parse_response_status(raw))
    assert "hits" in status_json, "Status should have 'hits' field"
    print("  ✓ parse_response_status() works")
    print("    Full status JSON:")
    print(json.dumps(status_json, indent=2, ensure_ascii=False))

    # Test 7: describe()
    print("Test 7: Query description (.describe())")
    qb = QueryBuilder("taxon").set_taxa(["Mammalia"], filter_type="tree").add_attribute("genome_size", "ge", "1G")
    description = qb.describe()
    assert isinstance(description, str) and len(description) > 0, "describe() should return non-empty string"
    print("  ✓ describe() works")
    print(f"    {description[:100]}...")

    # Test 8: snippet()
    print("Test 8: Code snippet generation (.snippet())")
    snippets = qb.snippet(site_name="goat", sdk_name="goat_sdk", languages=["python", "r", "javascript"])
    assert (
        "python" in snippets and "r" in snippets and "javascript" in snippets
    ), "snippet() should produce all languages"
    print("  ✓ snippet() works for all languages")

    # Test 9: parsing helper coverage
    print("Test 9: Parsing helpers (annotate/split/values/annotated/tidy)")
    raw = qb.search()
    records_json = parse_search_json(raw)
    asl = json.loads(annotate_source_labels(records_json, mode="non_direct"))
    assert isinstance(asl, list), "annotate_source_labels should return JSON array"
    print(f"  ✓ annotate_source_labels() works: returned {len(asl)} rows")
    if len(asl) > 0:
        print("    Sample annotated source labels (first 2 rows):")
        print(json.dumps(asl[:2], indent=2, ensure_ascii=False))
    split = json.loads(split_source_columns(records_json))
    assert isinstance(split, list), "split_source_columns should return JSON array"
    print(f"  ✓ split_source_columns() works: returned {len(split)} rows")
    if len(split) > 0:
        print("    Sample split columns (first 2 rows):")
        print(json.dumps(split[:2], indent=2, ensure_ascii=False))
    vo = json.loads(values_only(records_json))
    assert isinstance(vo, list), "values_only should return JSON array"
    print(f"  ✓ values_only() works: returned {len(vo)} rows")
    if len(vo) > 0:
        print("    Sample values_only (first 2 rows):")
        print(json.dumps(vo[:2], indent=2, ensure_ascii=False))
    ann = json.loads(annotated_values(records_json, mode="non_direct"))
    assert isinstance(ann, list), "annotated_values should return JSON array"
    print(f"  ✓ annotated_values() works: returned {len(ann)} rows")
    if len(ann) > 0:
        print("    Sample annotated_values (first 2 rows):")
        print(json.dumps(ann[:2], indent=2, ensure_ascii=False))
    tidy = json.loads(to_tidy_records(records_json))
    assert isinstance(tidy, list) and all(
        isinstance(r, dict) for r in tidy
    ), "to_tidy_records should return array of objects"
    print(f"  ✓ to_tidy_records() works: {len(tidy)} tidy rows")
    if len(tidy) > 0:
        print("    Sample tidy rows (first 2):")
        print(json.dumps(tidy[:2], indent=2, ensure_ascii=False))

    # Test 10: module utilities
    print("Test 10: Module utilities (validate_query_json, build_url, build_ui_url)")
    qy = qb.to_query_yaml()
    py = qb.to_params_yaml()

    # Load generated metadata if present in installed package
    field_meta = "{}"
    validation_config = "{}"
    synonyms = "{}"
    with contextlib.suppress(Exception):
        import goat_sdk as _pkg

        pkg_dir = os.path.dirname(_pkg.__file__)
        gen_dir = os.path.join(pkg_dir, "generated")
        if os.path.isdir(gen_dir):
            fm = os.path.join(gen_dir, "field_meta.json")
            vc = os.path.join(gen_dir, "validation_config.json")
            sy = os.path.join(gen_dir, "synonyms.json")
            if os.path.isfile(fm):
                with open(fm, "r", encoding="utf-8") as fh:
                    field_meta = fh.read()
            if os.path.isfile(vc):
                with open(vc, "r", encoding="utf-8") as fh:
                    validation_config = fh.read()
            if os.path.isfile(sy):
                with open(sy, "r", encoding="utf-8") as fh:
                    synonyms = fh.read()
    v = validate_query_json(qy, field_meta, validation_config, synonyms)
    if isinstance(v, str):
        v = json.loads(v)
    assert isinstance(v, list), f"validate_query_json should return list, got {type(v)}"
    print(f"  ✓ validate_query_json() works: {len(v)} messages")

    url = build_url(qy, py, "search")
    ui = build_ui_url(qy, py, "search")
    assert isinstance(url, str) and isinstance(ui, str)
    print(f"  ✓ build_url()/build_ui_url() work: {url[:60]}...")

    print("\n✓ All deep validation tests passed!\n")

    # Test 11: Deterministic fixture-based checks
    print("Test 11: Deterministic fixture-based checks (fixture_mammalia_search_raw.json)")
    fixture_path = os.path.abspath(
        os.path.join(
            os.path.dirname(__file__), "..", "tests", "python", "fixtures-goat", "fixture_mammalia_search_raw.json"
        )
    )
    if not os.path.isfile(fixture_path):
        print(f"  ⊙ Fixture not found at {fixture_path} — skipping deterministic checks")
        return

    raw_fixture = open(fixture_path, "r", encoding="utf-8").read()
    # run parsing helpers on fixture
    parsed = json.loads(parse_search_json(raw_fixture))
    print(f"  ✓ Parsed fixture into {len(parsed)} records")

    split = json.loads(split_source_columns(parse_search_json(raw_fixture)))
    # check for __direct / __descendant keys presence
    has_direct = any(any(k.endswith("__direct") for k in row.keys()) for row in split)
    has_desc = any(any(k.endswith("__descendant") for k in row.keys()) for row in split)
    print(f"    Found __direct columns: {has_direct}, __descendant columns: {has_desc}")
    assert has_direct or has_desc, "Expected at least one __direct or __descendant split column"

    ann_vals = json.loads(annotated_values(parse_search_json(raw_fixture), mode="non_direct"))
    # look for Descendant-labelled values in annotated output
    found_descendant_label = False
    for row in ann_vals:
        for v in row.values():
            if isinstance(v, str) and "Descendant" in v:
                found_descendant_label = True
                break
        if found_descendant_label:
            break
    print(f"    Found 'Descendant' label in annotated values: {found_descendant_label}")
    assert found_descendant_label, "Expected at least one 'Descendant' label in annotated values"

    print("  ✓ Deterministic fixture checks passed")


if __name__ == "__main__":
    try:
        main()
    except AssertionError as e:
        print("✗ Test failed:", e)
        sys.exit(1)
    except Exception as e:
        print("✗ Unexpected error:", e)
        raise
