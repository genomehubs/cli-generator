#!/usr/bin/env python3
"""Collect V2 API fixtures from the live GoaT instance.

This script fetches real V2 API responses for all major report types and
saves them as fixtures for parity testing against V3.

Usage:
    python scripts/collect_parity_fixtures.py [--api-base URL] [--timeout SECS]

Default fetches from https://goat.genomehubs.org/api/v2/
"""

import argparse
import json
import sys
import time
from pathlib import Path
from typing import Dict, List, Optional
from urllib.parse import urlencode
import urllib.request
import urllib.error

# Define standard test fixtures across all report types
DEFAULT_FIXTURES: Dict[str, List[Dict[str, str]]] = {
    "histogram": [
        {
            "name": "genome_size_all",
            "params": {
                "report": "histogram",
                "x": "genome_size",
                "result": "taxon",
            },
        },
        {
            "name": "genome_size_mammalia",
            "params": {
                "report": "histogram",
                "x": "genome_size",
                "result": "taxon",
                "query": "Mammalia",
                "taxonomy": "ncbi",
            },
        },
        {
            "name": "genome_size_mammalia_categorized",
            "params": {
                "report": "histogram",
                "x": "genome_size",
                "cat": "assembly_level",
                "result": "taxon",
                "query": "Mammalia",
                "taxonomy": "ncbi",
            },
        },
    ],
    "scatter": [
        {
            "name": "genome_size_vs_gc",
            "params": {
                "report": "scatter",
                "x": "genome_size",
                "y": "gc_percent",
                "result": "taxon",
            },
        },
        {
            "name": "genome_size_vs_gc_mammalia",
            "params": {
                "report": "scatter",
                "x": "genome_size",
                "y": "gc_percent",
                "result": "taxon",
                "query": "Mammalia",
                "taxonomy": "ncbi",
            },
        },
    ],
    "arc": [
        {
            "name": "arc_simple",
            "params": {
                "report": "arc",
                "x": "assembly_level=Chromosome",
                "y": "genome_size>1000000000",
                "result": "taxon",
            },
        },
        {
            "name": "arc_mammalia",
            "params": {
                "report": "arc",
                "x": "genome_size>3000000000",
                "y": "genome_size>1000000000",
                "result": "taxon",
                "query": "Mammalia",
                "taxonomy": "ncbi",
            },
        },
    ],
    "tree": [
        {
            "name": "tree_mammalia",
            "params": {
                "report": "tree",
                "result": "taxon",
                "taxa": "Mammalia",
                "rank": "phylum",
                "taxonomy": "ncbi",
            },
        },
    ],
    "map": [
        {
            "name": "map_all",
            "params": {
                "report": "map",
                "result": "taxon",
            },
        },
    ],
    "countPerRank": [
        {
            "name": "countPerRank_genome_size",
            "params": {
                "report": "countPerRank",
                "query": "genome_size",
                "result": "taxon",
            },
        },
    ],
    "sources": [
        {
            "name": "sources_all",
            "params": {
                "report": "sources",
                "result": "taxon",
            },
        },
    ],
}


def build_url(api_base: str, params: Dict[str, str]) -> str:
    """Build a full V2 API URL from params."""
    base = api_base.rstrip("/")
    query_str = urlencode(params)
    return f"{base}/report?{query_str}"


def fetch_fixture(
    url: str,
    timeout: int = 30,
) -> Optional[Dict]:
    """Fetch a single fixture from V2 API.

    Args:
        url: Full V2 API URL.
        timeout: Request timeout in seconds.

    Returns:
        Parsed JSON response, or None if fetch failed.
    """
    try:
        req = urllib.request.Request(
            url,
            headers={"User-Agent": "cli-generator-parity-test/1.0"},
        )
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            data = json.loads(resp.read().decode())
            return data
    except urllib.error.HTTPError as e:
        print(f"HTTP {e.code} for {url}", file=sys.stderr)
        return None
    except urllib.error.URLError as e:
        print(f"Failed to fetch {url}: {e}", file=sys.stderr)
        return None
    except json.JSONDecodeError as e:
        print(f"Invalid JSON response from {url}: {e}", file=sys.stderr)
        return None
    except Exception as e:
        print(f"Error fetching {url}: {e}", file=sys.stderr)
        return None


def save_fixture(
    fixture_dir: Path,
    name: str,
    data: Dict,
    url: str,
) -> bool:
    """Save fixture JSON and URL file.

    Args:
        fixture_dir: Directory to save to (e.g., tests/fixtures/parity/v2/histogram).
        name: Fixture name (becomes filename stem).
        data: JSON data to save.
        url: URL to save alongside.

    Returns:
        True if saved successfully.
    """
    fixture_dir.mkdir(parents=True, exist_ok=True)

    json_file = fixture_dir / f"{name}.json"
    url_file = fixture_dir / f"{name}.url"

    # Only overwrite if force is set (for now, we skip existing)
    if json_file.exists():
        print(f"  Skipping (already exists): {json_file.name}")
        return True

    try:
        json_file.write_text(json.dumps(data, indent=2))
        url_file.write_text(url)
        print(f"  Saved: {json_file.name}")
        return True
    except Exception as e:
        print(f"  Failed to save {json_file.name}: {e}", file=sys.stderr)
        return False


def collect_all_fixtures(
    api_base: str = "https://goat.genomehubs.org/api/v2",
    fixtures_base: Optional[Path] = None,
    timeout: int = 30,
    report_types: Optional[List[str]] = None,
) -> int:
    """Collect all default fixtures.

    Args:
        api_base: Base URL for V2 API.
        fixtures_base: Base directory for fixtures (default: tests/fixtures/parity/v2).
        timeout: Request timeout per fixture.
        report_types: Optional list of report types to collect. If None, collect all.

    Returns:
        Exit code (0 = success, 1 = some failures).
    """
    if fixtures_base is None:
        fixtures_base = Path(__file__).parent.parent / "tests" / "fixtures" / "parity" / "v2"

    total_collected = 0
    total_failed = 0

    # Filter report types if specified
    to_collect = DEFAULT_FIXTURES
    if report_types:
        to_collect = {k: v for k, v in DEFAULT_FIXTURES.items() if k in report_types}

    for report_type, fixtures in to_collect.items():
        print(f"\nCollecting {report_type} fixtures...")
        fixture_dir = fixtures_base / report_type

        for fixture_spec in fixtures:
            name = fixture_spec["name"]
            params = fixture_spec["params"]
            url = build_url(api_base, params)

            print(f"  Fetching {name}...", end=" ", flush=True)
            data = fetch_fixture(url, timeout=timeout)

            if data:
                if save_fixture(fixture_dir, name, data, url):
                    total_collected += 1
                else:
                    total_failed += 1
            else:
                total_failed += 1
                print("  Failed")

            # Rate limit: be nice to the server
            time.sleep(0.5)

    print(f"\n\nCollected {total_collected} fixtures, {total_failed} failed")
    return 1 if total_failed > 0 else 0


def main() -> int:
    """Entry point."""
    parser = argparse.ArgumentParser(
        description="Collect V2 API fixtures for parity testing",
    )
    parser.add_argument(
        "--api-base",
        default="https://goat.genomehubs.org/api/v2",
        help="Base URL for V2 API (default: GoaT production)",
    )
    parser.add_argument(
        "--fixtures-dir",
        type=Path,
        help="Output directory for fixtures (default: tests/fixtures/parity/v2)",
    )
    parser.add_argument(
        "--timeout",
        type=int,
        default=30,
        help="Request timeout in seconds (default: 30)",
    )
    parser.add_argument(
        "--report-types",
        type=str,
        nargs="+",
        help="Report types to collect (default: all)",
    )

    args = parser.parse_args()

    return collect_all_fixtures(
        api_base=args.api_base,
        fixtures_base=args.fixtures_dir,
        timeout=args.timeout,
        report_types=args.report_types,
    )


if __name__ == "__main__":
    sys.exit(main())
