"""Validate Rust workspace results and installed Host evidence across both platforms."""

import argparse
import json
from pathlib import Path


PLATFORMS = ("macos-arm64", "windows-x64-msvc")
HOSTS = ("codex", "claude")


def load_workspace_result(directory, commit, platform, run_id, attempt):
    paths = list(Path(directory).rglob("test-results.json"))
    if len(paths) != 1:
        raise ValueError(f"expected one Rust test evidence file under {directory}, found {len(paths)}")
    result = json.loads(paths[0].read_text(encoding="utf-8"))
    tests = result.get("tests")
    phases = result.get("phases", {})
    docs = result.get("doc_tests")
    if (result.get("commit") != commit or result.get("platform") != platform or
            result.get("run_id") != run_id or result.get("run_attempt") != attempt or
            not result.get("test_ids_sha256") or result.get("status") != "passed" or
            not isinstance(tests, list) or not tests or result.get("test_count") != len(tests) or
            not isinstance(docs, dict) or docs.get("passed", 0) < 1 or docs.get("failed") != 0 or
            any(phases.get(phase, {}).get("status") != "passed" or
                "--workspace" not in phases[phase].get("command", [])
                for phase in ("compile", "test", "doc")) or
            any(not test.get("id") or test.get("outcome") == "failed" for test in tests)):
        raise ValueError(f"Rust workspace evidence is incomplete or failed: {paths[0]}")
    return result


REQUIRED_EVIDENCE = ("work_contracts", "skill_contracts", "on_demand_sources", "discussion_versions",
                     "trace_anchors", "independent_skills")


def load_installed_host_evidence(directory, platform, commit, run_id, attempt):
    root = Path(directory)
    expected = {f"{host}-{platform}.json" for host in HOSTS}
    found = {path.name for path in root.glob("*.json")}
    if found != expected:
        raise ValueError(f"installed Host evidence is incomplete in {root}: {found}")
    outcomes = {}
    for host in HOSTS:
        record = json.loads((root / f"{host}-{platform}.json").read_text(encoding="utf-8"))
        evidence = record.get("simulation", {})
        if (record.get("host") != host or record.get("platform") != platform or
                record.get("commit") != commit or record.get("run_id") != run_id or
                record.get("run_attempt") != attempt or record.get("status") != "passed" or
                not record.get("version") or
                not record.get("zip") or not record.get("zip_sha256") or
                any(not evidence.get(key) for key in REQUIRED_EVIDENCE)):
            raise ValueError(f"Host simulation evidence is invalid for {host} {platform}")
        outcomes[host] = record
    return outcomes


def validate_evidence(rust_directories, host_directories, commit, run_id, attempts, host_attempts):
    for platform in PLATFORMS:
        rust = load_workspace_result(rust_directories[platform], commit, platform, run_id, attempts[platform])
        load_installed_host_evidence(host_directories[platform], platform, commit, run_id, host_attempts[platform])
        print(f"{platform}: {rust['test_count']} Rust tests, {rust['doc_tests']['passed']} doc tests; installed Host evidence valid")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for platform in PLATFORMS:
        parser.add_argument(f"--rust-{platform}", required=True, type=Path)
        parser.add_argument(f"--installed-host-{platform}", required=True, type=Path)
        parser.add_argument(f"--attempt-{platform}", required=True)
        parser.add_argument(f"--host-attempt-{platform}", required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--run-id", required=True)
    args = parser.parse_args()
    rust = {platform: getattr(args, f"rust_{platform.replace('-', '_')}") for platform in PLATFORMS}
    hosts = {platform: getattr(args, f"installed_host_{platform.replace('-', '_')}") for platform in PLATFORMS}
    attempts = {platform: getattr(args, f"attempt_{platform.replace('-', '_')}") for platform in PLATFORMS}
    host_attempts = {platform: getattr(args, f"host_attempt_{platform.replace('-', '_')}") for platform in PLATFORMS}
    validate_evidence(rust, hosts, args.commit, args.run_id, attempts, host_attempts)


if __name__ == "__main__":
    main()
