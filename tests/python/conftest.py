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

from hypothesis import HealthCheck, settings

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
