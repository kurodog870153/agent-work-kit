"""Verify published release assets and retain every failed check as JSON evidence."""

import argparse
import hashlib
import json
import subprocess
import sys
import urllib.request
from pathlib import Path

from release_package import DOWNLOADERS
from release_publication_checks import expected_assets


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else None


def verify(version, repository, tag, commit, manifest_sha256, expected_dir, actual_dir,
           runner=subprocess.run, fetcher=urllib.request.urlopen):
    checks = []

    def record(name, command, asset=None):
        try:
            result = runner(command, capture_output=True, text=True, check=False)
            item = {"check": name, "asset": asset, "status": "success" if result.returncode == 0 else "failure"}
            if result.returncode:
                item["error"] = (result.stderr or result.stdout or f"exit code {result.returncode}").strip()
        except OSError as error:
            item = {"check": name, "asset": asset, "status": "failure", "error": str(error)}
        checks.append(item)

    actual_dir.mkdir(exist_ok=True)
    manifest = expected_dir / "SHA256SUMS.txt"
    actual_manifest = sha256(manifest)
    checks.append({
        "check": "signed_manifest_digest", "asset": "SHA256SUMS.txt",
        "expected_sha256": manifest_sha256, "actual_sha256": actual_manifest,
        "status": "success" if manifest_sha256 and actual_manifest == manifest_sha256 else "failure",
    })
    base = [sys.executable, str(Path(__file__).with_name("release_publication_checks.py"))]
    record("published_identity", base + ["published-identity", "--repository", repository, "--tag", tag,
                                  "--expected-sha", commit])
    record("download", ["gh", "release", "download", tag, "--repo", repository, "--dir", str(actual_dir)])
    record("published_assets", base + ["assets-published", "--repository", repository, "--tag", tag,
                                 "--directory", str(actual_dir)])
    for name in sorted(expected_assets(version)):
        expected = sha256(expected_dir / name)
        actual = sha256(actual_dir / name)
        item = {
            "check": "asset_digest", "asset": name, "expected_sha256": expected,
            "actual_sha256": actual, "status": "success" if expected and expected == actual else "failure",
        }
        if item["status"] == "failure":
            item["error"] = f"expected={expected}, actual={actual}"
        checks.append(item)
        if name.endswith(".zip") and actual:
            record("zip_integrity", [sys.executable, str(Path(__file__).with_name("release_package_integrity.py")),
                                     "verify-zip", str(actual_dir / name)], name)
        if actual:
            asset = str(actual_dir / name)
            record("attestation", ["gh", "attestation", "verify", asset, "--repo", repository,
                                   "--signer-workflow", f"{repository}/.github/workflows/publish-release.yml",
                                   "--source-ref", "refs/heads/main", "--source-digest", commit], name)
            record("release_asset", ["gh", "release", "verify-asset", tag, asset, "--repo", repository], name)
    for relative in DOWNLOADERS.values():
        name = Path(relative).name
        expected = sha256(expected_dir / name)
        for channel, url in (
            ("fixed", f"https://github.com/{repository}/releases/download/{tag}/{name}"),
            ("latest", f"https://github.com/{repository}/releases/latest/download/{name}"),
        ):
            item = {"check": f"download_url_{channel}", "asset": name, "url": url,
                    "expected_sha256": expected}
            try:
                request = urllib.request.Request(url, headers={"User-Agent": "agent-work-kit-release-check"})
                with fetcher(request, timeout=30) as response:
                    actual = hashlib.sha256(response.read()).hexdigest()
                item["actual_sha256"] = actual
                item["status"] = "success" if expected and actual == expected else "failure"
                if item["status"] == "failure":
                    item["error"] = f"downloaded SHA-256 differs: expected={expected}, actual={actual}"
            except OSError as error:
                item["status"] = "failure"
                item["error"] = str(error)
            checks.append(item)
    record("release", ["gh", "release", "verify", tag, "--repo", repository])
    return checks


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("version", "repository", "tag", "commit", "manifest-sha256"):
        parser.add_argument(f"--{name}", required=True)
    parser.add_argument("--expected-dir", required=True, type=Path)
    parser.add_argument("--actual-dir", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    checks = verify(args.version, args.repository, args.tag, args.commit, args.manifest_sha256,
                    args.expected_dir, args.actual_dir)
    args.output.write_text(json.dumps(checks, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    for item in checks:
        print(f"{item['check']} {item['asset'] or ''}: {item['status']}")
    return 1 if any(item["status"] != "success" for item in checks) else 0


if __name__ == "__main__":
    sys.exit(main())
