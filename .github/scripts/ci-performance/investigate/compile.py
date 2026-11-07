#!/usr/bin/env python3
"""Preserve Cargo unit timings from the existing reusable test compilation."""

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys


def run(args):
    directory = Path(args.directory)
    phase = json.loads((directory / "compile.json").read_text(encoding="utf-8"))
    if phase.get("phase") != "compile" or "--timings" not in phase.get("command", []):
        raise ValueError("compile phase did not request Cargo timings")
    source = Path("target/cargo-timings/cargo-timing.html")
    available = source.is_file()
    if available:
        shutil.copyfile(source, directory / "compile-timings.html")
    result = {
        "schema_version": 1,
        "config_id": "rust-reused-compile-timing-v1",
        "platform": "windows" if sys.platform == "win32" else "macos" if sys.platform == "darwin" else sys.platform,
        "target": args.target or "native",
        "run_id": os.environ.get("GITHUB_RUN_ID", "local"),
        "commit": os.environ.get("GITHUB_SHA", "local"),
        "runner_image": os.environ.get("ImageOS", "unknown"),
        "toolchain": os.environ.get("RUSTUP_TOOLCHAIN", "unknown"),
        "rustc_version": subprocess.run(["rustc", "-vV"], capture_output=True, text=True, check=True).stdout.strip(),
        "compile_seconds": phase["duration_seconds"],
        "compile_exit_code": phase["exit_code"],
        "cargo_timings_available": available,
        "sample_scope": "one reusable test compilation in this run; compare repeated GitHub runs",
    }
    (directory / "compile-investigation.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    lines = [f"## Rust compile investigation: {result['platform']}", "",
             f"Commit: `{result['commit']}`; target: `{result['target']}`; runner image: `{result['runner_image']}`.",
             "", "| Compile seconds | Exit | Cargo unit timings |", "| ---: | ---: | --- |",
             f"| {result['compile_seconds']} | {result['compile_exit_code']} | {'saved' if available else 'missing'} |",
             "", "This run compiles tests once and reuses its artifacts. Compare multiple runs on each platform to assess runner variation. Cargo unit timings show compilation and concurrency, but do not independently isolate compiler, linker, or debug-symbol costs."]
    markdown = "\n".join(lines) + "\n"
    (directory / "compile-summary.md").write_text(markdown, encoding="utf-8")
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as handle:
            handle.write(markdown)
    print(markdown)
    return 0 if available else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", required=True)
    parser.add_argument("--target", default="")
    return run(parser.parse_args())


if __name__ == "__main__":
    sys.exit(main())
