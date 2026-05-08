#!/usr/bin/env python3
"""Collect V3 API responses for parity testing against V2 fixtures.

Instead of trying to perfectly translate V2 queries (which have different requirements),
this script creates simple, valid V3 requests per report type and collects responses.
"""

import json
import sys
from pathlib import Path
from typing import Dict, Any, Optional
import httpx


def create_v3_request(report_type: str) -> Dict[str, str]:
    """Create a simple valid V3 request for the given report type.

    Args:
        report_type: One of 'histogram', 'scatter', 'arc', 'tree', 'countPerRank'

    Returns:
        Dict with query_yaml, params_yaml, report_yaml keys
    """
    base = {
        "query_yaml": "index: taxon\ntaxa: [root]\n",
        "params_yaml": "taxonomy: ncbi\n",
    }

    if report_type == "histogram":
        base["report_yaml"] = "report: histogram\nx: genome_size\n"
    elif report_type == "scatter":
        # Use categorical x to avoid interval issues
        base["report_yaml"] = "report: scatter\nx: assembly_level\ny: gc_percent\n"
    elif report_type == "arc":
        # V3 arc: bare feature field (has any value), reference omitted (all taxa), ranks for arcPerRank
        base["report_yaml"] = "report: arc\nfeature: assembly_span\nranks: [phylum, class, order, family]\n"
    elif report_type == "tree":
        base["report_yaml"] = "report: tree\nx: assembly_date\ny: genome_size\n"
    elif report_type == "countPerRank":
        base["report_yaml"] = "report: countPerRank\nquery: genome_size\n"
    else:
        raise ValueError(f"Unknown report type: {report_type}")

    return base


def call_v3_api(
    request: Dict[str, str], api_base: str = "http://localhost:3000", timeout: int = 30
) -> Optional[Dict[str, Any]]:
    """Call V3 API and return response.

    Args:
        request: V3 request body with query_yaml, params_yaml, report_yaml
        api_base: Base URL for API
        timeout: Request timeout in seconds

    Returns:
        API response dict, or None if failed
    """
    client = httpx.Client(base_url=api_base)
    try:
        resp = client.post("/api/v3/report", json=request, timeout=timeout)
        resp.raise_for_status()
        return resp.json()
    except Exception as e:
        print(f"  ERROR: {e}", file=sys.stderr)
        return None
    finally:
        client.close()


def main():
    # Create/ensure output directory
    output_dir = Path("tests/fixtures/parity/v3")
    output_dir.mkdir(parents=True, exist_ok=True)

    report_types = ["histogram", "scatter", "arc", "tree", "countPerRank"]

    for report_type in report_types:
        print(f"\n{report_type.upper()}")
        print("=" * 50)

        # Create request
        v3_request = create_v3_request(report_type)
        print(f"Request:")
        for key, val in v3_request.items():
            print(f"  {key}: {val.replace(chr(10), ' ')[:60]}...")

        # Call API
        print(f"Calling API...", end="", flush=True)
        response = call_v3_api(v3_request)
        if not response:
            continue

        print(" OK")

        # Save response
        if response.get("report"):
            # Save response
            resp_file = output_dir / f"{report_type}_response.json"
            with open(resp_file, "w") as f:
                json.dump(response, f, indent=2, default=str)

            # Save request for reference
            req_file = output_dir / f"{report_type}_request.json"
            with open(req_file, "w") as f:
                json.dump(v3_request, f, indent=2)

            print(f"✓ Saved response to {resp_file.name}")
            print(f"  Report type: {response['report'].get('type')}")
            print(f"  Report keys: {list(response['report'].keys())[:5]}")
        else:
            status = response.get("status", {})
            print(f"✗ API error: {status.get('error')}")


if __name__ == "__main__":
    main()
