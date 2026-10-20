"""Create a release tag only after both main Rust workflows pass at HEAD."""

import json
import os
import re
import subprocess
import sys
import time
import tomllib
from pathlib import Path


WORKFLOWS = ("work-rust-macos.yml", "work-rust-windows.yml")
SHA = re.compile(r"[0-9a-f]{40}\Z")


def gh_api(path, *, method="GET", fields=()):
    command = ["gh", "api", "--method", method, path]
    for field in fields:
        command.extend(("-f", field))
    result = subprocess.run(command, check=True, capture_output=True, text=True)
    return json.loads(result.stdout) if result.stdout.strip() else None


def main_head(repository):
    return gh_api(f"repos/{repository}/branches/main")["commit"]["sha"]


def successful_runs(repository, sha):
    for workflow in WORKFLOWS:
        response = gh_api(
            f"repos/{repository}/actions/workflows/{workflow}/runs?head_sha={sha}&event=push&per_page=100"
        )
        runs = [run for run in response["workflow_runs"] if run["head_sha"] == sha and run["event"] == "push"]
        if not runs:
            return False
        latest = max(runs, key=lambda run: (run["run_number"], run["run_attempt"]))
        if latest["status"] != "completed" or latest["conclusion"] != "success":
            return False
    return True


def wait_for_successful_runs(repository, sha):
    for attempt in range(12):
        if successful_runs(repository, sha):
            return True
        if attempt < 11:
            time.sleep(5)
    return False


def checked_version(root):
    sys.path.insert(0, str(root / ".github/scripts/version-guard"))
    from version_guard import validate_lock

    with (root / "rust/Cargo.toml").open("rb") as stream:
        manifest = tomllib.load(stream)
    return validate_lock(root, manifest)


def main():
    repository = os.environ["GITHUB_REPOSITORY"]
    event_sha = os.environ["WORKFLOW_HEAD_SHA"]
    root = Path(__file__).resolve().parents[3]
    if not SHA.fullmatch(event_sha):
        raise ValueError("invalid workflow head SHA")
    if main_head(repository) != event_sha:
        print("Skipping stale main commit")
        return 0
    if not wait_for_successful_runs(repository, event_sha):
        print("Waiting for both successful Rust workflows")
        return 0
    checkout_sha = subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=root, check=True, capture_output=True, text=True
    ).stdout.strip()
    if checkout_sha != event_sha:
        raise ValueError("checkout does not match successful CI commit")
    version = checked_version(root)
    tag = f"v{version}"
    existing_tag = gh_api(f"repos/{repository}/git/ref/tags/{tag}") if tag_exists(repository, tag) else None
    if existing_tag:
        if existing_tag["object"]["type"] == "commit" and existing_tag["object"]["sha"] == event_sha:
            print(f"Tag already exists at this commit: {tag}")
            return 0
        raise ValueError(f"tag already exists: {tag}")
    if release_exists(repository, tag):
        raise ValueError(f"release already exists: {tag}")
    if main_head(repository) != event_sha or not successful_runs(repository, event_sha):
        raise ValueError("main or CI status changed before tag creation")
    gh_api(f"repos/{repository}/git/refs", method="POST", fields=(f"ref=refs/tags/{tag}", f"sha={event_sha}"))
    subprocess.run(
        ["gh", "workflow", "run", "release.yml", "--repo", repository, "--ref", tag, "-f", f"expected_sha={event_sha}"],
        check=True,
    )
    print(f"Dispatched {tag} at {event_sha}")
    return 0


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


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (KeyError, OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"Main release gate failed: {error}", file=sys.stderr)
        sys.exit(1)
