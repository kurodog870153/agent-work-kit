"""Build the two self-contained Work and Tooling release ZIP assets and outer checksums."""

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

from release_package_integrity import create_manifest, verify_tree, verify_zip

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "version-guard"))
from version_guard import validate_lock


PLATFORMS = {
    "macos-arm64": {
        "runner_label": "macos-latest",
        "target": "aarch64-apple-darwin",
        "work-binary": "bin/work",
        "tooling_binary": "bin/tooling",
        "installer": "os-scripts/mac/install-work-from-package.command",
        "modules": (
            "os-scripts/mac/installer/cli/parse-options.sh",
            "os-scripts/mac/installer/install/install-core.sh",
            "os-scripts/mac/installer/install/work-install.sh",
            "os-scripts/mac/installer/install/tooling-install.sh",
            "os-scripts/mac/installer/install/migration-cleanup.sh",
            "os-scripts/mac/installer/state/state-core.js",
            "os-scripts/mac/installer/state/work-state.js",
            "os-scripts/mac/installer/state/tooling-state.js",
            "os-scripts/mac/installer/target/preflight.sh",
            "os-scripts/mac/installer/transaction/work.sh",
            "os-scripts/mac/installer/transaction/tooling.sh",
            "os-scripts/mac/installer/global-agents/policy.sh",
            "os-scripts/mac/installer/global-agents/transaction.sh",
            "os-scripts/mac/installer/package/verify-integrity.sh",
        ),
    },
    "windows-x64-msvc": {
        "runner_label": "windows-latest",
        "target": "x86_64-pc-windows-msvc",
        "work-binary": "bin/work.exe",
        "tooling_binary": "bin/tooling.exe",
        "installer": "os-scripts/windows/install-work-from-package.bat",
        "modules": (
            "os-scripts/windows/installer/cli/parse-options.ps1",
            "os-scripts/windows/installer/install/install-core.ps1",
            "os-scripts/windows/installer/install/work-install.ps1",
            "os-scripts/windows/installer/install/tooling-install.ps1",
            "os-scripts/windows/installer/install/migration-cleanup.ps1",
            "os-scripts/windows/installer/lock/mutex.ps1",
            "os-scripts/windows/installer/state/state-core.ps1",
            "os-scripts/windows/installer/state/work-state.ps1",
            "os-scripts/windows/installer/state/tooling-state.ps1",
            "os-scripts/windows/installer/target/preflight.ps1",
            "os-scripts/windows/installer/transaction/work.ps1",
            "os-scripts/windows/installer/transaction/tooling.ps1",
            "os-scripts/windows/installer/global-agents/policy.ps1",
            "os-scripts/windows/installer/global-agents/transaction.ps1",
            "os-scripts/windows/installer/package/verify-integrity.ps1",
        ),
    },
}
DOWNLOADERS = {
    "macos-arm64": "os-scripts/mac/install-work-from-github-release.command",
    "windows-x64-msvc": "os-scripts/windows/install-work-from-github-release.bat",
}
DOWNLOADER_TAG_PLACEHOLDER = b"__WORK_RELEASE_TAG__"


def checked_version(repository):
    with (repository / "rust/Cargo.toml").open("rb") as stream:
        return validate_lock(repository, tomllib.load(stream))


def checked_workspace_version(repository):
    with (repository / "rust/Cargo.toml").open("rb") as stream:
        return tomllib.load(stream)["workspace"]["package"]["version"]


def tracked_skill_files(repository):
    result = subprocess.run(
        ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard", "--", "skills/work", "skills/tooling"],
        cwd=repository,
        check=True,
        capture_output=True,
    )
    paths = [Path(name.decode("utf-8")) for name in result.stdout.split(b"\0") if name]
    paths = [path for path in paths if path.suffix in {".md", ".yaml", ".yml"}]
    if not any(path == Path("skills/work/SKILL.md") for path in paths):
        raise ValueError("Work skill entry is missing")
    if not any(path == Path("skills/tooling/SKILL.md") for path in paths):
        raise ValueError("Tooling skill entry is missing")
    return paths


def copy_asset(repository, stage, source, destination):
    source_path = repository / source
    if source_path.is_symlink() or not source_path.is_file():
        raise ValueError(f"release source is missing or is a symlink: {source}")
    target_path = stage / destination
    target_path.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source_path, target_path)


def build_package(repository, output, platform, binary, tooling_binary, commit, tested_environment):
    config = PLATFORMS[platform]
    required = ("runner_label", "runner_os", "cpu_architecture", "runner_image_name",
                "runner_image_version", "os_name", "os_version", "os_build")
    if any(not tested_environment.get(field) for field in required):
        raise ValueError("tested environment is missing a required field")
    if tested_environment["runner_label"] != config["runner_label"]:
        raise ValueError("runner label does not match package platform")
    version = checked_version(repository)
    workspace_version = checked_workspace_version(repository)
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
        platform_dir = "mac" if platform == "macos-arm64" else "windows"
        for path in config["modules"]:
            module_path = Path(path).relative_to(f"os-scripts/{platform_dir}/installer")
            copy_asset(repository, stage, Path(path), Path("installer") / module_path)
        copy_asset(repository, stage, Path("os-scripts/common/installer/options.tsv"), Path("installer/cli/options.tsv"))
        copy_asset(repository, stage, Path("os-scripts/common/installer/install-state.json"), Path("installer/state/install-state.json"))
        if platform == "macos-arm64":
            lock_helper = stage / "bin/work-install-lock"
            lock_helper.parent.mkdir(parents=True, exist_ok=True)
            subprocess.run(
                ["cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-o", str(lock_helper),
                 str(repository / "os-scripts/mac/installer/lock/native-lock.c")],
                check=True,
            )
        for source, destination in ((binary, config["work-binary"]), (tooling_binary, config["tooling_binary"])):
            if source.is_symlink() or not source.is_file():
                raise ValueError(f"release binary is missing or is a symlink: {source}")
            binary_target = stage / destination
            binary_target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, binary_target)
        info = {
            "version": version,
            "work_version": workspace_version,
            "tooling_version": workspace_version,
            "tag": tag,
            "commit": commit,
            "target": config["target"],
            "license": "PolyForm Noncommercial License 1.0.0",
            "tested_environment": tested_environment,
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
                executable = relative in (config["work-binary"], config["tooling_binary"], "bin/work-install-lock") or relative.endswith(".command") or relative.endswith(".sh")
                zip_info.external_attr = (stat.S_IFREG | (0o755 if executable else 0o644)) << 16
                zip_info.compress_type = zipfile.ZIP_DEFLATED
                archive.writestr(zip_info, path.read_bytes(), compress_type=zipfile.ZIP_DEFLATED, compresslevel=9)
    verify_zip(target)
    return target


def build_downloaders(repository, output):
    version = checked_version(repository)
    tag = f"v{version}"
    sources = []
    for platform, relative in DOWNLOADERS.items():
        source = repository / relative
        if source.is_symlink() or not source.is_file():
            raise ValueError(f"Release downloader is missing or unsafe: {relative}")
        data = source.read_bytes()
        marker = (b"release_tag=" if platform == "macos-arm64" else b"$releaseTag = '")
        if data.count(marker + DOWNLOADER_TAG_PLACEHOLDER) != 1 or data.count(DOWNLOADER_TAG_PLACEHOLDER) != 1 or data.startswith(b"\xef\xbb\xbf"):
            raise ValueError(f"Release downloader tag placeholder is missing or ambiguous: {relative}")
        if platform == "macos-arm64" and b"\r" in data:
            raise ValueError("macOS downloader must use LF")
        if platform == "windows-x64-msvc" and data.count(b"\r\n") != data.count(b"\n"):
            raise ValueError("Windows downloader must use CRLF")
        destination = output / source.name
        if destination.exists():
            raise ValueError(f"Release downloader already exists: {destination}")
        sources.append((destination, data.replace(DOWNLOADER_TAG_PLACEHOLDER, tag.encode("ascii"))))
    output.mkdir(parents=True, exist_ok=True)
    for destination, data in sources:
        destination.write_bytes(data)
    return [destination for destination, _ in sources]


def write_outer_checksums(repository, output):
    version = checked_version(repository)
    names = [f"work-v{version}-{platform}.zip" for platform in sorted(PLATFORMS)]
    names += [Path(DOWNLOADERS[platform]).name for platform in sorted(DOWNLOADERS)]
    target = output / "SHA256SUMS.txt"
    if target.exists():
        raise ValueError(f"outer SHA256SUMS.txt already exists: {target}")
    lines = []
    for name in names:
        path = output / name
        if name.endswith(".zip"):
            verify_zip(path)
        elif not path.is_file():
            raise ValueError(f"Release downloader is missing: {path}")
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        lines.append(f"{digest}  {name}\n")
    target.write_bytes("".join(lines).encode("ascii"))
    return target


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("package", "downloaders", "checksums"))
    parser.add_argument("--repository", type=Path, default=Path(__file__).resolve().parents[3])
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--platform", choices=sorted(PLATFORMS))
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--tooling-binary", type=Path)
    parser.add_argument("--commit")
    parser.add_argument("--runner-os")
    parser.add_argument("--runner-label")
    parser.add_argument("--runner-arch")
    parser.add_argument("--runner-image")
    parser.add_argument("--runner-image-version")
    parser.add_argument("--os-name")
    parser.add_argument("--os-version")
    parser.add_argument("--os-build")
    args = parser.parse_args()
    try:
        if args.action == "package":
            if not all((args.platform, args.binary, args.tooling_binary, args.commit, args.runner_label, args.runner_os, args.runner_arch,
                        args.runner_image, args.runner_image_version, args.os_name, args.os_version, args.os_build)):
                parser.error("package requires platform, both binaries, commit and all tested environment fields")
            tested_environment = {
                "runner_label": args.runner_label,
                "runner_os": args.runner_os,
                "cpu_architecture": args.runner_arch,
                "runner_image_name": args.runner_image,
                "runner_image_version": args.runner_image_version,
                "os_name": args.os_name,
                "os_version": args.os_version,
                "os_build": args.os_build,
            }
            result = build_package(args.repository, args.output, args.platform, args.binary, args.tooling_binary, args.commit, tested_environment)
        elif args.action == "downloaders":
            result = build_downloaders(args.repository, args.output)
        else:
            result = write_outer_checksums(args.repository, args.output)
    except (OSError, ValueError, subprocess.CalledProcessError, zipfile.BadZipFile) as error:
        print(f"Release packaging failed: {error}", file=sys.stderr)
        return 1
    print(result)
    return 0


if __name__ == "__main__":
    sys.exit(main())
