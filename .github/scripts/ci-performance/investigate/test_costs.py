#!/usr/bin/env python3
"""Collect comparable Rust test timing evidence on CI runners."""

import argparse
import json
import os
from pathlib import Path
import re
import statistics
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
from report_charts import bar_chart


SLOW_STORAGE_TESTS = (
    "carried_native_validation_recovers_each_attempt_start_boundary_without_reexecution",
    "correction_staging_recovers_immutable_artifact_and_exact_invalidation",
    "attempt_close_staging_recovers_completed_stopped_and_blocked_transitions",
)


def investigation_charts(results, threads):
    runs = results["test_runs"]
    slow = [(f"{name} / {runner}", item) for name, runners in results["slow_tests"].items()
            for runner, item in runners.items()]
    native = [(f"{name} / {stage}", timing["sum_seconds"])
              for name, item in runs.items() for stage, timing in item["cli_timings"].items()]
    native.extend((f"{name} / outside CLI timer", item["outside_cli_seconds"])
                  for name, item in runs.items() if "outside_cli_seconds" in item)
    return {
        "runs": bar_chart("Test run wall time", [(name, item["seconds"]) for name, item in runs.items()]),
        "slow": bar_chart("Slow storage wall time", [(name, item["seconds"]) for name, item in slow]),
        "setup": bar_chart("Slow storage setup time", [
            (name, item["internal_timings"].get("storage_case_setup", {}).get("sum_seconds"))
            for name, item in slow
        ]),
        "native": bar_chart("Native Work command stages", native),
        "parallel": bar_chart("Nextest wall time by thread count", [
            (str(count), runs[f"nextest_{count}"]["seconds"]) for count in threads
        ]),
    }


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
    cli = {}
    for label, micros in re.findall(r"AWK_CLI_TIMING (\w+) (\d+)", log_text):
        cli.setdefault(label, {"samples": [], "count": 0, "sum_seconds": 0})["samples"].append(int(micros) / 1_000_000)
    for label, count, micros in re.findall(r"AWK_CLI_TIMING_AGG (\w+) (\d+) (\d+)", log_text):
        timing = cli.setdefault(label, {"samples": [], "count": 0, "sum_seconds": 0})
        timing["count"] += int(count)
        timing["sum_seconds"] += int(micros) / 1_000_000
    record = {
        "command": command,
        "seconds": seconds,
        "exit_code": result.returncode,
        "internal_timings": {
            label: {"count": len(samples), "sum_seconds": round(sum(samples), 3),
                    "median_seconds": round(statistics.median(samples), 6)}
            for label, samples in internal.items()
        },
        "cli_timings": {
            label: {"count": len(timing["samples"]) + timing["count"],
                    "sum_seconds": round(sum(timing["samples"]) + timing["sum_seconds"], 3),
                    "median_seconds": round(statistics.median(timing["samples"]), 6)
                    if timing["samples"] and not timing["count"] else "n/a"}
            for label, timing in cli.items()
        },
    }
    record["test_count"] = test_count_from_log(log_text, runner)
    outer = record["internal_timings"].get("native_work_command", {}).get("sum_seconds")
    inner = record["cli_timings"].get("total", {}).get("sum_seconds")
    if outer is not None and inner is not None:
        record["outside_cli_seconds"] = round(max(0, outer - inner), 3)
    durations = [float(value) for value in re.findall(r"^\s*PASS\s+\[\s*([\d.]+)s\]", log_text, re.MULTILINE)] if runner == "nextest" else []
    if durations:
        record["individual_tests"] = {
            "count": len(durations),
            "sum_seconds": round(sum(durations), 3),
            "median_seconds": round(statistics.median(durations), 3),
        }
    print(f"{name}: {seconds:.3f}s, exit={result.returncode}", flush=True)
    return record


def run(args):
    workspace = Path(__file__).resolve().parents[4]
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
        "test_scope": "work-cli process_boundary integration test",
        "test_runs": {},
        "slow_tests": {},
    }
    if not binary.is_file():
        raise FileNotFoundError(f"Build the work-cli test target first: {binary}")

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
        outer_count = item["internal_timings"].get("native_work_command", {}).get("count")
        inner_count = item["cli_timings"].get("total", {}).get("count")
        if outer_count and inner_count != outer_count:
            diagnostics.append(f"{name} CLI timing count does not match native command count")
        if name.startswith("nextest_") and item.get("individual_tests", {}).get("count") != item["test_count"]:
            diagnostics.append(f"{name} PASS lines do not match its summary count")
    for test_name, runners in results["slow_tests"].items():
        for runner, item in runners.items():
            if item["exit_code"] != 0 or item["test_count"] != 1:
                diagnostics.append(f"{test_name} {runner} did not pass exactly one test")
    results["diagnostics"] = diagnostics
    results["status"] = "failed" if diagnostics else "passed"
    (output / "investigation.json").write_text(json.dumps(results, indent=2) + "\n", encoding="utf-8")
    charts = investigation_charts(results, args.threads)
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
    summary.extend(["", charts["runs"]])
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
    summary.extend(["", charts["slow"], "", charts["setup"]])
    summary.extend(["", "| Run | Internal step | Calls | Sum seconds | Median seconds |", "| --- | --- | ---: | ---: | ---: |"])
    for name, item in results["test_runs"].items():
        summary.extend(
            f"| {name} | {label} | {timing['count']} | {timing['sum_seconds']} | {timing['median_seconds']} |"
            for label, timing in item["internal_timings"].items()
        )
    summary.extend(["", "### Native Work command breakdown", "", "| Run | Stage | Calls | Sum seconds | Median seconds |", "| --- | --- | ---: | ---: | ---: |"])
    for name, item in results["test_runs"].items():
        summary.extend(
            f"| {name} | {label} | {timing['count']} | {timing['sum_seconds']} | {timing['median_seconds']} |"
            for label, timing in item["cli_timings"].items()
        )
        if "outside_cli_seconds" in item:
            summary.append(f"| {name} | outside CLI timer (startup, serialization, exit, capture) | n/a | {item['outside_cli_seconds']} | n/a |")
    summary.extend(["", charts["native"]])
    summary.extend(["", "CLI stages overlap: dispatch includes Git, file I/O, and command logic; outside CLI timer includes process startup, response serialization, exit, and output capture. Artifact file reads are aggregated per process, so their median is unavailable. These values do not isolate startup or all file reads."])
    summary.extend(["", "### Parallelism comparison", "", "| Threads | Tests | Wall seconds | Speedup vs 1 thread |", "| ---: | ---: | ---: | ---: |"])
    baseline = results["test_runs"].get("nextest_1", {}).get("seconds")
    for threads in args.threads:
        item = results["test_runs"][f"nextest_{threads}"]
        speedup = round(baseline / item["seconds"], 2) if baseline and item["seconds"] else "n/a"
        summary.append(f"| {threads} | {item['test_count']} | {item['seconds']} | {speedup} |")
    summary.extend(["", charts["parallel"]])
    summary.extend(["", "Parallelism results cover process_boundary only; they do not establish the best setting for the full workspace."])
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
    parser.add_argument("--threads", type=int, nargs="+", default=[1, 2, 4, 8])
    args = parser.parse_args()
    if not args.threads or any(value < 1 for value in args.threads):
        parser.error("thread counts must be positive")
    return run(args)


if __name__ == "__main__":
    sys.exit(main())
