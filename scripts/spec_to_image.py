#!/usr/bin/env python3
"""Render a Vega spec to PNG or SVG with vl-convert.

Reads a spec from a file or stdin, so it can follow plot_to_spec:

    cargo run -q --bin plot_to_spec < plot.json | python3 scripts/spec_to_image.py -o tree.png
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

import vl_convert as vlc

SUPPORTED_SUFFIXES = (".png", ".svg")


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    """Parse command-line arguments."""
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("spec", nargs="?", type=Path, help="Vega spec JSON file; reads stdin when omitted")
    parser.add_argument("-o", "--output", type=Path, required=True, help="output file ending in .png or .svg")
    parser.add_argument("--scale", type=float, default=2.0, help="PNG scale factor (default: 2)")
    args = parser.parse_args(argv)
    if args.output.suffix.lower() not in SUPPORTED_SUFFIXES:
        parser.error(f"output must end in one of {', '.join(SUPPORTED_SUFFIXES)}")
    return args


def load_spec(path: Path | None) -> dict[str, Any]:
    """Read a Vega spec from `path`, or from stdin when `path` is None."""
    text = path.read_text(encoding="utf-8") if path else sys.stdin.read()
    spec: dict[str, Any] = json.loads(text)
    if "error" in spec:
        raise SystemExit(f"plot_to_spec reported an error: {spec['error']}")
    return spec


def write_image(spec: dict[str, Any], output: Path, scale: float) -> None:
    """Render `spec` to `output`, choosing PNG or SVG from the file suffix."""
    if output.suffix.lower() == ".svg":
        output.write_text(vlc.vega_to_svg(spec), encoding="utf-8")
    else:
        output.write_bytes(vlc.vega_to_png(spec, scale=scale))


def main(argv: list[str] | None = None) -> None:
    """Load a spec and write the rendered image."""
    args = parse_args(argv)
    spec = load_spec(args.spec)
    write_image(spec, args.output, args.scale)
    print(f"wrote {args.output}")


if __name__ == "__main__":
    main()
