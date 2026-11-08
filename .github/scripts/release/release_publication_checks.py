"""Fail closed on tag, version, repository and published asset mismatches."""

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path

from release_package import DOWNLOADERS, HOSTS, PLATFORMS
from release_content import resolve


SHA = re.compile(r"[0-9a-f]{40}\Z")


def api(path, *options):
    result = subprocess.run(["gh", "api", path, *options], check=True, capture_output=True, text=True)
    return json.loads(result.stdout)


def check_identity(root, repository, tag, expected_sha, *, require_latest_main=True):
    if not SHA.fullmatch(expected_sha):
        raise ValueError("expected SHA must be a full commit hash")
    sys.path.insert(0, str(root / ".github/scripts/version-guard"))
    from version_guard import checked_project_version

    version = checked_project_version(root)
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
    return ({f"agent-work-kit-v{version}-{host}-{platform}.zip" for platform in PLATFORMS for host in HOSTS}
            | {Path(path).name for path in DOWNLOADERS.values()} | {"SHA256SUMS.txt"})


def check_assets(repository, tag, directory, *, published, expected_title=None, expected_notes=None):
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
    if expected_title is not None and release.get("name") != expected_title:
        raise ValueError("release title does not match validated title")
    if expected_notes is not None and (release.get("body") or "").replace("\r\n", "\n") != expected_notes.replace("\r\n", "\n"):
        raise ValueError("release notes do not match validated description")
    names = expected_assets(tag.removeprefix("v"))
    assets = release["assets"]
    if {asset["name"] for asset in assets} != names or len(assets) != len(names):
        raise ValueError("release asset names or count differ from the expected seven")
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


def published_baseline(root, repository, expected_sha):
    if not SHA.fullmatch(expected_sha):
        raise ValueError("expected SHA must be a full commit hash")
    pages = api(f"repos/{repository}/releases?per_page=100", "--paginate", "--slurp")
    published = [item for page in pages for item in page if not item["draft"] and not item["prerelease"]]
    previous = max(published, key=lambda item: item["published_at"]) if published else None
    if previous:
        baseline_tag = previous["tag_name"]
        baseline = checked_git(root, "rev-parse", f"{baseline_tag}^{{commit}}")
        if subprocess.run(
            ["git", "merge-base", "--is-ancestor", baseline, expected_sha], cwd=root
        ).returncode != 0:
            raise ValueError("published release baseline is not an ancestor of this tag")
        revision = f"{baseline}..{expected_sha}"
        baseline_label = f"{baseline_tag} ({baseline})"
    else:
        baseline_tag = None
        revision = expected_sha
        baseline_label = "initial repository history"
    return baseline_tag, baseline_label, revision


def release_notes(root, repository, tag, expected_sha):
    if not SHA.fullmatch(expected_sha):
        raise ValueError("expected SHA must be a full commit hash")
    if checked_git(root, "rev-parse", "HEAD") != expected_sha:
        raise ValueError("release notes checkout does not match expected SHA")
    # A tag is deliberately absent during the preflight gate.
    baseline_tag, baseline_label, revision = published_baseline(root, repository, expected_sha)
    if baseline_tag and checked_git(root, "rev-parse", f"{baseline_tag}^{{commit}}") == expected_sha:
        raise ValueError("published release baseline is not an ancestor of this tag")
    commits = checked_git(root, "log", "--reverse", "--format=%h %s", revision).splitlines()
    if not commits:
        raise ValueError("release notes comparison has no commits")
    notes = "\n".join((f"Changes since {baseline_label}", "", *(f"- {line}" for line in commits), "")) + "\n"
    return baseline_tag, baseline_label, commits, notes


def related_pulls(root, repository, tag, expected_sha, baseline_tag):
    revision = f"{baseline_tag}..{expected_sha}" if baseline_tag else expected_sha
    commits = checked_git(root, "rev-list", revision).splitlines()
    commit_set = set(commits)
    pulls = {}
    for sha in commits:
        pages = api(f"repos/{repository}/commits/{sha}/pulls?per_page=100", "--paginate", "--slurp")
        for pull in (item for page in pages for item in page):
            if (pull.get("merged_at") and pull.get("base", {}).get("ref") == "main"
                    and pull.get("merge_commit_sha") in commit_set):
                pulls[pull["number"]] = pull
    return [pulls[number] for number in sorted(pulls)]


def generate_release_content(repository, tag, previous_tag, expected_sha):
    options = ["--method", "POST", "-f", f"tag_name={tag}", "-f", f"target_commitish={expected_sha}"]
    if previous_tag:
        options.extend(["-f", f"previous_tag_name={previous_tag}"])
    generated = api(f"repos/{repository}/releases/generate-notes", *options)
    description = generated.get("body")
    if not isinstance(description, str) or not description.strip():
        raise ValueError("GitHub did not generate a release description")
    return description.strip()


def assemble_notes(summary, generated, history):
    if not re.search(r"^## (Overview|Highlights)\b", summary, re.M):
        summary = "## Overview\n\n" + summary
    if len(re.findall(r"^## What's Changed\s*$", generated, re.M | re.I)) != 1:
        raise ValueError("GitHub notes require exactly one What's Changed section")
    if len(re.findall(r"^\*\*Full Changelog\*\*:", generated, re.M | re.I)) != 1:
        raise ValueError("GitHub notes require exactly one Full Changelog link")
    if re.search(r"^## (What's Changed|Commit History)\b|^\*\*Full Changelog\*\*:", summary, re.M | re.I):
        raise ValueError("release summary duplicates generated sections")
    if re.search(r"^## Commit History\b", generated, re.M | re.I):
        raise ValueError("GitHub notes duplicate Commit History")
    if not history.strip() or not re.search(r"^- [^\n]+$", history, re.M):
        raise ValueError("Commit History is empty")
    summary_items = {line.strip().casefold() for line in summary.splitlines() if line.strip().startswith("- ")}
    generated_items = [line.strip().casefold() for line in generated.splitlines() if line.strip().startswith("- ")]
    if len(generated_items) != len(set(generated_items)) or summary_items.intersection(generated_items):
        raise ValueError("release notes contain duplicate list content")
    return "\n\n".join((summary.strip(), generated.strip(), "## Commit History\n\n" + history.strip())) + "\n"


def prepare_content(root, repository, tag, expected_sha, title_file, notes_file, manifest_file):
    if checked_git(root, "rev-parse", "HEAD") != expected_sha:
        raise ValueError("release content checkout differs from expected SHA")
    sys.path.insert(0, str(root / ".github/scripts/version-guard"))
    from version_guard import checked_project_version
    if tag != f"v{checked_project_version(root)}":
        raise ValueError("release content tag differs from project version")
    baseline_tag, baseline_label, commits, history = release_notes(root, repository, tag, expected_sha)
    fields, sources = resolve(root, tag, related_pulls(root, repository, tag, expected_sha, baseline_tag))
    generated = generate_release_content(repository, tag, baseline_tag, expected_sha)
    body = assemble_notes(fields["description"], generated, history)
    title_file.write_text(fields["title"] + "\n", encoding="utf-8")
    notes_file.write_text(body, encoding="utf-8")
    manifest = {"tag": tag, "sha": expected_sha, "baseline_tag": baseline_tag,
                "baseline_label": baseline_label, "sources": sources, "commits": len(commits),
                "title_sha256": hashlib.sha256(title_file.read_bytes()).hexdigest(),
                "notes_sha256": hashlib.sha256(notes_file.read_bytes()).hexdigest()}
    manifest_file.write_text(json.dumps(manifest, sort_keys=True, indent=2) + "\n", encoding="utf-8")
    return manifest


def verify_content(root, repository, tag, expected_sha, title_file, notes_file, manifest_file):
    manifest = json.loads(manifest_file.read_text(encoding="utf-8"))
    if manifest["tag"] != tag or manifest["sha"] != expected_sha or checked_git(root, "rev-parse", "HEAD") != expected_sha:
        raise ValueError("release content source identity drifted")
    for key, path in (("title_sha256", title_file), ("notes_sha256", notes_file)):
        if hashlib.sha256(path.read_bytes()).hexdigest() != manifest[key]:
            raise ValueError(f"release content changed: {path}")
    pages = api(f"repos/{repository}/releases?per_page=100", "--paginate", "--slurp")
    published = [item for page in pages for item in page if not item["draft"] and not item["prerelease"]]
    latest = max(published, key=lambda item: item["published_at"])["tag_name"] if published else None
    if latest != manifest["baseline_tag"]:
        raise ValueError("release content baseline changed after preparation")
    return manifest


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
    parser.add_argument("action", choices=("identity", "published-identity", "immutable", "assets-draft", "assets-published", "notes", "verify-content", "first-attempt"))
    parser.add_argument("--repository", required=True)
    parser.add_argument("--tag")
    parser.add_argument("--expected-sha")
    parser.add_argument("--directory", type=Path)
    parser.add_argument("--notes-file", type=Path)
    parser.add_argument("--title-file", type=Path)
    parser.add_argument("--manifest-file", type=Path)
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
        elif args.action in ("notes", "verify-content"):
            if not args.notes_file or not args.title_file or not args.manifest_file:
                raise ValueError("release content requires title, notes and manifest files")
            if args.action == "notes":
                manifest = prepare_content(root, args.repository, args.tag, args.expected_sha,
                                           args.title_file, args.notes_file, args.manifest_file)
            else:
                manifest = verify_content(root, args.repository, args.tag, args.expected_sha,
                                          args.title_file, args.notes_file, args.manifest_file)
            summary = (f"### Release content\n\nBaseline: {manifest['baseline_label']}\n\n"
                       f"Target: {args.tag} ({args.expected_sha})\n\nSources: {manifest['sources']}\n\n")
            if summary_path := os.environ.get("GITHUB_STEP_SUMMARY"):
                with open(summary_path, "a", encoding="utf-8") as stream:
                    stream.write(summary)
            print(summary)
        elif args.action == "first-attempt":
            check_first_publish_attempt(args.repository, args.tag, args.expected_sha, args.run_id or "", args.run_attempt or "")
        else:
            expected_title = None
            expected_notes = None
            if args.action == "assets-draft":
                if not args.title_file or not args.notes_file:
                    raise ValueError("assets-draft action requires --title-file and --notes-file")
                expected_title = args.title_file.read_text(encoding="utf-8").strip()
                expected_notes = args.notes_file.read_text(encoding="utf-8")
                if not expected_title:
                    raise ValueError("release title file is empty")
            check_assets(args.repository, args.tag, args.directory,
                         published=args.action == "assets-published", expected_title=expected_title,
                         expected_notes=expected_notes)
    except (KeyError, OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"Release gate failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
