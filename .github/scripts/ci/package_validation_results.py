"""Record actual package command outcomes without rerunning validation."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import time


def run(directory, phase, command):
    if not command:
        raise ValueError("validation command is required")
    started = time.monotonic()
    code = 127
    try:
        code = subprocess.run(command, check=False).returncode
    finally:
        data = {"phase": phase, "exit_code": code,
                "duration_seconds": round(time.monotonic() - started, 3),
                "commit": os.environ.get("GITHUB_SHA"),
                "run_id": os.environ.get("GITHUB_RUN_ID"),
                "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT")}
        try:
            directory.mkdir(parents=True, exist_ok=True)
            (directory / f"{phase}.json").write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
        except OSError as error:
            print(f"Package validation evidence unavailable: {error}", file=sys.stderr)
    return code if code >= 0 else 128 - code


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="action", required=True)
    run_parser = subparsers.add_parser("run")
    run_parser.add_argument("--directory", required=True, type=Path)
    run_parser.add_argument("--phase", required=True)
    run_parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    return run(args.directory, args.phase, args.command[1:] if args.command[:1] == ["--"] else args.command)


if __name__ == "__main__":
    raise SystemExit(main())
