"""Pytest configuration and Hypothesis profile setup.

Profiles
--------
``ci``
    Used in CI (``CI=1`` env var). Runs more examples, derandomises output for
    reproducible failures, and disables the deadline.
``dev``
    Used locally. Fewer examples for a fast feedback loop.

To override the profile explicitly::

    pytest --hypothesis-profile=ci
"""

import os
import sys
from pathlib import Path

from hypothesis import HealthCheck, settings

# Add project root to path for imports
PROJECT_ROOT = Path(__file__).parent.parent.parent
sys.path.insert(0, str(PROJECT_ROOT))

settings.register_profile(
    "ci",
    max_examples=200,
    derandomize=True,
    deadline=None,
    suppress_health_check=[HealthCheck.too_slow],
    print_blob=True,
)
settings.register_profile("dev", max_examples=50)
settings.load_profile("ci" if os.environ.get("CI") else "dev")


# ── Pytest configuration ──────────────────────────────────────────────────────


def pytest_configure(config):
    """Configure pytest with custom markers."""
    config.addinivalue_line(
        "markers",
        "integration: marks tests as integration tests (require live API)",
    )
    config.addinivalue_line(
        "markers",
        "slow: marks tests as slow (require API calls or large data processing)",
    )


# ── Import fixture definitions ────────────────────────────────────────────────
# These fixtures are available to all tests

import contextlib

with contextlib.suppress(ImportError):
    from discover_fixtures import all_fixtures, fixture_name, fixture_response  # noqa: F401
