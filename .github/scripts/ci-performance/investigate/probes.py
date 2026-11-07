#!/usr/bin/env python3
"""Measure isolated fixture, Git, and native process baselines."""

import argparse
import json
import os
from pathlib import Path
import shutil
import statistics
import subprocess
import sys
import tempfile
import time


def probe_operation(name, repetitions, operation):
    samples = []
    for index in range(repetitions):
        started = time.perf_counter()
        operation(index)
        samples.append(round(time.perf_counter() - started, 6))
    result = {"samples_seconds": samples, "median_seconds": round(statistics.median(samples), 6)}
    print(f"{name}: median={result['median_seconds']:.6f}s, n={repetitions}", flush=True)
    return result


def run(args):
    workspace = Path(__file__).resolve().parents[4]
    rust = workspace / "rust"
    output = Path(args.output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    suffix = ".exe" if os.name == "nt" else ""
    binary = rust / "target" / args.target / "debug" / f"work{suffix}" if args.target else rust / "target" / "debug" / f"work{suffix}"
    source = workspace / "rust/crates/work/work-infrastructure/fixtures/shared/task-diagnostics-project"
    references = workspace / "skills/work/references"
    for path in (binary, source, references):
        if not path.exists():
            raise FileNotFoundError(path)
    results = {
        "schema_version": 1,
        "config_id": "rust-operation-probes-v1",
        "platform": "windows" if sys.platform == "win32" else "macos" if sys.platform == "darwin" else sys.platform,
        "target": args.target or "native",
        "commit": os.environ.get("GITHUB_SHA", "local"),
        "repetitions": args.repetitions,
        "operations": {},
    }
    with tempfile.TemporaryDirectory(prefix="awk-probes-", dir=output) as scratch_dir:
        scratch = Path(scratch_dir)

        def git_init(index):
            directory = scratch / f"git-{index}"
            directory.mkdir()
            subprocess.run(["git", "init", "--quiet", str(directory)], check=True,
                           stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)

        results["operations"]["git_init"] = probe_operation("git_init", args.repetitions, git_init)
        for label, fixture in (("project_fixture_copy", source), ("references_copy", references)):
            results["operations"][label] = probe_operation(
                label, args.repetitions,
                lambda index, fixture=fixture, label=label: shutil.copytree(fixture, scratch / f"{label}-{index}"),
            )
        cached = [(path.relative_to(source), path.read_bytes()) for path in source.rglob("*") if path.is_file()]

        def write_cached_fixture(index):
            destination = scratch / f"cached_fixture-{index}"
            for relative, content in cached:
                target = destination / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(content)

        results["operations"]["project_fixture_cached_write"] = probe_operation(
            "project_fixture_cached_write", args.repetitions, write_cached_fixture
        )
    results["operations"]["native_work_help"] = probe_operation(
        "native_work_help", args.repetitions,
        lambda _index: subprocess.run([str(binary), "--help"], cwd=rust, check=True,
                                      stdout=subprocess.DEVNULL, stderr=subprocess.PIPE),
    )
    (output / "probes.json").write_text(json.dumps(results, indent=2) + "\n", encoding="utf-8")
    lines = [f"## Rust operation probes: {results['platform']}", "",
             f"Commit: `{results['commit']}`; target: `{results['target']}`.", "",
             "| Operation | Median seconds | Samples |", "| --- | ---: | ---: |"]
    lines.extend(f"| {name} | {item['median_seconds']} | {args.repetitions} |"
                 for name, item in results["operations"].items())
    lines.extend(["", "These are standalone baselines, not timings inside the Rust tests. The cached fixture probe shares immutable input bytes but writes a separate directory for every sample; Recovery state remains isolated."])
    markdown = "\n".join(lines) + "\n"
    (output / "summary.md").write_text(markdown, encoding="utf-8")
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as handle:
            handle.write(markdown)
    print(markdown)
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True)
    parser.add_argument("--target", default="")
    parser.add_argument("--repetitions", type=int, default=10)
    args = parser.parse_args()
    if args.repetitions < 1:
        parser.error("repetitions must be positive")
    return run(args)


if __name__ == "__main__":
    sys.exit(main())
