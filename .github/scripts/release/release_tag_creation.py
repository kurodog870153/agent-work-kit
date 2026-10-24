"""Create a version tag after all Rust CI jobs pass at HEAD."""

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path

from release_package import checked_version


SHA = re.compile(r"[0-9a-f]{40}\Z")


def api(path, *options):
    result = subprocess.run(["gh", "api", path, *options], check=True, capture_output=True, text=True)
    return json.loads(result.stdout)


def tag_exists(repository, tag):
    result = subprocess.run(
        ["gh", "api", f"repos/{repository}/git/ref/tags/{tag}"], capture_output=True, text=True
    )
    if result.returncode == 0:
        return True
    if "HTTP 404" in result.stderr:
        return False
    raise RuntimeError(result.stderr)


def release_exists(repository, tag):
    result = subprocess.run(
        ["gh", "api", f"repos/{repository}/releases/tags/{tag}"], capture_output=True, text=True
    )
    if result.returncode == 0:
        return True
    if "HTTP 404" in result.stderr:
        return False
    raise RuntimeError(result.stderr)


def create_tag(root, repository, expected_sha):
    if not expected_sha or not SHA.fullmatch(expected_sha):
        raise ValueError("expected SHA must be a full commit hash")
    if api(f"repos/{repository}/branches/main")["commit"]["sha"] != expected_sha:
        raise ValueError("main changed before tag creation")
    checkout = subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=root, check=True, capture_output=True, text=True
    ).stdout.strip()
    if checkout != expected_sha:
        raise ValueError("checkout does not match successful CI commit")
    tag = f"v{checked_version(root)}"
    if tag_exists(repository, tag):
        raise ValueError(f"tag already exists: {tag}")
    if release_exists(repository, tag):
        raise ValueError(f"release already exists: {tag}")
    if api(f"repos/{repository}/branches/main")["commit"]["sha"] != expected_sha:
        raise ValueError("main changed before tag creation")
    api(f"repos/{repository}/git/refs", "--method", "POST", "-f", f"ref=refs/tags/{tag}", "-f", f"sha={expected_sha}")
    return tag


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--expected-sha", required=True)
    args = parser.parse_args()
    try:
        tag = create_tag(Path(__file__).resolve().parents[3], args.repository, args.expected_sha)
        with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
            output.write(f"tag={tag}\n")
        print(f"Created {tag} at {args.expected_sha}")
    except (KeyError, OSError, RuntimeError, ValueError, subprocess.CalledProcessError) as error:
        print(f"Release tag creation failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
