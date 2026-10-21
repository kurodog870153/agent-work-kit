"""Record release verification evidence even when an earlier job failed."""

import argparse
import hashlib
import json
import os
import sys
from pathlib import Path

from release_publication_checks import expected_assets


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else None


def build_report(version, commit, run_url, stages, expected_dir, actual_dir, checks=None):
    checks = checks or []
    assets = []
    failures = [f"{name}: {status}" for name, status in stages.items() if status != "success"]
    for name in sorted(expected_assets(version)):
        expected = digest(expected_dir / name)
        actual = digest(actual_dir / name)
        matches = expected is not None and actual == expected
        assets.append({"name": name, "expected_sha256": expected, "actual_sha256": actual, "matches": matches})
        if not matches:
            failures.append(f"asset_digest ({name}): expected={expected}, actual={actual}")
    for item in checks:
        if item["status"] != "success":
            failures.append(f"{item['check']} ({item.get('asset') or 'release'}): {item.get('error', 'failed')}")
    return {
        "version": version,
        "tag": f"v{version}",
        "commit": commit,
        "run_url": run_url,
        "assets": assets,
        "checks": checks,
        "stages": stages,
        "attestation_verified": stages.get("post_publish_verify") == "success" and all(
            item["status"] == "success" for item in checks if item["check"] == "attestation"
        ),
        "result": "success" if not failures else "failure",
        "failure_reasons": failures,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--run-url", required=True)
    parser.add_argument("--expected-dir", required=True, type=Path)
    parser.add_argument("--actual-dir", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--checks", type=Path)
    for stage in ("validate", "macos_e2e", "windows_e2e", "attest", "publish", "post_publish_verify"):
        parser.add_argument(f"--{stage.replace('_', '-')}-status", required=True)
    args = parser.parse_args()
    stages = {stage: getattr(args, f"{stage}_status") for stage in (
        "validate", "macos_e2e", "windows_e2e", "attest", "publish", "post_publish_verify"
    )}
    checks = json.loads(args.checks.read_text(encoding="utf-8")) if args.checks and args.checks.is_file() else []
    report = build_report(args.version, args.commit, args.run_url, stages, args.expected_dir, args.actual_dir, checks)
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    summary = (
        f"### Release verification: {report['result']}\n\n"
        f"Tag: {report['tag']}  \nCommit: {report['commit']}  \n"
        f"Run: {report['run_url']}\n\n"
        + "\n".join(f"- {name}: {status}" for name, status in stages.items())
        + "\n\n"
        + "\n".join(f"- {item['name']}: expected={item['expected_sha256']}, actual={item['actual_sha256']}"
                    for item in report["assets"])
        + "\n\n" + "\n".join(f"- {reason}" for reason in report["failure_reasons"])
        + "\n"
    )
    if path := os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(path, "a", encoding="utf-8") as stream:
            stream.write(summary)
    print(summary)
    return 0 if report["result"] == "success" else 1


if __name__ == "__main__":
    sys.exit(main())
