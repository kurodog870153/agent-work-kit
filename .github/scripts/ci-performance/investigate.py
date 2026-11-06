#!/usr/bin/env python3
"""Collect comparable Rust test and filesystem timing evidence on CI runners."""

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
import time


SLOW_STORAGE_TESTS = (
    "carried_native_validation_recovers_each_attempt_start_boundary_without_reexecution",
    "correction_staging_recovers_immutable_artifact_and_exact_invalidation",
    "attempt_close_staging_recovers_completed_stopped_and_blocked_transitions",
)


def test_count_from_log(log_text, runner):
    if runner == "cargo":
        matches = re.findall(r"^test result: (?:ok|FAILED)\. (\d+) passed;", log_text, re.MULTILINE)
    elif runner == "nextest":
        matches = re.findall(r"^\s*Summary \[\s*[\d.]+s\]\s+(\d+) tests? run", log_text, re.MULTILINE)
    else:
        raise ValueError(f"unknown test runner: {runner}")
    return int(matches[-1]) if matches else None


def timed_command(name, command, output, cwd, runner, environment=None):
    print(f"{name}: started", flush=True)
    started = time.perf_counter()
    log_path = output / f"{name}.log"
    with log_path.open("w", encoding="utf-8", errors="replace") as log:
        result = subprocess.run(
            command,
            cwd=cwd,
            env=environment,
            stdout=log,
            stderr=subprocess.STDOUT,
            check=False,
        )
    seconds = round(time.perf_counter() - started, 3)
    log_text = log_path.read_text(encoding="utf-8", errors="replace")
    internal = {}
    for label, micros in re.findall(r"AWK_TEST_TIMING (\w+) (\d+)", log_text):
        internal.setdefault(label, []).append(int(micros) / 1_000_000)
    record = {
        "command": command,
        "seconds": seconds,
        "exit_code": result.returncode,
        "internal_timings": {
            label: {"count": len(samples), "sum_seconds": round(sum(samples), 3),
                    "median_seconds": round(statistics.median(samples), 6)}
            for label, samples in internal.items()
        },
    }
    record["test_count"] = test_count_from_log(log_text, runner)
    durations = [float(value) for value in re.findall(r"^\s*PASS\s+\[\s*([\d.]+)s\]", log_text, re.MULTILINE)] if runner == "nextest" else []
    if durations:
        record["individual_tests"] = {
            "count": len(durations),
            "sum_seconds": round(sum(durations), 3),
            "median_seconds": round(statistics.median(durations), 3),
        }
    print(f"{name}: {seconds:.3f}s, exit={result.returncode}", flush=True)
    return record


def probe_operation(name, repetitions, operation):
    samples = []
    for index in range(repetitions):
        start = time.perf_counter()
        operation(index)
        samples.append(round(time.perf_counter() - start, 6))
    result = {
        "samples_seconds": samples,
        "median_seconds": round(statistics.median(samples), 6),
    }
    print(f"{name}: median={result['median_seconds']:.6f}s, n={repetitions}", flush=True)
    return result


def run(args):
    workspace = Path(__file__).resolve().parents[3]
    rust = workspace / "rust"
    output = Path(args.output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    suffix = ".exe" if os.name == "nt" else ""
    binary = rust / "target" / args.target / "debug" / f"work{suffix}" if args.target else rust / "target" / "debug" / f"work{suffix}"
    target_args = ["--target", args.target] if args.target else []
    results = {
        "schema_version": 1,
        "config_id": "rust-test-investigation-v1",
        "run_id": os.environ.get("GITHUB_RUN_ID", "local"),
        "event": os.environ.get("GITHUB_EVENT_NAME", "local"),
        "commit": os.environ.get("GITHUB_SHA", "local"),
        "runner_os": os.environ.get("RUNNER_OS", sys.platform),
        "runner_arch": os.environ.get("RUNNER_ARCH", "unknown"),
        "runner_image": os.environ.get("ImageOS", "unknown"),
        "toolchain": os.environ.get("RUSTUP_TOOLCHAIN", "unknown"),
        "platform": "windows" if sys.platform == "win32" else "macos" if sys.platform == "darwin" else sys.platform,
        "target": args.target or "native",
        "repetitions": args.repetitions,
        "test_scope": "work-cli process_boundary integration test",
        "test_runs": {},
        "slow_tests": {},
        "operations": {},
    }

    source = workspace / "rust/crates/work/work-infrastructure/fixtures/shared/task-diagnostics-project"
    references = workspace / "skills/work/references"
    with tempfile.TemporaryDirectory(prefix="awk-perf-") as scratch:
        scratch = Path(scratch)

        def git_init(index):
            directory = scratch / f"git-{index}"
            directory.mkdir()
            completed = subprocess.run(
                ["git", "init", "--quiet", str(directory)],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            if completed.returncode:
                raise RuntimeError(f"git init failed: {completed.stderr.decode(errors='replace')}")

        results["operations"]["git_init"] = probe_operation("git_init", args.repetitions, git_init)
        for label, fixture in (("project_fixture_copy", source), ("references_copy", references)):
            if not fixture.is_dir():
                raise FileNotFoundError(fixture)
            results["operations"][label] = probe_operation(
                label,
                args.repetitions,
                lambda index, fixture=fixture, label=label: shutil.copytree(
                    fixture, scratch / f"{label}-{index}"
                ),
            )

    if not binary.is_file():
        raise FileNotFoundError(f"Build the work-cli test target first: {binary}")
    results["operations"]["native_work_help"] = probe_operation(
        "native_work_help",
        args.repetitions,
        lambda _index: subprocess.run(
            [str(binary), "--help"],
            cwd=rust,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            check=True,
        ),
    )

    environment = {**os.environ, "AWK_TEST_TIMING": "1", "CARGO_TERM_COLOR": "never"}
    cargo = ["cargo", "test", "--locked", "-p", "work-cli", "--test", "process_boundary", *target_args, "--", "--test-threads=1", "--nocapture"]
    results["test_runs"]["cargo_test_1"] = timed_command("cargo_test_1", cargo, output, rust, "cargo", environment)
    for threads in args.threads:
        nextest = [
            "cargo", "nextest", "run", "--locked", "-p", "work-cli", "--test", "process_boundary",
            *target_args, "--no-fail-fast", "--test-threads", str(threads), "--success-output", "immediate",
        ]
        name = f"nextest_{threads}"
        results["test_runs"][name] = timed_command(name, nextest, output, rust, "nextest", environment)

    for test_name in SLOW_STORAGE_TESTS:
        results["slow_tests"][test_name] = {}
        for runner, command in (
            ("cargo", ["cargo", "test", "--locked", "-p", "work-infrastructure", "--lib", *target_args,
                       test_name, "--", "--nocapture"]),
            ("nextest", ["cargo", "nextest", "run", "--locked", "-p", "work-infrastructure", "--lib",
                         *target_args, "--test-threads", "1", "--success-output", "immediate", test_name]),
        ):
            results["slow_tests"][test_name][runner] = timed_command(
                f"slow_{test_name}_{runner}", command, output, rust, runner, environment
            )

    diagnostics = []
    counts = [item["test_count"] for item in results["test_runs"].values()]
    if None in counts or len(set(counts)) != 1 or counts[0] == 0:
        diagnostics.append("Process boundary test counts differ or are missing")
    for name, item in results["test_runs"].items():
        if item["exit_code"] != 0:
            diagnostics.append(f"{name} exited with {item['exit_code']}")
        if name.startswith("nextest_") and item.get("individual_tests", {}).get("count") != item["test_count"]:
            diagnostics.append(f"{name} PASS lines do not match its summary count")
    for test_name, runners in results["slow_tests"].items():
        for runner, item in runners.items():
            if item["exit_code"] != 0 or item["test_count"] != 1:
                diagnostics.append(f"{test_name} {runner} did not pass exactly one test")
    results["diagnostics"] = diagnostics
    results["status"] = "failed" if diagnostics else "passed"
    (output / "investigation.json").write_text(json.dumps(results, indent=2) + "\n", encoding="utf-8")
    summary = [
        f"## Rust test investigation: {results['platform']}",
        "",
        f"Event: `{results['event']}`; commit: `{results['commit']}`; toolchain: `{results['toolchain']}`.",
        f"Scope: `{results['test_scope']}`; target: `{results['target']}`; result: **{results['status']}**.",
        "",
        "| Run | Tests | Seconds | Exit |",
        "| --- | ---: | ---: | ---: |",
    ]
    summary.extend(f"| {name} | {item['test_count']} | {item['seconds']} | {item['exit_code']} |" for name, item in results["test_runs"].items())
    summary.extend(["", "| Nextest run | PASS lines | Sum of test seconds | Median test seconds |", "| --- | ---: | ---: | ---: |"])
    summary.extend(
        f"| {name} | {item['individual_tests']['count']} | {item['individual_tests']['sum_seconds']} | {item['individual_tests']['median_seconds']} |"
        for name, item in results["test_runs"].items() if "individual_tests" in item
    )
    summary.extend(["", "### Slow storage tests", "", "| Test | Runner | Seconds | Setup sum seconds | Historical skill copy sum seconds | Native command sum seconds |", "| --- | --- | ---: | ---: | ---: | ---: |"])
    for test_name, runners in results["slow_tests"].items():
        for runner, item in runners.items():
            setup = item["internal_timings"].get("storage_case_setup", {}).get("sum_seconds", "n/a")
            skill_copy = item["internal_timings"].get("historical_skill_copy", {}).get("sum_seconds", "n/a")
            native = item["internal_timings"].get("storage_native_command", {}).get("sum_seconds", "n/a")
            summary.append(f"| {test_name} | {runner} | {item['seconds']} | {setup} | {skill_copy} | {native} |")
    summary.extend(["", "| Run | Internal step | Calls | Sum seconds | Median seconds |", "| --- | --- | ---: | ---: | ---: |"])
    for name, item in results["test_runs"].items():
        summary.extend(
            f"| {name} | {label} | {timing['count']} | {timing['sum_seconds']} | {timing['median_seconds']} |"
            for label, timing in item["internal_timings"].items()
        )
    summary.extend(["", "| Operation | Median seconds | Samples |", "| --- | ---: | ---: |"])
    summary.extend(f"| {name} | {item['median_seconds']} | {args.repetitions} |" for name, item in results["operations"].items())
    summary.extend(["", "Operation probes are standalone filesystem and process baselines, not timings inside the Rust tests."])
    if diagnostics:
        summary.extend(["", "Diagnostics: " + "; ".join(diagnostics)])
    markdown = "\n".join(summary) + "\n"
    (output / "summary.md").write_text(markdown, encoding="utf-8")
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as handle:
            handle.write(markdown)
    print(markdown)
    return 0 if results["status"] == "passed" else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True)
    parser.add_argument("--target", default="")
    parser.add_argument("--repetitions", type=int, default=10)
    parser.add_argument("--threads", type=int, nargs="+", default=[1, 2, 4, 8])
    args = parser.parse_args()
    if args.repetitions < 1 or not args.threads or any(value < 1 for value in args.threads):
        parser.error("repetitions and thread counts must be positive")
    return run(args)


if __name__ == "__main__":
    sys.exit(main())
