#!/usr/bin/env python3
"""Extract V2 API URLs from GoaT server logs and collect as fixtures.

This script parses actual server logs to extract real-world V2 API requests,
deduplicates by report type (keeping representative examples), and fetches
their responses for parity testing.

Usage:
    python scripts/collect_goat_log_fixtures.py [LOG_FILE] [--limit N]

Log file format:
    Each line should contain a /api/v2/report?... URL fragment
    Extracts unique requests, deduplicates within report types
"""

import argparse
import json
import re
import sys
import time
from collections import defaultdict
from pathlib import Path
from typing import Dict, List, Optional, Set, Tuple
from urllib.parse import parse_qs, urlencode, urlparse, urlunparse
import urllib.request
import urllib.error


def extract_urls_from_log(log_file: Path, limit: Optional[int] = None) -> Dict[str, List[str]]:
    """Extract unique V2 API URLs from server log file.

    Args:
        log_file: Path to log file containing /api/v2/report URLs.
        limit: Optional maximum number of URLs to extract per report type.

    Returns:
        Dict mapping report_type -> [URL, ...], deduplicated.
    """
    urls_by_type: Dict[str, List[str]] = defaultdict(list)
    seen: Set[str] = set()

    with open(log_file) as f:
        for line in f:
            # Extract /api/v2/report?... from line
            match = re.search(r"(/api/v2/report\?[^\s]+)", line)
            if not match:
                continue

            url_fragment = match.group(1)

            # Skip if already seen (exact duplicate)
            if url_fragment in seen:
                continue
            seen.add(url_fragment)

            # Parse to get report type
            params = parse_qs(urlparse(url_fragment).query)
            report_type = params.get("report", [None])[0]
            if not report_type:
                continue

            urls_by_type[report_type].append(url_fragment)

            # Apply per-type limit
            if limit and len(urls_by_type[report_type]) >= limit:
                urls_by_type[report_type] = urls_by_type[report_type][:limit]

    return dict(urls_by_type)


def extract_param_signature(url: str) -> Dict[str, str]:
    """Extract key parameters from URL for diversity scoring.

    Returns dict with x, y, z, cat, rank, and filter complexity indicators.
    """
    params = parse_qs(urlparse(url).query)

    # Extract field names (what's being measured)
    x = params.get("x", [""])[0]
    y = params.get("y", [""])[0]
    z = params.get("z", [""])[0]
    cat = params.get("cat", [""])[0]
    rank = params.get("rank", [""])[0]

    # Extract field base names (ignore filters, aggregates, etc.)
    def extract_field_name(expr: str) -> str:
        """Get base field name from expression like 'min(assembly_date)' or 'genome_size>1e9'."""
        # Remove aggregates: min(x) -> x, max(x) -> x, etc.
        expr = re.sub(r"(min|max|median|mean)\((\w+)\)", r"\2", expr)
        # Remove operators and values: genome_size>1e9 -> genome_size
        expr = re.sub(r"[<>=!]+.*$", "", expr)
        # Remove logical operators: a AND b -> a, b
        expr = re.sub(r"\s*(AND|OR|NOT)\s+", " ", expr)
        # Remove functions and take first field
        expr = re.split(r"[\s\(\)&|,]", expr)[0]
        return expr.strip()

    x_field = extract_field_name(x) if x else ""
    y_field = extract_field_name(y) if y else ""
    z_field = extract_field_name(z) if z else ""
    cat_field = extract_field_name(cat) if cat else ""

    # Flags for filter complexity
    has_aggregate = bool(re.search(r"(min|max|median|mean)\(", x + y + z))
    has_and = "AND" in (x + y + z).upper()
    has_complex_cat = len(cat.split(",")) > 1 if cat else False
    has_bioproject = "bioproject" in (x + y + z + cat).lower()
    has_rank_filter = "rank(" in (x + y + z).lower() or "tax_rank" in (x + y + z).lower()

    return {
        "x_field": x_field,
        "y_field": y_field,
        "z_field": z_field,
        "cat_field": cat_field,
        "rank": rank,
        "has_aggregate": str(has_aggregate),
        "has_and": str(has_and),
        "has_complex_cat": str(has_complex_cat),
        "has_bioproject": str(has_bioproject),
        "has_rank_filter": str(has_rank_filter),
    }


def select_representative_urls(
    urls_by_type: Dict[str, List[str]],
    max_per_type: int = 3,
) -> Dict[str, List[str]]:
    """Select representative URLs per report type based on parameter diversity.

    Scores URLs by diversity of field combinations and filter complexity
    to capture real-world usage patterns.

    Args:
        urls_by_type: Dict mapping report_type -> [URL, ...].
        max_per_type: Maximum URLs to keep per report type.

    Returns:
        Dict with deduplicated representative URLs.
    """
    result: Dict[str, List[str]] = {}

    for report_type, urls in urls_by_type.items():
        if not urls:
            continue

        # Score each URL by diversity features
        scored_urls = []
        for url in urls:
            sig = extract_param_signature(url)
            # Create a tuple for sorting: prioritize complex filters, diverse fields
            score = (
                sig["has_and"] == "True",  # Prefer AND expressions
                sig["has_aggregate"] == "True",  # Prefer aggregates
                sig["has_complex_cat"] == "True",  # Prefer complex categorization
                sig["has_bioproject"] == "True",  # Prefer bioproject filters
                len(sig["x_field"]) > 0,  # Prefer URLs with explicit x field
                len(sig["y_field"]) > 0,  # Prefer URLs with explicit y field
                len(sig["rank"]) > 0,  # Prefer URLs with rank parameter
            )
            scored_urls.append((url, sig, score))

        # Sort by score (most complex/diverse first)
        scored_urls.sort(key=lambda x: x[2], reverse=True)

        # Pick representative URLs with diverse signatures
        kept = []
        seen_sigs = set()

        for url, sig, _score in scored_urls:
            # Create a minimal signature (deduplicate exact param combinations)
            minimal_sig = (
                sig["x_field"],
                sig["y_field"],
                sig["z_field"],
                sig["cat_field"],
                sig["has_and"],
                sig["has_aggregate"],
            )

            if minimal_sig not in seen_sigs:
                seen_sigs.add(minimal_sig)
                kept.append(url)

            if len(kept) >= max_per_type:
                break

        result[report_type] = kept

    return result


def build_full_url(
    url_fragment: str,
    api_base: str = "https://goat.genomehubs.org",
) -> str:
    """Convert /api/v2/report?... fragment to full URL.

    Args:
        url_fragment: Path + query string like /api/v2/report?report=histogram&...
        api_base: Base URL (default: production GoaT).

    Returns:
        Full URL.
    """
    return api_base.rstrip("/") + url_fragment


def fetch_and_save_fixture(
    url: str,
    fixture_dir: Path,
    report_type: str,
    fixture_name: str,
    timeout: int = 30,
) -> bool:
    """Fetch and save a single fixture.

    Args:
        url: Full V2 API URL.
        fixture_dir: Directory to save (e.g. tests/fixtures/parity/v2/histogram).
        report_type: Report type (for directory).
        fixture_name: Fixture name (for filename).
        timeout: Request timeout in seconds.

    Returns:
        True if saved successfully.
    """
    type_dir = fixture_dir / report_type
    type_dir.mkdir(parents=True, exist_ok=True)

    json_file = type_dir / f"{fixture_name}.json"
    url_file = type_dir / f"{fixture_name}.url"

    # Skip if already exists
    if json_file.exists():
        print(f"  Skipping (exists): {fixture_name}")
        return True

    try:
        print(f"  Fetching {fixture_name}...", end=" ", flush=True)
        req = urllib.request.Request(
            url,
            headers={"User-Agent": "cli-generator-parity-test/1.0"},
        )
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            data = json.loads(resp.read().decode())

        json_file.write_text(json.dumps(data, indent=2))
        url_file.write_text(url)
        print("OK")
        return True

    except urllib.error.HTTPError as e:
        print(f"HTTP {e.code}")
        return False
    except urllib.error.URLError as e:
        print(f"Failed: {e}")
        return False
    except json.JSONDecodeError as e:
        print(f"Invalid JSON: {e}")
        return False
    except Exception as e:
        print(f"Error: {e}")
        return False


def collect_goat_log_fixtures(
    log_file: Path,
    fixtures_base: Optional[Path] = None,
    max_per_type: int = 3,
    timeout: int = 30,
    api_base: str = "https://goat.genomehubs.org",
) -> int:
    """Main entry point.

    Args:
        log_file: Path to GoaT log file.
        fixtures_base: Output directory for fixtures.
        max_per_type: Max fixtures to keep per report type.
        timeout: Request timeout per fixture.
        api_base: V2 API base URL.

    Returns:
        Exit code (0 = success, 1 = failures).
    """
    if not log_file.exists():
        print(f"Log file not found: {log_file}", file=sys.stderr)
        return 1

    if fixtures_base is None:
        fixtures_base = Path(__file__).parent.parent / "tests" / "fixtures" / "parity" / "v2"

    print(f"Extracting URLs from {log_file.name}...")
    urls_by_type = extract_urls_from_log(log_file)

    print(f"Found {sum(len(v) for v in urls_by_type.values())} unique URLs across {len(urls_by_type)} report types")

    print(f"Selecting representative examples (max {max_per_type} per type)...")
    selected = select_representative_urls(urls_by_type, max_per_type=max_per_type)

    total_to_fetch = sum(len(v) for v in selected.values())
    print(f"Will fetch {total_to_fetch} fixtures total\n")

    total_collected = 0
    total_failed = 0

    for report_type in sorted(selected.keys()):
        urls = selected[report_type]
        print(f"Collecting {report_type} ({len(urls)} fixtures):")

        for i, url in enumerate(urls, start=1):
            # Create a filename based on report type and counter
            fixture_name = f"{report_type}_{i:02d}"

            full_url = build_full_url(url, api_base)

            if fetch_and_save_fixture(
                url=full_url,
                fixture_dir=fixtures_base,
                report_type=report_type,
                fixture_name=fixture_name,
                timeout=timeout,
            ):
                total_collected += 1
            else:
                total_failed += 1

            # Rate limit: be nice to server
            time.sleep(0.5)

        print()

    print(f"Collected {total_collected} fixtures, {total_failed} failed")
    return 1 if total_failed > 0 else 0


def main() -> int:
    """Entry point."""
    parser = argparse.ArgumentParser(
        description="Collect V2 API fixtures from GoaT server logs",
    )
    parser.add_argument(
        "log_file",
        type=Path,
        help="Path to server log file containing /api/v2/report URLs",
    )
    parser.add_argument(
        "--fixtures-dir",
        type=Path,
        help="Output directory for fixtures (default: tests/fixtures/parity/v2)",
    )
    parser.add_argument(
        "--max-per-type",
        type=int,
        default=3,
        help="Maximum fixtures to collect per report type (default: 3)",
    )
    parser.add_argument(
        "--timeout",
        type=int,
        default=30,
        help="Request timeout in seconds (default: 30)",
    )
    parser.add_argument(
        "--api-base",
        default="https://goat.genomehubs.org",
        help="Base URL for V2 API (default: production)",
    )

    args = parser.parse_args()

    return collect_goat_log_fixtures(
        log_file=args.log_file,
        fixtures_base=args.fixtures_dir,
        max_per_type=args.max_per_type,
        timeout=args.timeout,
        api_base=args.api_base,
    )


if __name__ == "__main__":
    sys.exit(main())
