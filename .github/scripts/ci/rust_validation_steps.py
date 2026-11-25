"""Shared Rust validation steps for macOS and Windows CI."""

import argparse
import os
import subprocess
import sys
from pathlib import Path


def run_checked(*command):
    subprocess.run(command, check=True)


def set_up_toolchain():
    toolchain = os.environ["RUSTUP_TOOLCHAIN"]
    run_checked("rustup", "toolchain", "install", toolchain)
    run_checked("rustup", "component", "add", "clippy", "rustfmt", "--toolchain", toolchain)


def build_release_binaries():
    for package in ("work-cli", "skill-cli"):
        run_checked("cargo", "build", "--release", "--locked", "-p", package)


def smoke_test_binaries():
    suffix = ".exe" if sys.platform == "win32" else ""
    for binary in ("work", "skill"):
        run_checked(str(Path("target/release") / f"{binary}{suffix}"), "--help")


def simulate_host():
    suffix = ".exe" if sys.platform == "win32" else ""
    run_checked(
        sys.executable,
        "-B",
        "../.github/scripts/release/host_simulation.py",
        "--work",
        f"target/release/work{suffix}",
        "--skill",
        f"target/release/skill{suffix}",
        "--install",
        "../skills",
        "--evidence",
        ".ci-results/binary-host-simulation.json",
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("setup", "build", "smoke", "simulation"))
    args = parser.parse_args()
    {"setup": set_up_toolchain, "build": build_release_binaries, "smoke": smoke_test_binaries, "simulation": simulate_host}[args.action]()


if __name__ == "__main__":
    main()
