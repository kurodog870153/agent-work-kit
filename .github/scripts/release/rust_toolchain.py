"""Read the pinned Rust toolchain from the workspace manifest."""

import argparse
import re
import tomllib
from pathlib import Path


REPOSITORY = Path(__file__).resolve().parents[3]


def rust_version(repository=REPOSITORY):
    with (Path(repository) / "rust/Cargo.toml").open("rb") as stream:
        version = tomllib.load(stream)["workspace"]["package"]["rust-version"]
    if not isinstance(version, str) or not re.fullmatch(r"\d+\.\d+\.\d+", version):
        raise ValueError("workspace rust-version must be an exact stable Rust release")
    return version


def rust_toolchain(target=None, repository=REPOSITORY):
    version = rust_version(repository)
    if target is None:
        return version
    if not re.fullmatch(r"[A-Za-z0-9_-]+", target):
        raise ValueError("invalid Rust target")
    return f"{version}-{target}"


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target")
    args = parser.parse_args()
    print(rust_toolchain(args.target))
