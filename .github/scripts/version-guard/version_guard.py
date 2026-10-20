"""Validate the Rust workspace version before a pull request can merge."""

import argparse
import re
import subprocess
import sys
import tomllib
from pathlib import Path


SEMVER = re.compile(
    r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"
    r"(?:-((?:0|[1-9][0-9]*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*)"
    r"(?:\.(?:0|[1-9][0-9]*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*))*))?"
    r"(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$"
)


def parse_version(value):
    if not isinstance(value, str):
        raise ValueError("workspace version must be a SemVer string")
    match = SEMVER.fullmatch(value)
    if not match:
        raise ValueError(f"invalid SemVer: {value!r}")
    return tuple(int(match.group(i)) for i in range(1, 4)), match.group(4)


def compare_versions(candidate, baseline):
    left, left_pre = parse_version(candidate)
    right, right_pre = parse_version(baseline)
    if left != right:
        return (left > right) - (left < right)
    if left_pre is None or right_pre is None:
        return (left_pre is None) - (right_pre is None)
    for a, b in zip(left_pre.split("."), right_pre.split(".")):
        a_numeric, b_numeric = a.isascii() and a.isdigit(), b.isascii() and b.isdigit()
        if a_numeric and b_numeric:
            result = (int(a) > int(b)) - (int(a) < int(b))
        elif a_numeric != b_numeric:
            result = -1 if a_numeric else 1
        else:
            result = (a > b) - (a < b)
        if result:
            return result
    return (len(left_pre.split(".")) > len(right_pre.split("."))) - (
        len(left_pre.split(".")) < len(right_pre.split("."))
    )


def workspace_version(manifest):
    return manifest["workspace"]["package"]["version"]


def validate_workspace(root, baseline_manifest):
    rust = root / "rust"
    manifest = tomllib.loads((rust / "Cargo.toml").read_text(encoding="utf-8"))
    version = workspace_version(manifest)
    baseline = workspace_version(tomllib.loads(baseline_manifest))
    if compare_versions(version, baseline) <= 0:
        raise ValueError(f"PR version {version} must be greater than main version {baseline}")

    validate_lock(root, manifest)
    return version


def validate_lock(root, manifest):
    rust = root / "rust"
    version = workspace_version(manifest)
    parse_version(version)

    members = manifest["workspace"]["members"]
    names = set()
    for member in members:
        package = tomllib.loads((rust / member / "Cargo.toml").read_text(encoding="utf-8"))["package"]
        if package.get("version") != {"workspace": True}:
            raise ValueError(f"{member} must inherit workspace.package.version")
        name = package["name"]
        if name in names:
            raise ValueError(f"duplicate workspace package: {name}")
        names.add(name)

    lock = tomllib.loads((rust / "Cargo.lock").read_text(encoding="utf-8"))
    locked = {package["name"]: package["version"] for package in lock["package"] if package["name"] in names}
    if set(locked) != names or any(value != version for value in locked.values()):
        raise ValueError(f"Cargo.lock workspace versions do not all match {version}")
    return version


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[3])
    parser.add_argument("--base-ref", default="refs/remotes/origin/main")
    args = parser.parse_args()
    baseline = subprocess.run(
        ["git", "show", f"{args.base_ref}:rust/Cargo.toml"],
        cwd=args.root,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    try:
        version = validate_workspace(args.root, baseline)
    except (KeyError, OSError, tomllib.TOMLDecodeError, ValueError) as error:
        print(f"Version guard failed: {error}", file=sys.stderr)
        return 1
    print(f"Version guard passed: {version}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
