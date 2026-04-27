#!/usr/bin/env python3
"""Capture a real SDK search response and write to a fixture file.

Usage: python3 scripts/capture_fixture_using_sdk.py /path/to/output.json
Run inside a venv where the generated wheel is installed.
"""
import json
import sys
from pathlib import Path

from goat_sdk import QueryBuilder


def main() -> int:
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("scripts/fixtures-goat/fixture_mammalia_search_raw.json")
    qb = QueryBuilder("taxon").set_taxa(["Mammalia"], filter_type="tree").add_field("genome_size").set_size(5)
    raw = qb.search()
    # raw may already be a JSON string; write it as-is
    out.parent.mkdir(parents=True, exist_ok=True)
    with out.open("w", encoding="utf-8") as fh:
        if isinstance(raw, (bytes, bytearray)):
            fh.write(raw.decode("utf-8"))
        else:
            fh.write(str(raw))
    print(f"Wrote fixture to {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
