#!/usr/bin/env python3
"""Run the Rust query-builder example for each fixture and compare bodies.

This script iterates `FIXTURE_DEFINITIONS` from
`tests/python/discover_fixtures.py`, invokes the Rust example
`live_query_demo` to print the generated ES request body, and compares
that body against the saved fixture's `query` object (ignoring `aggs`).

Usage:
  python tests/python/run_query_builder_fixtures.py

Requires: `cargo` available and the repo built (this script shells out
to `cargo run --example live_query_demo`).
"""

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DISCOVER = ROOT / "tests" / "python" / "discover_fixtures.py"
FIXTURES_DIR = ROOT / "tests" / "python" / "fixtures-goat"

if not DISCOVER.exists():
    print(f"Cannot find {DISCOVER}")
    sys.exit(2)

# Import the fixture definitions by executing the file and reading FIXTURE_DEFINITIONS
import runpy

# Execute discover_fixtures.py in its own namespace so FIXTURE_DEFINITIONS is available
spec = runpy.run_path(str(DISCOVER))
FIXTURE_DEFINITIONS = spec.get("FIXTURE_DEFINITIONS", [])


def run_rust_example_for_query(query_dict):
    """Invoke the Rust example to print the ES body for the provided query dict.

    Returns the parsed JSON body or None on failure.
    """
    # Build simple CLI args mapping used by the example
    args = ["cargo", "run", "--example", "live_query_demo", "--", "--debug"]
    # map result/index
    idx = query_dict.get("index", "taxon")
    args += ["--result", idx]
    # taxa -> --taxa (comma-separated)
    if query_dict.get("taxa"):
        args += ["--taxa", ",".join(query_dict.get("taxa"))]
    # rank -> --rank
    if query_dict.get("rank"):
        args += ["--rank", str(query_dict.get("rank"))]
    # taxon_filter_type -> --taxon_filter_type
    if query_dict.get("taxon_filter_type"):
        args += ["--taxon_filter_type", str(query_dict.get("taxon_filter_type"))]
    # fields -> --fields (comma-separated)
    if query_dict.get("fields"):
        # convert any dict field entries to their name
        names = [f["name"] if isinstance(f, dict) else str(f) for f in query_dict.get("fields")]
        args += ["--fields", ",".join(names)]
    # size
    if query_dict.get("size"):
        args += ["--size", str(query_dict.get("size"))]

    # forward optional params often present in fixtures
    if query_dict.get("names"):
        args += ["--names", ",".join(query_dict.get("names"))]
    if query_dict.get("ranks"):
        args += ["--ranks", ",".join(query_dict.get("ranks"))]
    if query_dict.get("assemblies"):
        args += ["--assemblies", ",".join(query_dict.get("assemblies"))]
    if query_dict.get("samples"):
        args += ["--samples", ",".join(query_dict.get("samples"))]
    if query_dict.get("exclude_ancestral"):
        args += ["--exclude_ancestral", ",".join(query_dict.get("exclude_ancestral"))]
    if query_dict.get("exclude_descendant"):
        args += ["--exclude_descendant", ",".join(query_dict.get("exclude_descendant"))]
    if query_dict.get("exclude_direct"):
        args += ["--exclude_direct", ",".join(query_dict.get("exclude_direct"))]
    if query_dict.get("exclude_missing"):
        args += ["--exclude_missing", ",".join(query_dict.get("exclude_missing"))]

    # If attributes are present, prefer passing a full `query_yaml` so the
    # example's `adapter::parse_url_params` will populate `attributes` fully.
    if query_dict.get("attributes"):
        # Build the minimal SearchQuery YAML structure (JSON is acceptable to serde_yaml)
        query_yaml_dict = {
            "index": query_dict.get("index", "taxon"),
            "taxa": query_dict.get("taxa", []),
            "rank": query_dict.get("rank"),
            "attributes": query_dict.get("attributes", []),
            "fields": [f if isinstance(f, dict) else {"name": f} for f in query_dict.get("fields", [])],
        }
        args += ["--query_yaml", json.dumps(query_yaml_dict)]

    try:
        proc = subprocess.run(args, cwd=str(ROOT), capture_output=True, text=True, timeout=30)
    except Exception as e:
        return None, f"failed to run example: {e}"

    out = proc.stdout + proc.stderr
    marker = "Request body:"
    if marker not in out:
        return None, "no Request body found in output"
    # take text after marker up to 'Full response:' if present
    start = out.find(marker) + len(marker)
    tail = out[start:]
    end_marker = "Full response:"
    if end_marker in tail:
        tail = tail[: tail.find(end_marker)]
    # find the first '{' and parse JSON from there
    jstart = tail.find("{")
    if jstart == -1:
        return None, "no JSON object found after Request body"
    json_text = tail[jstart:]
    # attempt to load a single JSON object and ignore any trailing text
    try:
        decoder = json.JSONDecoder()
        body, _idx = decoder.raw_decode(json_text)
        return body, None
    except Exception as e:
        return None, f"json parse error: {e}"


def compare_bodies(fixture_body, gen_body):
    """Compare the two ES 'query' objects and return a short diff summary."""
    if fixture_body is None:
        return "fixture has no 'query' block"
    # ignore top-level `aggs` presence since builder may omit aggregations
    f_keys = set(fixture_body.keys())
    g_keys = set(gen_body.keys())
    missing = f_keys - g_keys
    extra = g_keys - f_keys
    # Compare filter lengths where present
    f_filters = fixture_body.get("query", {}).get("bool", {}).get("filter", [])
    g_filters = gen_body.get("query", {}).get("bool", {}).get("filter", [])
    msgs = []
    if missing:
        msgs.append(f"missing keys: {sorted(missing)}")
    if extra:
        msgs.append(f"extra keys: {sorted(extra)}")
    if len(f_filters) != len(g_filters):
        msgs.append(f"filter count differs: fixture={len(f_filters)} generated={len(g_filters)}")
    # detect minimum_should_match wrapper in fixture
    f_has_msm = any(isinstance(f, dict) and ("minimum_should_match" in json.dumps(f)) for f in f_filters)
    g_has_msm = any("minimum_should_match" in json.dumps(f) for f in g_filters)
    if f_has_msm and not g_has_msm:
        msgs.append("fixture sets minimum_should_match but generated does not")

    return "; ".join(msgs) if msgs else "match"


def main():
    summary = []
    for fixture_def in FIXTURE_DEFINITIONS:
        name = fixture_def.get("name")
        print(f"\n== {name} ==")
        fixture_file = FIXTURES_DIR / f"{name}.json"
        if not fixture_file.exists():
            print(f"  - fixture file {fixture_file} not found; skipping")
            summary.append((name, "missing fixture file"))
            continue
        with open(fixture_file) as f:
            fixture = json.load(f)
        fixture_query = fixture.get("query")

        # Build query dict from fixture definition (same as discover_fixtures)
        query_dict = fixture_def["query_builder"]()

        gen_body, err = run_rust_example_for_query(query_dict)
        if err:
            print(f"  - generation error: {err}")
            summary.append((name, f"generation error: {err}"))
            continue

        cmp = compare_bodies(fixture_query, gen_body)
        print(f"  - compare: {cmp}")
        summary.append((name, cmp))

    print("\nSummary:")
    for name, res in summary:
        print(f"  {name}: {res}")


if __name__ == "__main__":
    main()
