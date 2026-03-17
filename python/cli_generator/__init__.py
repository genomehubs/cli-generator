"""Generic CLI generator for genomehubs instances

This package exposes the core Rust library as a Python module. The compiled
Rust extension is imported here; see ``cli_generator.pyi`` for type signatures
of all exported symbols.
"""

from .cli_generator import build_url, version
from .query import QueryBuilder

__all__ = ["build_url", "QueryBuilder", "version"]
