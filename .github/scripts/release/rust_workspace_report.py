"""Combine existing Rust workspace CI results with installed CLI smoke evidence."""

import argparse
from collections import Counter
import json
from pathlib import Path


PLATFORMS = ("macos-arm64", "windows-x64-msvc")
HOSTS = ("codex", "claude")


def load_workspace_result(directory, commit):
    paths = list(Path(directory).rglob("performance.json"))
    if len(paths) != 1:
        raise ValueError(f"expected one Rust performance report under {directory}, found {len(paths)}")
    result = json.loads(paths[0].read_text(encoding="utf-8"))
    tests = result.get("tests")
    phases = result.get("phases", {})
    docs = result.get("doc_tests")
    if (result.get("commit") != commit or result.get("status") != "passed" or
            not isinstance(tests, list) or not tests or result.get("test_count") != len(tests) or
            not isinstance(docs, dict) or docs.get("passed", 0) < 1 or docs.get("failed") != 0 or
            any(phases.get(phase, {}).get("status") != "passed" or
                "--workspace" not in phases[phase].get("command", [])
                for phase in ("compile", "test", "doc")) or
            any(test.get("outcome") == "failed" for test in tests)):
        raise ValueError(f"Rust workspace evidence is incomplete or failed: {paths[0]}")
    suites = Counter(test["id"].split("::", 1)[0] for test in tests)
    return result, suites


def load_cli_smoke(directory, platform):
    root = Path(directory)
    expected = {f"{host}-{platform}.md" for host in HOSTS}
    found = {path.name for path in root.glob("*.md")}
    if found != expected:
        raise ValueError(f"installed CLI smoke reports are incomplete in {root}: {found}")
    outcomes = {}
    for host in HOSTS:
        content = (root / f"{host}-{platform}.md").read_text(encoding="utf-8")
        if ("# Installed Rust CLI smoke evidence" not in content or
                "$ work --project-root <root> task validate" not in content or
                "## Tooling CLI smoke\n\n```json\n" not in content):
            raise ValueError(f"Work or Tooling smoke evidence is missing for {host} {platform}")
        raw = content.split("## Tooling CLI smoke\n\n```json\n", 1)[1].split("\n```", 1)[0]
        tooling = json.loads(raw)
        if tooling.get("target_boundary") != "target_valid" or tooling.get("incompatible_policy_blocked") is not True:
            raise ValueError(f"Tooling smoke evidence is invalid for {host} {platform}")
        outcomes[host] = tooling
    return outcomes


def compose_report(rust_directories, smoke_directories, commit):
    lines = ["# Rust workspace validation", "", f"Commit: `{commit}`", "",
             "The Rust validation jobs run the complete workspace test and doc-test scopes. "
             "The package jobs separately exercise installed Work and Tooling CLIs from Release ZIPs.", "",
             "| Platform | Rust tests | Doc tests | Installed CLI hosts |", "| --- | ---: | ---: | --- |"]
    details = []
    for platform in PLATFORMS:
        rust, suites = load_workspace_result(rust_directories[platform], commit)
        smoke = load_cli_smoke(smoke_directories[platform], platform)
        lines.append(f"| {platform} | {rust['test_count']} | {rust['doc_tests']['passed']} | codex, claude |")
        details.extend(["", f"## {platform}", "", f"Target: `{rust.get('target', 'native')}`; "
                        f"test ID SHA-256: `{rust['test_ids_sha256']}`.", "",
                        "### Rust test suites", "", "| Suite | Tests |", "| --- | ---: |"])
        details.extend(f"| `{suite}` | {count} |" for suite, count in sorted(suites.items()))
        details.extend(["", "### Installed CLI smoke", ""])
        for host in HOSTS:
            outcome = smoke[host]
            details.append(f"1. `{host}`: Work Source, Discussion and TASK smoke passed; "
                           f"Tooling first load returned {outcome['first_sources']} sources, "
                           f"changed load returned {outcome['changed_sources']} sources, "
                           "target boundary passed and incompatible policy remained blocked. "
                           f"Full evidence: `cli-smoke/{host}-{platform}.md`.")
    return "\n".join(lines + details) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for platform in PLATFORMS:
        parser.add_argument(f"--rust-{platform}", required=True, type=Path)
        parser.add_argument(f"--smoke-{platform}", required=True, type=Path)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    rust = {platform: getattr(args, f"rust_{platform.replace('-', '_')}") for platform in PLATFORMS}
    smoke = {platform: getattr(args, f"smoke_{platform.replace('-', '_')}") for platform in PLATFORMS}
    text = compose_report(rust, smoke, args.commit)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(text, encoding="utf-8")
    print(f"Rust workspace report written: {args.output}")


if __name__ == "__main__":
    main()
