"""Build the two self-contained Work release ZIP assets and outer checksums."""

import argparse
import hashlib
import json
import shutil
import stat
import subprocess
import sys
import tempfile
import tomllib
import zipfile
from pathlib import Path

from release_integrity import create_manifest, verify_tree, verify_zip

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "version-guard"))
from version_guard import validate_lock


PLATFORMS = {
    "macos-arm64": {
        "target": "aarch64-apple-darwin",
        "binary": "bin/work",
        "installer": "os-scripts/mac/install-release.command",
        "modules": (
            "os-scripts/mac/installer/global-agents.sh",
            "os-scripts/mac/installer/work-core.sh",
            "os-scripts/mac/installer/verify-package.sh",
        ),
    },
    "windows-x64-msvc": {
        "target": "x86_64-pc-windows-msvc",
        "binary": "bin/work.exe",
        "installer": "os-scripts/windows/install-release.bat",
        "modules": (
            "os-scripts/windows/installer/global-agents.bat",
            "os-scripts/windows/installer/work-core.bat",
            "os-scripts/windows/installer/verify-package.ps1",
        ),
    },
}


def checked_version(repository):
    with (repository / "rust/Cargo.toml").open("rb") as stream:
        return validate_lock(repository, tomllib.load(stream))


def tracked_skill_files(repository):
    result = subprocess.run(
        ["git", "ls-files", "-z", "--", "skills/work"],
        cwd=repository,
        check=True,
        capture_output=True,
    )
    paths = [Path(name.decode("utf-8")) for name in result.stdout.split(b"\0") if name]
    if not paths:
        raise ValueError("no tracked Work skill files found")
    return paths


def copy_asset(repository, stage, source, destination):
    source_path = repository / source
    if source_path.is_symlink() or not source_path.is_file():
        raise ValueError(f"release source is missing or is a symlink: {source}")
    target_path = stage / destination
    target_path.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source_path, target_path)


def build_package(repository, output, platform, binary, commit, runner):
    config = PLATFORMS[platform]
    version = checked_version(repository)
    tag = f"v{version}"
    name = f"work-{tag}-{platform}.zip"
    target = output / name
    if target.exists():
        raise ValueError(f"release ZIP already exists: {target}")
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".work-release-", dir=output) as temporary:
        stage = Path(temporary)
        for path in tracked_skill_files(repository):
            copy_asset(repository, stage, path, path)
        for path in ("AGENTS.md", "LICENSE"):
            copy_asset(repository, stage, Path(path), Path(path))
        copy_asset(repository, stage, Path("docs/release/README.md"), Path("README.md"))
        copy_asset(repository, stage, Path(config["installer"]), Path(config["installer"]).name)
        for path in config["modules"]:
            copy_asset(repository, stage, Path(path), Path("installer") / Path(path).name)
        if binary.is_symlink() or not binary.is_file():
            raise ValueError(f"release binary is missing or is a symlink: {binary}")
        binary_target = stage / config["binary"]
        binary_target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(binary, binary_target)
        info = {
            "version": version,
            "tag": tag,
            "commit": commit,
            "target": config["target"],
            "license": "PolyForm Noncommercial License 1.0.0",
            "e2e_runner": runner,
        }
        (stage / "release-info.json").write_bytes(
            (json.dumps(info, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
        )
        create_manifest(stage)
        verify_tree(stage)
        with zipfile.ZipFile(target, "x", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
            for path in sorted(stage.rglob("*")):
                if not path.is_file():
                    continue
                relative = path.relative_to(stage).as_posix()
                zip_info = zipfile.ZipInfo(relative, date_time=(1980, 1, 1, 0, 0, 0))
                zip_info.create_system = 3
                executable = relative == config["binary"] or relative.endswith(".command") or relative.endswith(".sh")
                zip_info.external_attr = (stat.S_IFREG | (0o755 if executable else 0o644)) << 16
                zip_info.compress_type = zipfile.ZIP_DEFLATED
                archive.writestr(zip_info, path.read_bytes(), compress_type=zipfile.ZIP_DEFLATED, compresslevel=9)
    verify_zip(target)
    return target


def write_outer_checksums(repository, output):
    version = checked_version(repository)
    names = [f"work-v{version}-{platform}.zip" for platform in sorted(PLATFORMS)]
    target = output / "SHA256SUMS.txt"
    if target.exists():
        raise ValueError(f"outer SHA256SUMS.txt already exists: {target}")
    lines = []
    for name in names:
        path = output / name
        verify_zip(path)
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        lines.append(f"{digest}  {name}\n")
    target.write_bytes("".join(lines).encode("ascii"))
    return target


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("package", "checksums"))
    parser.add_argument("--repository", type=Path, default=Path(__file__).resolve().parents[3])
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--platform", choices=sorted(PLATFORMS))
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--commit")
    parser.add_argument("--runner-os")
    parser.add_argument("--runner-arch")
    parser.add_argument("--runner-image")
    parser.add_argument("--os-version")
    args = parser.parse_args()
    try:
        if args.action == "package":
            if not all((args.platform, args.binary, args.commit, args.runner_os, args.runner_arch, args.runner_image, args.os_version)):
                parser.error("package requires platform, binary, commit and all runner fields")
            runner = {
                "os": args.runner_os,
                "arch": args.runner_arch,
                "image": args.runner_image,
                "os_version": args.os_version,
            }
            result = build_package(args.repository, args.output, args.platform, args.binary, args.commit, runner)
        else:
            result = write_outer_checksums(args.repository, args.output)
    except (OSError, ValueError, subprocess.CalledProcessError, zipfile.BadZipFile) as error:
        print(f"Release packaging failed: {error}", file=sys.stderr)
        return 1
    print(result)
    return 0


if __name__ == "__main__":
    sys.exit(main())
