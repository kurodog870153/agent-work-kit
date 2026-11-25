#!/usr/bin/env python3
"""Collect Rust test evidence and enforce workspace test results."""

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import re
import subprocess
import sys
import time
import xml.etree.ElementTree as ET

SCHEMA_VERSION = 1
PHASES = ("compile", "test", "doc")
DOC_RESULT = re.compile(r"test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored")


def write_json(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".tmp")
    temporary.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    os.replace(temporary, path)


def measure(args):
    command = args.command
    if command and command[0] == "--":
        command = command[1:]
    if not command:
        raise ValueError("measure requires a command")
    log_path = Path(args.directory) / (args.phase + ".log")
    log_path.parent.mkdir(parents=True, exist_ok=True)
    started = time.time()
    tick = time.monotonic()
    code = 127
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="backslashreplace")
    with log_path.open("w", encoding="utf-8", errors="replace") as log:
        try:
            process = subprocess.Popen(
                command,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                encoding="utf-8",
                errors="replace",
                bufsize=1,
            )
            assert process.stdout is not None
            with process.stdout:
                for line in process.stdout:
                    log.write(line)
                    log.flush()
                    sys.stdout.write(line)
                    sys.stdout.flush()
            code = process.wait()
        except OSError as error:
            message = f"Could not start {command[0]}: {error}\n"
            log.write(message)
            print(message, end="", file=sys.stderr)
    duration = round(time.monotonic() - tick, 3)
    write_json(
        Path(args.directory) / (args.phase + ".json"),
        {
            "phase": args.phase,
            "command": command,
            "started_unix_seconds": round(started, 3),
            "duration_seconds": duration,
            "exit_code": code,
        },
    )
    return code if code >= 0 else 128 - code


def load_phase(directory, name):
    path = Path(directory) / (name + ".json")
    if not path.exists():
        return {"phase": name, "status": "missing", "duration_seconds": None, "exit_code": None}
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
        if data["phase"] != name or data["duration_seconds"] < 0:
            raise ValueError("invalid phase content")
        return {**data, "status": "passed" if data["exit_code"] == 0 else "failed"}
    except (ValueError, KeyError, TypeError) as error:
        return {"phase": name, "status": "invalid", "duration_seconds": None, "exit_code": None, "error": str(error)}


def parse_junit(path):
    path = Path(path)
    if not path.is_file():
        return [], "JUnit report missing"
    try:
        root = ET.parse(path).getroot()
        tests = []
        for suite in root.iter("testsuite"):
            suite_name = suite.get("name", "")
            for case in suite.findall("testcase"):
                name = case.get("name", "")
                identifier = f"{suite_name}::{case.get('classname', '')}::{name}"
                seconds = float(case.get("time", "0"))
                if not math.isfinite(seconds) or seconds < 0 or not name:
                    raise ValueError("invalid JUnit testcase")
                outcome = "failed" if case.find("failure") is not None or case.find("error") is not None else "skipped" if case.find("skipped") is not None else "passed"
                tests.append({"id": identifier, "duration_seconds": seconds, "outcome": outcome})
        if not tests:
            raise ValueError("JUnit report contains no tests")
        return tests, None
    except (ET.ParseError, ValueError, OSError) as error:
        return [], f"JUnit report invalid: {error}"


def print_junit_failures(path):
    try:
        root = ET.parse(path).getroot()
    except (ET.ParseError, OSError):
        return
    for suite in root.iter("testsuite"):
        for case in suite.findall("testcase"):
            failure = case.find("failure")
            if failure is None:
                failure = case.find("error")
            if failure is None:
                continue
            identifier = f"{suite.get('name', '')}::{case.get('classname', '')}::{case.get('name', '')}"
            print(f"Failed test: {identifier}", file=sys.stderr)
            message = " ".join((failure.get("message") or "").split())
            if message:
                print(f"  Failure: {message[:500]}", file=sys.stderr)
            detail = " ".join((failure.text or "").split())
            if detail and detail != message:
                print(f"  Failure output: {detail[-1000:]}", file=sys.stderr)
            stderr = case.findall(".//system-err")
            if stderr:
                for line in (stderr[-1].text or "").strip().splitlines()[-12:]:
                    print(f"  stderr: {line[:300]}", file=sys.stderr)


def parse_doc_results(path, phase):
    path = Path(path)
    if not path.is_file():
        return None
    log = path.read_text(encoding="utf-8", errors="replace")
    matches = DOC_RESULT.findall(log)
    if not matches:
        if phase["status"] == "passed" and re.fullmatch(r"\s*Finished `test` profile \[[^\r\n]+\] target\(s\) in [^\r\n]+\s*", log):
            return {"passed": 0, "failed": 0, "ignored": 0}
        return None
    return {
        "passed": sum(int(match[1]) for match in matches),
        "failed": sum(int(match[2]) for match in matches),
        "ignored": sum(int(match[3]) for match in matches),
    }


def build_report(directory, junit, metadata, min_doc_tests=0):
    phases = {name: load_phase(directory, name) for name in PHASES}
    tests, junit_error = parse_junit(junit)
    doc_results = parse_doc_results(Path(directory) / "doc.log", phases["doc"])
    diagnostics = []
    if junit_error:
        diagnostics.append(junit_error)
    if doc_results is None:
        diagnostics.append("Doc test results missing")
    elif doc_results["passed"] < min_doc_tests:
        diagnostics.append(f"Expected at least {min_doc_tests} passing doc tests, found {doc_results['passed']}")
    if any(phase["status"] in ("missing", "invalid") for phase in phases.values()):
        diagnostics.append("One or more phase records are missing or invalid")
    for name, phase in phases.items():
        if phase["status"] == "failed":
            diagnostics.append(f"{name} exited with {phase['exit_code']}")
    if tests and any(test["outcome"] == "failed" for test in tests):
        diagnostics.append("One or more nextest tests failed")
    if doc_results is not None and doc_results["failed"]:
        diagnostics.append("One or more doc tests failed")
    passed = not diagnostics and all(phase["status"] == "passed" for phase in phases.values())
    scope_hash = hashlib.sha256("\n".join(sorted(test["id"] for test in tests)).encode("utf-8")).hexdigest() if tests else None
    return {
        "schema_version": SCHEMA_VERSION,
        **metadata,
        "phases": phases,
        "tests": tests,
        "test_count": len(tests) if junit_error is None else None,
        "test_counts": ({outcome: sum(test["outcome"] == outcome for test in tests)
                         for outcome in ("passed", "failed", "skipped")}
                        if junit_error is None else None),
        "test_ids_sha256": scope_hash,
        "doc_tests": doc_results,
        "status": "passed" if passed else "failed" if any(phase["status"] == "failed" for phase in phases.values()) else "incomplete",
        "diagnostics": diagnostics,
    }


def environment_command(args):
    target = args.target
    if target == "native":
        version = subprocess.run(["rustc", "-vV"], capture_output=True, text=True, check=True).stdout
        target = next((line.removeprefix("host: ").strip() for line in version.splitlines()
                       if line.startswith("host: ")), None)
    metadata = {
        "checkout_commit": subprocess.run(["git", "rev-parse", "HEAD"], capture_output=True, text=True, check=True).stdout.strip(),
        "rustc": subprocess.run(["rustc", "--version"], capture_output=True, text=True, check=True).stdout.strip(),
        "cargo": subprocess.run(["cargo", "--version"], capture_output=True, text=True, check=True).stdout.strip(),
        "platform": args.platform,
        "target": target,
        "runner_os": os.environ.get("RUNNER_OS", sys.platform),
        "runner_arch": os.environ.get("RUNNER_ARCH"),
        "runner_image": os.environ.get("ImageOS"),
    }
    write_json(Path(args.directory) / "environment.json", metadata)
    return 0


def report_command(args):
    metadata = {
        "run_id": os.environ.get("GITHUB_RUN_ID", "local"),
        "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT", "local"),
        "commit": os.environ.get("GITHUB_SHA", "local"),
        "workflow": os.environ.get("GITHUB_WORKFLOW", "local"),
        "platform": args.platform,
        "target": None if args.target == "native" else args.target,
        "runner_os": os.environ.get("RUNNER_OS", sys.platform),
        "runner_arch": os.environ.get("RUNNER_ARCH", "unknown"),
        "runner_image": os.environ.get("ImageOS", "unknown"),
        "toolchain": os.environ.get("RUSTUP_TOOLCHAIN", "unknown"),
    }
    environment = Path(args.directory) / "environment.json"
    if environment.is_file():
        recorded = json.loads(environment.read_text(encoding="utf-8"))
        metadata.update({key: recorded.get(key) for key in ("checkout_commit", "rustc", "cargo", "runner_os", "runner_arch", "runner_image")})
        if args.target == "native":
            metadata["target"] = recorded.get("target")
    result = build_report(args.directory, args.junit, metadata, args.min_doc_tests)
    write_json(args.output, result)
    print("Rust test evidence: " + result["status"])
    return 0


def enforce_command(args):
    result = build_report(args.directory, args.junit, {}, args.min_doc_tests)
    if result["status"] != "passed":
        print("Rust tests did not pass: " + "; ".join(result.get("diagnostics", [])), file=sys.stderr)
        if any(test["outcome"] == "failed" for test in result["tests"]):
            print_junit_failures(args.junit)
        return 1
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="action", required=True)
    environment_parser = commands.add_parser("environment")
    environment_parser.add_argument("--directory", required=True)
    environment_parser.add_argument("--platform", required=True)
    environment_parser.add_argument("--target", required=True)
    measure_parser = commands.add_parser("measure")
    measure_parser.add_argument("--directory", required=True)
    measure_parser.add_argument("--phase", choices=PHASES, required=True)
    measure_parser.add_argument("command", nargs=argparse.REMAINDER)
    report_parser = commands.add_parser("report")
    report_parser.add_argument("--directory", required=True)
    report_parser.add_argument("--junit", required=True)
    report_parser.add_argument("--output", required=True)
    report_parser.add_argument("--platform", required=True)
    report_parser.add_argument("--target", required=True)
    report_parser.add_argument("--min-doc-tests", type=int, default=0)
    enforce_parser = commands.add_parser("enforce")
    enforce_parser.add_argument("--directory", required=True)
    enforce_parser.add_argument("--junit", required=True)
    enforce_parser.add_argument("--min-doc-tests", type=int, default=0)
    args = parser.parse_args()
    return {"environment": environment_command, "measure": measure, "report": report_command, "enforce": enforce_command}[args.action](args)


if __name__ == "__main__":
    sys.exit(main())
