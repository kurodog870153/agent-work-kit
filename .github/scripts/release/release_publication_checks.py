"""Fail closed on tag, version, repository and published asset mismatches."""

import argparse
import hashlib
import json
import re
import subprocess
import sys
import tomllib
from pathlib import Path

from release_package import PLATFORMS


SHA = re.compile(r"[0-9a-f]{40}\Z")


def api(path, *options):
    result = subprocess.run(["gh", "api", path, *options], check=True, capture_output=True, text=True)
    return json.loads(result.stdout)


def check_identity(root, repository, tag, expected_sha):
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
    main = api(f"repos/{repository}/branches/main")["commit"]["sha"]
    if checkout != expected_sha or remote_tag != expected_sha or main != expected_sha:
        raise ValueError("checkout, tag, expected SHA and latest main must match")
    return version


def expected_assets(version):
    return {f"work-v{version}-{platform}.zip" for platform in PLATFORMS} | {"SHA256SUMS.txt"}


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
        raise ValueError("release asset names or count differ from the expected three")
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("identity", "immutable", "assets-draft", "assets-published"))
    parser.add_argument("--repository", required=True)
    parser.add_argument("--tag")
    parser.add_argument("--expected-sha")
    parser.add_argument("--directory", type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[3]
    try:
        if args.action == "identity":
            check_identity(root, args.repository, args.tag, args.expected_sha)
        elif args.action == "immutable":
            check_immutable(args.repository)
        else:
            check_assets(args.repository, args.tag, args.directory, published=args.action == "assets-published")
    except (KeyError, OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"Release gate failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
