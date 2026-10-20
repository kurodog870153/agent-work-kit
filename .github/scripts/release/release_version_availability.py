"""Check whether a pull request can use its proposed release version."""

import sys
from pathlib import Path

from release_package import checked_version
from main_release_coordinator import release_exists, tag_exists


def check_version_available(root, repository):
    version = checked_version(root)
    tag = f"v{version}"
    if tag_exists(repository, tag) or release_exists(repository, tag):
        raise ValueError(f"release version is already used: {tag}")
    return tag


def main():
    import os

    try:
        tag = check_version_available(Path(__file__).resolve().parents[3], os.environ["GITHUB_REPOSITORY"])
    except (KeyError, OSError, ValueError, RuntimeError) as error:
        print(f"PR release check failed: {error}", file=sys.stderr)
        return 1
    print(f"Release version is available: {tag}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
