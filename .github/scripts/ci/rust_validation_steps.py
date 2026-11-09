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
    for package in ("work-cli", "tooling-cli"):
        run_checked("cargo", "build", "--release", "--locked", "-p", package)


def smoke_test_binaries():
    suffix = ".exe" if sys.platform == "win32" else ""
    for binary in ("work", "tooling"):
        run_checked(str(Path("target/release") / f"{binary}{suffix}"), "--help")


def verify_host_activation():
    suffix = ".exe" if sys.platform == "win32" else ""
    run_checked(
        sys.executable,
        "-B",
        "../.github/scripts/tests/release/test_host_activation_modes.py",
        "--work",
        f"target/release/work{suffix}",
        "--tooling",
        f"target/release/tooling{suffix}",
        "--evidence",
        ".ci-perf/host-activation.json",
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("setup", "build", "smoke", "activation"))
    args = parser.parse_args()
    {"setup": set_up_toolchain, "build": build_release_binaries, "smoke": smoke_test_binaries, "activation": verify_host_activation}[args.action]()


if __name__ == "__main__":
    main()
