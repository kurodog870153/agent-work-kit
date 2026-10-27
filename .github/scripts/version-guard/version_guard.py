"""Validate the project version before a pull request can merge."""

import argparse
import glob
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


def project_version(root):
    version = (root / "VERSION").read_text(encoding="utf-8").strip()
    parse_version(version)
    return version


def validate_workspace(root, baseline_version):
    rust = root / "rust"
    manifest = tomllib.loads((rust / "Cargo.toml").read_text(encoding="utf-8"))
    version = project_version(root)
    baseline = baseline_version.strip()
    if compare_versions(version, baseline) <= 0:
        raise ValueError(f"PR version {version} must be greater than main version {baseline}")

    validate_lock(root, manifest)
    return version


def validate_lock(root, manifest):
    rust = root / "rust"
    version = project_version(root)
    cargo_version = workspace_version(manifest)
    parse_version(cargo_version)

    members = manifest["workspace"]["members"]
    names = set()
    expected_versions = {}
    for member in members:
        paths = sorted(Path(path) for path in glob.glob(str(rust / member)))
        if not paths:
            raise ValueError(f"workspace member pattern matches no directories: {member}")
        for path in paths:
            package = tomllib.loads((path / "Cargo.toml").read_text(encoding="utf-8"))["package"]
            if package.get("version") != {"workspace": True}:
                raise ValueError(f"{path.relative_to(rust)} must inherit workspace.package.version")
            name = package["name"]
            if name in names:
                raise ValueError(f"duplicate workspace package: {name}")
            names.add(name)
            expected_versions[name] = cargo_version

    lock = tomllib.loads((rust / "Cargo.lock").read_text(encoding="utf-8"))
    locked = {package["name"]: package["version"] for package in lock["package"] if package["name"] in names}
    if set(locked) != names or any(value != expected_versions[name] for name, value in locked.items()):
        raise ValueError("Cargo.lock workspace versions do not match their package versions")
    return version


def baseline_version(root, base_ref):
    result = subprocess.run(
        ["git", "show", f"{base_ref}:VERSION"],
        cwd=root,
        capture_output=True,
        text=True,
    )
    if result.returncode == 0:
        return result.stdout
    if not (
        "path 'VERSION' does not exist" in result.stderr
        or "path 'VERSION' exists on disk, but not in" in result.stderr
    ):
        raise subprocess.CalledProcessError(result.returncode, result.args, stderr=result.stderr)
    # The first migration PR compares against main's former version source.
    manifest = subprocess.run(
        ["git", "show", f"{base_ref}:rust/Cargo.toml"],
        cwd=root,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return workspace_version(tomllib.loads(manifest))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[3])
    parser.add_argument("--base-ref", default="refs/remotes/origin/main")
    args = parser.parse_args()
    try:
        baseline = baseline_version(args.root, args.base_ref)
        version = validate_workspace(args.root, baseline)
    except (KeyError, OSError, subprocess.CalledProcessError, tomllib.TOMLDecodeError, ValueError) as error:
        print(f"Version guard failed: {error}", file=sys.stderr)
        return 1
    print(f"Version guard passed: {version}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
