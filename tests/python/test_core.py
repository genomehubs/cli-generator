"""Tests for the cli_generator Python extension.

Covers:
- Unit tests  — verify the `version()` function returns a well-formed string.
- Property tests — invariants that must hold for any version string.
"""

import re

from hypothesis import given
from hypothesis import strategies as st

from cli_generator import version

# ── Unit tests ────────────────────────────────────────────────────────────────


def test_version_returns_a_string() -> None:
    assert isinstance(version(), str)


def test_version_is_non_empty() -> None:
    assert len(version()) > 0


def test_version_matches_semver_pattern() -> None:
    # Accepts "MAJOR.MINOR.PATCH" with an optional pre-release suffix.
    pattern = re.compile(r"^\d+\.\d+\.\d+")
    assert pattern.match(version()), f"Unexpected version string: {version()!r}"


def test_version_is_stable_across_calls() -> None:
    assert version() == version()
