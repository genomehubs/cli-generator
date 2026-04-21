"""Template for fixture discovery in generated SDKs.

This file is copied to each generated SDK's tests/ directory by the generator.

It automatically detects the site's API base URL from the SDK location.

Usage (in a generated SDK):
    # Cache fixtures from the site's API
    python tests/discover_fixtures.py --update

    # Load from cache (default)
    python tests/discover_fixtures.py
"""

import sys
from pathlib import Path

# Add the generator to path so we can import discover_fixtures template
PROJECT_ROOT = Path(__file__).parent.parent.parent  # Up to <site>-cli/
SITE_NAME = PROJECT_ROOT.parent.name.replace("my-", "")  # e.g., "goat" from "my-goat"

# For a generated SDK, the API base is always constructed from the site name
API_BASE = f"https://{SITE_NAME}.genomehubs.org/api"

if __name__ == "__main__":
    # Import the discovery script from the generator
    # In a real generated SDK, this would be embedded or copied directly
    # For now, delegate to the generator's discover_fixtures.py

    import subprocess

    # Find the cli-generator directory
    # workdir/my-<site>/<site>-cli/tests/ -> go up to find cli-generator/
    cli_gen_root = None
    current = PROJECT_ROOT.parent.parent.parent  # workdir/
    while current != current.parent:
        if (current / "tests" / "python" / "discover_fixtures.py").exists():
            cli_gen_root = current
            break
        current = current.parent

    if not cli_gen_root:
        print("Error: Could not find cli-generator root")
        sys.exit(1)

    # Call the generator's discover_fixtures.py with --api-base
    discover_script = cli_gen_root / "tests" / "python" / "discover_fixtures.py"
    args = [
        sys.executable,
        str(discover_script),
        "--api-base",
        API_BASE,
    ] + sys.argv[1:]

    result = subprocess.run(args)
    sys.exit(result.returncode)
