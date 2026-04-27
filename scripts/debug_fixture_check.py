#!/usr/bin/env python3
"""Debug helper: show expected vs actual split/annotated aggregation flags for the fixture.

Run inside a venv where the generated wheel is installed.
"""
import json
import sys
from pathlib import Path

from goat_sdk import annotated_values, parse_search_json, split_source_columns


def main(path: str) -> int:
    p = Path(path)
    raw = p.read_text(encoding="utf-8")
    parsed = json.loads(parse_search_json(raw))
    print(f"Parsed records: {len(parsed)}")

    expected_direct = False
    expected_desc = False
    for rec in parsed:
        if isinstance(rec, str):
            try:
                rec_obj = json.loads(rec)
            except Exception:
                continue
        else:
            rec_obj = rec
        fields_obj = None
        if isinstance(rec_obj, dict):
            fields_obj = rec_obj.get("fields") or rec_obj.get("result", {}).get("fields") or rec_obj
        if isinstance(fields_obj, dict):
            for f in fields_obj.values():
                ags = f.get("aggregation_source") if isinstance(f, dict) else None
                if isinstance(ags, list):
                    if "direct" in ags:
                        expected_direct = True
                    if "descendant" in ags:
                        expected_desc = True
                elif isinstance(ags, str):
                    if ags == "direct":
                        expected_direct = True
                    if ags == "descendant":
                        expected_desc = True

    print(f"Expected - direct: {expected_direct}, descendant: {expected_desc}")

    split_raw = json.loads(split_source_columns(parse_search_json(raw)))
    if isinstance(split_raw, dict):
        split_list = list(split_raw.values())
    else:
        split_list = split_raw
    normalised_split = []
    for row in split_list:
        if isinstance(row, str):
            try:
                row_obj = json.loads(row)
            except Exception:
                continue
        else:
            row_obj = row
        if isinstance(row_obj, dict):
            normalised_split.append(row_obj)

    has_direct = any(any(k.endswith("__direct") for k in r.keys()) for r in normalised_split)
    has_desc = any(any(k.endswith("__descendant") for k in r.keys()) for r in normalised_split)
    print(f"Actual split - direct: {has_direct}, descendant: {has_desc}")

    if len(normalised_split) > 0:
        print("Sample split keys (first row):", list(normalised_split[0].keys())[:20])

    ann = json.loads(annotated_values(parse_search_json(raw), mode="non_direct"))
    normalised_ann = []
    for row in ann:
        if isinstance(row, str):
            try:
                normalised_ann.append(json.loads(row))
            except Exception:
                continue
        elif isinstance(row, dict):
            normalised_ann.append(row)
    found_desc_label = any(any(isinstance(v, str) and "Descendant" in v for v in r.values()) for r in normalised_ann)
    print(f"Annotated contains 'Descendant' label: {found_desc_label}")

    return 0


if __name__ == "__main__":
    path = sys.argv[1] if len(sys.argv) > 1 else "scripts/fixtures-goat/fixture_mammalia_search_raw.json"
    raise SystemExit(main(path))
