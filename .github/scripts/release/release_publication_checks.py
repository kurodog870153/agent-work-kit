"""Fail closed on tag, version, repository and published asset mismatches."""

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import tomllib
from pathlib import Path

from release_package import DOWNLOADERS, PLATFORMS


SHA = re.compile(r"[0-9a-f]{40}\Z")


def api(path, *options):
    result = subprocess.run(["gh", "api", path, *options], check=True, capture_output=True, text=True)
    return json.loads(result.stdout)


def check_identity(root, repository, tag, expected_sha, *, require_latest_main=True):
    if not SHA.fullmatch(expected_sha):
        raise ValueError("expected SHA must be a full commit hash")
    sys.path.insert(0, str(root / ".github/scripts/version-guard"))
    from version_guard import validate_lock

    with (root / "rust/Cargo.toml").open("rb") as stream:
        version = validate_lock(root, tomllib.load(stream))
    if tag != f"v{version}":
        raise ValueError(f"tag {tag} does not match project version {version}")
    checkout = subprocess.run(["git", "rev-parse", "HEAD"], cwd=root, check=True, capture_output=True, text=True).stdout.strip()
    remote_tag = api(f"repos/{repository}/git/ref/tags/{tag}")["object"]["sha"]
    if checkout != expected_sha or remote_tag != expected_sha:
        raise ValueError("checkout, tag and expected SHA must match")
    if require_latest_main and api(f"repos/{repository}/branches/main")["commit"]["sha"] != expected_sha:
        raise ValueError("latest main must still match expected SHA")
    return version


def expected_assets(version):
    return ({f"work-v{version}-{platform}.zip" for platform in PLATFORMS}
            | {Path(path).name for path in DOWNLOADERS.values()} | {"SHA256SUMS.txt"})


def check_assets(repository, tag, directory, *, published):
    if published:
        release = api(f"repos/{repository}/releases/tags/{tag}")
    else:
        pages = api(f"repos/{repository}/releases?per_page=100", "--paginate", "--slurp")
        matches = [release for page in pages for release in page if release["tag_name"] == tag]
        if len(matches) != 1:
            raise ValueError(f"expected exactly one draft release for {tag}")
        release = matches[0]
    if release["tag_name"] != tag or bool(release["draft"]) == published or release["prerelease"]:
        raise ValueError("release state or tag is incorrect")
    names = expected_assets(tag.removeprefix("v"))
    assets = release["assets"]
    if {asset["name"] for asset in assets} != names or len(assets) != len(names):
        raise ValueError("release asset names or count differ from the expected five")
    for asset in assets:
        path = directory / asset["name"]
        digest = "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()
        if asset.get("digest") and asset["digest"] != digest:
            raise ValueError(f"GitHub asset digest mismatch: {asset['name']}")
        if asset["size"] != path.stat().st_size:
            raise ValueError(f"GitHub asset size mismatch: {asset['name']}")


def check_immutable(repository):
    # GitHub returns 404 when immutable releases are disabled.
    setting = api(f"repos/{repository}/immutable-releases")
    if setting.get("enabled") is not True:
        raise ValueError("immutable releases are not enabled")


def checked_git(root, *arguments):
    return subprocess.run(
        ["git", *arguments], cwd=root, check=True, capture_output=True, text=True
    ).stdout.strip()


def release_notes(root, repository, tag, expected_sha):
    if not SHA.fullmatch(expected_sha):
        raise ValueError("expected SHA must be a full commit hash")
    if checked_git(root, "rev-parse", "HEAD") != expected_sha:
        raise ValueError("release notes checkout does not match expected SHA")
    if checked_git(root, "rev-parse", f"{tag}^{{commit}}") != expected_sha:
        raise ValueError("release notes tag does not match expected SHA")
    pages = api(f"repos/{repository}/releases?per_page=100", "--paginate", "--slurp")
    published = [item for page in pages for item in page if not item["draft"] and not item["prerelease"]]
    previous = max(published, key=lambda item: item["published_at"]) if published else None
    if previous:
        baseline_tag = previous["tag_name"]
        baseline = checked_git(root, "rev-parse", f"{baseline_tag}^{{commit}}")
        if baseline == expected_sha or subprocess.run(
            ["git", "merge-base", "--is-ancestor", baseline, expected_sha], cwd=root
        ).returncode != 0:
            raise ValueError("published release baseline is not an ancestor of this tag")
        revision = f"{baseline}..{expected_sha}"
        baseline_label = f"{baseline_tag} ({baseline})"
    else:
        baseline_tag = None
        revision = expected_sha
        baseline_label = "initial repository history"
    commits = checked_git(root, "log", "--reverse", "--format=%h %s", revision).splitlines()
    if not commits:
        raise ValueError("release notes comparison has no commits")
    notes = "\n".join((f"Changes since {baseline_label}", "", *(f"- {line}" for line in commits), "")) + "\n"
    return baseline_tag, baseline_label, commits, notes


def check_first_publish_attempt(repository, tag, expected_sha, run_id, run_attempt):
    if not SHA.fullmatch(expected_sha) or not run_id.isdecimal() or run_attempt != "1":
        raise ValueError("release rerun or invalid run identity is forbidden")
    pages = api(
        f"repos/{repository}/actions/workflows/publish-release.yml/runs?head_sha={expected_sha}&event=push&per_page=100",
        "--paginate", "--slurp",
    )
    runs = [run for page in pages for run in page["workflow_runs"] if run["head_sha"] == expected_sha]
    if len(runs) != 1 or str(runs[0]["id"]) != run_id:
        raise ValueError(f"release tag {tag} has another run or an unverifiable run history")
    releases = api(f"repos/{repository}/releases?per_page=100", "--paginate", "--slurp")
    if any(release["tag_name"] == tag for page in releases for release in page):
        raise ValueError(f"release tag {tag} already has a draft or published release")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("identity", "published-identity", "immutable", "assets-draft", "assets-published", "notes", "first-attempt"))
    parser.add_argument("--repository", required=True)
    parser.add_argument("--tag")
    parser.add_argument("--expected-sha")
    parser.add_argument("--directory", type=Path)
    parser.add_argument("--notes-file", type=Path)
    parser.add_argument("--run-id")
    parser.add_argument("--run-attempt")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[3]
    try:
        if args.action in ("identity", "published-identity"):
            check_identity(root, args.repository, args.tag, args.expected_sha,
                           require_latest_main=args.action == "identity")
        elif args.action == "immutable":
            check_immutable(args.repository)
        elif args.action == "notes":
            if not args.notes_file:
                raise ValueError("notes action requires --notes-file")
            _, baseline_label, commits, notes = release_notes(root, args.repository, args.tag, args.expected_sha)
            args.notes_file.write_text(notes, encoding="utf-8")
            summary = (
                f"### Release notes comparison\n\nBaseline: {baseline_label}\n\n"
                f"Target: {args.tag} ({args.expected_sha})\n\n"
                f"Included commits: {len(commits)}\n\n"
            )
            if summary_path := os.environ.get("GITHUB_STEP_SUMMARY"):
                with open(summary_path, "a", encoding="utf-8") as stream:
                    stream.write(summary)
            print(summary)
        elif args.action == "first-attempt":
            check_first_publish_attempt(args.repository, args.tag, args.expected_sha, args.run_id or "", args.run_attempt or "")
        else:
            check_assets(args.repository, args.tag, args.directory, published=args.action == "assets-published")
    except (KeyError, OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"Release gate failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
