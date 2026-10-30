"""Exercise the final extracted release ZIP on its intended native runner."""

import argparse
import hashlib
import json
import os
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import zipfile
from pathlib import Path

from release_package_integrity import verify_tree, verify_zip
from rust_toolchain import rust_toolchain


CLI_OPTIONS = ("--global-agent", "--no-global-agent", "--skills-install-dir",
               "--global-agent-install-dir", "--non-interactive", "--dry-run", "--verbose",
               "--help")


def verify_readme_download(archive_path, checksum_path, platform):
    name = archive_path.name
    entries = {}
    for line in checksum_path.read_text(encoding="ascii").splitlines():
        match = re.fullmatch(r"([0-9a-f]{64})  ([A-Za-z0-9._-]+)", line)
        if not match or match.group(2) in entries:
            raise ValueError("invalid outer SHA256SUMS.txt")
        entries[match.group(2)] = match.group(1)
    if name not in entries or hashlib.sha256(archive_path.read_bytes()).hexdigest() != entries[name]:
        raise ValueError(f"outer ZIP digest missing or mismatched: {name}")
    with zipfile.ZipFile(archive_path) as archive:
        info = json.loads(archive.read("release-info.json"))
    expected_label = {"macos-arm64": "macos-latest", "windows-x64-msvc": "windows-latest"}[platform]
    environment = info["tested_environment"]
    if environment.get("runner_label") != expected_label or any(not environment.get(field) for field in (
        "runner_os", "cpu_architecture", "runner_image_name", "runner_image_version",
        "os_name", "os_version", "os_build",
    )):
        raise ValueError("ZIP tested environment is incomplete or has the wrong runner label")


def assert_macos_network_sandbox():
    try:
        with socket.socket() as probe:
            probe.bind(("127.0.0.1", 0))
    except OSError:
        return
    raise AssertionError("macOS network sandbox allowed a direct socket")


def extract_checked(archive_path, destination):
    verify_zip(archive_path)
    with zipfile.ZipFile(archive_path) as archive:
        archive.extractall(destination)
        for info in archive.infolist():
            if not info.is_dir() and sys.platform == "darwin":
                (destination / info.filename).chmod((info.external_attr >> 16) & 0o777)
    verify_tree(destination)


def verify_cli_entries(package, platform):
    suffix = ".exe" if platform == "windows-x64-msvc" else ""
    for name, schema in (("work", "work-cli-result"), ("tooling", "tooling-cli-result")):
        binary = package / "bin" / (name + suffix)
        result = subprocess.run([str(binary), "--help"], capture_output=True, timeout=30, check=True)
        response = json.loads(result.stdout)
        help_text = response.get("data", {}).get("help", "")
        if (response.get("schema") != schema or response.get("status") != "success" or
                not help_text or name not in help_text):
            raise AssertionError(f"{name} release CLI entry returned an invalid response")


def run_installer(script, home, options=(), answers=None, extra_env=None, stream_output=False):
    environment = os.environ.copy()
    if sys.platform == "win32":
        system_root = Path(os.environ.get("SystemRoot", r"C:\Windows"))
        system_paths = (system_root / "System32", system_root / "System32/WindowsPowerShell/v1.0")
        environment["PATH"] = os.pathsep.join(map(str, system_paths))
        command = ["cmd.exe", "/D", "/C", str(script), *options]
    else:
        environment["PATH"] = "/usr/bin:/bin"
        command = ["bash", str(script), *options]
    environment.update({"USERPROFILE": str(home), "HOME": str(home), "CI": "true"})
    environment.update(extra_env or {})
    kwargs = {"input": answers.encode("utf-8")} if answers is not None else {"stdin": subprocess.DEVNULL}
    started = time.monotonic()
    print(f"Installer E2E started: {script.name}, case={home.name}, "
          f"options={ascii(list(options))}", flush=True)
    try:
        result = subprocess.run(command, cwd=home, env=environment,
                                capture_output=not stream_output, timeout=600, **kwargs)
    except BaseException as error:
        print(f"Installer E2E interrupted: {script.name}, case={home.name}, "
              f"elapsed={time.monotonic() - started:.1f}s, reason={type(error).__name__}", flush=True)
        raise
    print(f"Installer E2E finished: {script.name}, case={home.name}, "
          f"elapsed={time.monotonic() - started:.1f}s, exit={result.returncode}", flush=True)
    return result.returncode, result.stdout, result.stderr


def assert_success(result):
    code, stdout, stderr = result
    if code:
        raise AssertionError(f"installer failed ({code}): stdout={stdout!r}, stderr={stderr!r}")


def assert_failed(result, expected=None):
    if result[0] == 0 or (expected is not None and result[0] != expected):
        raise AssertionError(f"expected installer failure {expected}: {result!r}")


def assert_no_target(result, home, target):
    assert_failed(result)
    if target.exists() or (home / ".codex").exists():
        raise AssertionError(f"rejected package changed the target: {result!r}")


def assert_state(work, version, workspace_version, global_agents, global_dir=None):
    state = json.loads((work.parent / "install-state.json").read_text(encoding="utf-8"))
    if (state["version"] != version or state["work_version"] != workspace_version or
            state["tooling_version"] != workspace_version or
            Path(state["skill_install_dir"]) != work.parent or
            state["global_agent_dir"] != (str(global_dir) if global_agents else None) or
            state["status"] != "installed" or
            set(state) != {"version", "work_version", "tooling_version", "skill_install_dir", "global_agent_dir", "status"}):
        raise AssertionError(f"unexpected Skills state: {state!r}")
    if (work / "install-state.json").exists():
        raise AssertionError("Work retained a child install-state.json")
    binary = work / "scripts" / ("work.exe" if sys.platform == "win32" else "work")
    subprocess.run([str(binary), "--help"], check=True, capture_output=True, timeout=30)
    tooling = work.parent / "tooling"
    if (tooling / "install-state.json").exists():
        raise AssertionError("Tooling retained a child install-state.json")
    tooling_binary = tooling / "scripts" / ("tooling.exe" if sys.platform == "win32" else "tooling")
    response = json.loads(subprocess.run([str(tooling_binary), "--help"], check=True,
                                       capture_output=True, timeout=30).stdout)
    if response.get("schema") != "tooling-cli-result" or response.get("status") != "success":
        raise AssertionError("installed Tooling CLI returned an invalid response")
    for required in ("SKILL.md", "references/instruction-loading.md", "references/instructions/instructions.md"):
        if not (tooling / required).is_file():
            raise AssertionError(f"installed Tooling omitted {required}")
    if (tooling / "references/instructions/interfaces/repl").exists():
        raise AssertionError("installed Tooling retained the removed REPL interface")


def assert_no_work_transactions(work):
    parent = work.parent
    if (any(parent.glob(".work-install.*")) or any(parent.glob(".agents-install.*")) or
            any(parent.glob(".tooling-install.*"))):
        raise AssertionError(f"successful installation retained a transaction under {parent}")


def exercise_package(package, root, platform, source_install=False):
    suffix = ".bat" if platform == "windows-x64-msvc" else ".command"
    script = package / f"install-work-from-package{suffix}"
    source = Path(__file__).resolve().parents[3] / "os-scripts" / (
        "windows" if suffix == ".bat" else "mac") / f"install-work-from-source{suffix}"
    if not script.is_file() or not source.is_file():
        raise AssertionError("a formal installer entry is missing")
    for entry in (script, source):
        help_home = root / f"help {entry.name}"
        help_home.mkdir()
        for option in ("--help", "-h"):
            result = run_installer(entry, help_home, [option])
            assert_success(result)
            if not all(flag.encode() in result[1] for flag in CLI_OPTIONS):
                raise AssertionError(f"{entry.name} {option} omitted a CLI option: {result!r}")
        dry = run_installer(entry, help_home,
            ["--non-interactive", "--no-global-agent", "--dry-run"])
        assert_success(dry)
        if b"Version:" not in dry[1] or b"Tooling target:" not in dry[1] or b"Dry run:" not in dry[1]:
            raise AssertionError(f"{entry.name} dry run omitted its summary: {dry!r}")
        verbose = run_installer(entry, help_home,
            ["--non-interactive", "--no-global-agent", "--dry-run", "--verbose"])
        assert_success(verbose)
        if b"Installer" not in verbose[1] or b"Dry run:" not in verbose[1]:
            raise AssertionError(f"{entry.name} verbose dry run omitted diagnostics: {verbose!r}")
        for invalid in (["--unknown"], ["--install-dir", "old/work"], ["--skills"],
                        ["--skills", "unknown"], ["--list-skills"],
                        ["--work-install-dir"], ["--skills-install-dir"], ["--global-agent-install-dir"],
                        ["--global-agent", "--no-global-agent"],
                        ["--verbose", "--verbose"], ["--help", "-h"],
                        ["--yes"], ["--force"]):
            assert_failed(run_installer(entry, help_home, invalid), 2)
        if (help_home / ".agents").exists() or (help_home / ".codex").exists():
            raise AssertionError("read-only CLI actions wrote installation data")

    home = root / "default install"
    home.mkdir()
    work = home / ".agents/skills/work"
    common = ["--non-interactive", "--no-global-agent"]
    release_info = json.loads((package / "release-info.json").read_text(encoding="utf-8"))
    version = release_info["version"]
    workspace_version = release_info["work_version"]
    assert_success(run_installer(script, home, [*common, "--dry-run"]))
    if work.exists():
        raise AssertionError("dry run created Work")
    assert_success(run_installer(script, home, common))
    assert_state(work, version, workspace_version, False)
    assert_no_work_transactions(work)
    if (home / ".codex").exists():
        raise AssertionError("disabled Global AGENTS created a target")

    default_home = root / "first install defaults"
    default_home.mkdir()
    assert_success(run_installer(script, default_home, ["--non-interactive"]))
    assert_state(default_home / ".agents/skills/work", version, workspace_version, True, default_home / ".codex")
    if (default_home / ".codex/AGENTS.md").read_bytes() != (package / "AGENTS.md").read_bytes():
        raise AssertionError("first installation did not enable the default Global AGENTS")

    saved_home = root / "saved Global AGENTS directory"
    saved_home.mkdir()
    saved_work = saved_home / ".agents/skills/work"
    saved_global = root / "自訂 Global AGENTS & path"
    saved_options = ["--non-interactive", "--global-agent",
                     "--global-agent-install-dir", str(saved_global)]
    assert_success(run_installer(script, saved_home, saved_options))
    assert_state(saved_work, version, workspace_version, True, saved_global)
    assert_success(run_installer(script, saved_home, ["--non-interactive"]))
    assert_state(saved_work, version, workspace_version, True, saved_global)
    if (saved_home / ".codex").exists():
        raise AssertionError("saved Global AGENTS directory was replaced by the default")

    empty_home = root / "empty Work directory"
    empty_home.mkdir()
    empty_work = empty_home / ".agents/skills/work"
    empty_work.mkdir(parents=True)
    assert_success(run_installer(script, empty_home, common))
    assert_state(empty_work, version, workspace_version, False)
    assert_no_work_transactions(empty_work)

    cancelled_home = root / "cancelled installation"
    cancelled_home.mkdir()
    cancelled_work = cancelled_home / ".agents/skills/work"
    assert_no_target(run_installer(script, cancelled_home,
        ["--no-global-agent"], answers="\nn\n"), cancelled_home, cancelled_work)

    conflict_home = root / "Global AGENTS conflict"
    conflict_home.mkdir()
    global_target = conflict_home / ".codex/AGENTS.md"
    global_target.parent.mkdir()
    global_target.write_bytes(b"user-owned Global AGENTS\r\n")
    conflict_work = conflict_home / ".agents/skills/work"
    assert_failed(run_installer(script, conflict_home,
        ["--non-interactive", "--global-agent"]))
    if conflict_work.exists() or global_target.read_bytes() != b"user-owned Global AGENTS\r\n":
        raise AssertionError("Global AGENTS conflict changed existing data")
    assert_success(run_installer(script, conflict_home,
        ["--global-agent"], answers="\n\ny\ny\n"))
    assert_state(conflict_work, version, workspace_version, True, conflict_home / ".codex")
    if global_target.read_bytes() != (package / "AGENTS.md").read_bytes():
        raise AssertionError("confirmed Global AGENTS replacement did not publish source bytes")
    assert_no_work_transactions(conflict_work)

    retained_home = root / "retained Global AGENTS"
    retained_home.mkdir()
    retained_target = retained_home / ".codex/AGENTS.md"
    retained_target.parent.mkdir()
    retained_target.write_bytes(b"keep user Global AGENTS\n")
    assert_success(run_installer(script, retained_home,
        ["--global-agent"], answers="\n\nn\ny\n"))
    assert_state(retained_home / ".agents/skills/work", version, workspace_version, True, retained_home / ".codex")
    if retained_target.read_bytes() != b"keep user Global AGENTS\n":
        raise AssertionError("declined Global AGENTS replacement changed original bytes")
    assert_success(run_installer(script, home, common))
    assert_state(work, version, workspace_version, False)
    for mode in ("task", "execute"):
        path = work / "references/instructions" / mode / "programming-language/java/spring-boot/instructions.md"
        if not path.is_file():
            raise AssertionError(f"selected hierarchy is missing: {path}")
    assert_no_work_transactions(work)

    custom_home = root / "custom home"
    custom_home.mkdir()
    custom = root / "路徑 with spaces & symbols" / "work"
    custom_options = [*common, "--skills-install-dir", str(custom.parent)]
    assert_success(run_installer(script, custom_home, custom_options))
    assert_state(custom, version, workspace_version, False)
    for mode in ("task", "execute"):
        path = custom / "references/instructions" / mode / "web/frontend/css/tailwind/instructions.md"
        if not path.is_file():
            raise AssertionError(f"all hierarchy omitted {path}")
    assert_no_work_transactions(custom)

    migration_home = root / "migrate default Work"
    migration_home.mkdir()
    old_work = migration_home / ".agents/skills/work"
    shutil.copytree(work, old_work)
    old_binary = (old_work / "scripts" / ("work.exe" if suffix == ".bat" else "work")).read_bytes()
    moved_work = root / "migrated Work" / "work"
    assert_success(run_installer(script, migration_home,
        ["--non-interactive", "--no-global-agent", "--skills-install-dir", str(moved_work.parent)]))
    if (old_work / "scripts" / ("work.exe" if suffix == ".bat" else "work")).read_bytes() != old_binary:
        raise AssertionError("independent installation changed the previous Work")
    assert_state(moved_work, version, workspace_version, False)
    assert_no_work_transactions(moved_work)

    if source_install:
        source_home = root / "source install"
        source_home.mkdir()
        source_work = source_home / ".agents/skills/work"
        target = "x86_64-pc-windows-msvc" if suffix == ".bat" else "aarch64-apple-darwin"
        toolchain = rust_toolchain(target)
        source_environment = {
            "PATH": os.environ["PATH"],
            "RUSTUP_TOOLCHAIN": toolchain,
            "RUSTUP_AUTO_INSTALL": "0",
            "CARGO_NET_OFFLINE": "true",
            "RUSTUP_HOME": os.environ.get("RUSTUP_HOME", str(Path.home() / ".rustup")),
            "CARGO_HOME": os.environ.get("CARGO_HOME", str(Path.home() / ".cargo")),
        }
        print("Running Source installation with cached Cargo dependencies.", flush=True)
        assert_success(run_installer(source, source_home, common,
                                     extra_env=source_environment, stream_output=True))
        assert_state(source_work, version, workspace_version, False)
        assert_no_work_transactions(source_work)
        source_interactive_home = root / "source interactive install"
        source_interactive_home.mkdir()
        assert_success(run_installer(source, source_interactive_home,
                                     ["--no-global-agent"],
                                     answers="\ny\n", extra_env=source_environment,
                                     stream_output=True))
        assert_state(source_interactive_home / ".agents/skills/work", version, workspace_version, False)

    first_update = root / "first update"
    first_update.mkdir()
    legacy = first_update / ".agents/skills/work"
    shutil.copytree(work, legacy)
    legacy_state = legacy / "install-state.json"
    legacy_state.write_bytes(b"legacy state\n")
    rejected = run_installer(script, first_update, common)
    assert_failed(rejected)
    assert_failed(run_installer(script, first_update, ["--no-global-agent"], answers="\nYES\ny\n"))
    if not (legacy / "scripts" / ("work.exe" if suffix == ".bat" else "work")).is_file() or legacy_state.read_bytes() != b"legacy state\n":
        raise AssertionError("legacy Work state or binary changed after rejection")

    declined_home = root / "declined first update"
    declined_home.mkdir()
    declined = declined_home / ".agents/skills/work"
    shutil.copytree(work, declined)
    prior_binary = (declined / "scripts" / ("work.exe" if suffix == ".bat" else "work")).read_bytes()
    assert_failed(run_installer(script, declined_home,
        ["--no-global-agent", "--skills-install-dir", str(declined.parent)], answers="no\n"))
    if (declined / "install-state.json").exists() or (declined / "scripts" / ("work.exe" if suffix == ".bat" else "work")).read_bytes() != prior_binary:
        raise AssertionError("declined first update changed Work")

    invalid_home = root / "invalid install state"
    invalid_home.mkdir()
    invalid_work = invalid_home / ".agents/skills/work"
    shutil.copytree(work, invalid_work)
    invalid_state = invalid_work / "install-state.json"
    invalid_state.write_bytes(b"{invalid json\n")
    assert_failed(run_installer(script, invalid_home, common))
    if invalid_state.read_bytes() != b"{invalid json\n":
        raise AssertionError("noninteractive first update changed invalid state")
    assert_failed(run_installer(script, invalid_home,
        ["--no-global-agent"], answers="\nYES\ny\n"))
    if invalid_state.read_bytes() != b"{invalid json\n":
        raise AssertionError("interactive attempt changed invalid legacy state")

    for name, raw in (("empty", b""), ("missing field", b'{"version":"1.0.5"}'),
                      ("incompatible", b'{"schema_version":999}')):
        case_home = root / f"invalid state {name}"
        case_home.mkdir()
        case_work = case_home / ".agents/skills/work"
        shutil.copytree(work, case_work)
        state_file = case_work / "install-state.json"
        state_file.write_bytes(raw)
        assert_failed(run_installer(script, case_home, common))
        if state_file.read_bytes() != raw:
            raise AssertionError(f"{name} state changed on noninteractive rejection")
        assert_failed(run_installer(script, case_home,
            ["--no-global-agent", "--skills-install-dir", str(case_work.parent)],
            answers="YES\ny\n"))
        if state_file.read_bytes() != raw:
            raise AssertionError(f"{name} state changed on interactive rejection")

    unknown_home = root / "unknown contents"
    unknown_home.mkdir()
    unknown = unknown_home / ".agents/skills/work"
    unknown.mkdir(parents=True)
    marker = unknown / "user-notes.txt"
    marker.write_text("keep", encoding="utf-8")
    assert_failed(run_installer(script, unknown_home, common))
    unknown_result = run_installer(script, unknown_home,
        ["--no-global-agent", "--skills-install-dir", str(unknown.parent)], answers="y\n")
    assert_failed(unknown_result)
    diagnostic = unknown_result[1] + unknown_result[2]
    if b"parent install-state.json" not in diagnostic or str(unknown).encode() not in diagnostic:
        raise AssertionError("unsafe Work diagnostic omitted the unmanaged target")
    if marker.read_text(encoding="utf-8") != "keep" or (unknown / "install-state.json").exists():
        raise AssertionError("unknown Work content was modified")

    link_home = root / "unsafe managed link"
    link_home.mkdir()
    linked_work = link_home / ".agents/skills/work"
    shutil.copytree(work, linked_work)
    outside = root / "linked user data"
    outside.mkdir()
    (outside / "marker.txt").write_text("keep", encoding="utf-8")
    linked_directory = linked_work / "references/instructions/task/general"
    shutil.rmtree(linked_directory)
    if suffix == ".bat":
        result = subprocess.run(["cmd.exe", "/D", "/C", "mklink", "/J",
                                 str(linked_directory), str(outside)],
                                capture_output=True, timeout=30)
        if result.returncode:
            raise AssertionError(f"failed to create Windows junction: stdout={result.stdout!r}, stderr={result.stderr!r}")
    else:
        linked_directory.symlink_to(outside, target_is_directory=True)
    assert_failed(run_installer(script, link_home, common))
    assert_failed(run_installer(script, link_home,
        ["--no-global-agent", "--skills-install-dir", str(linked_work.parent)],
        answers="y\n", stream_output=True))
    if (outside / "marker.txt").read_text(encoding="utf-8") != "keep" or not linked_directory.exists():
        raise AssertionError("unsafe Work link or linked data changed after rejection")

    for scenario in ("hash", "extra", "missing", "missing-options", "duplicate-manifest", "unsafe-manifest"):
        bad = root / f"bad {scenario}"
        shutil.copytree(package, bad)
        if scenario == "hash":
            (bad / "AGENTS.md").write_bytes(b"tampered")
        elif scenario == "extra":
            (bad / "extra.txt").write_bytes(b"extra")
        elif scenario == "missing":
            (bad / "AGENTS.md").unlink()
        elif scenario == "missing-options":
            (bad / "installer/cli/options.tsv").unlink()
        elif scenario == "duplicate-manifest":
            manifest = bad / "SHA256SUMS"
            lines = manifest.read_bytes().splitlines(keepends=True)
            manifest.write_bytes(b"".join(lines + [lines[0]]))
        else:
            with (bad / "SHA256SUMS").open("ab") as stream:
                stream.write(b"0" * 64 + b"  ../outside\n")
        bad_home = root / f"target {scenario}"
        bad_home.mkdir()
        target = bad_home / ".agents/skills/work"
        assert_no_target(run_installer(bad / script.name, bad_home, common), bad_home, target)

    if suffix == ".command":
        for flag, value in (("-s", "Linux"), ("-m", "x86_64")):
            wrong = root / f"wrong macOS {flag}"
            wrong.mkdir()
            shim = root / f"uname-{flag[1:]}"
            shim.mkdir()
            command = shim / "uname"
            command.write_text(
                f'#!/bin/sh\nif [ "$1" = "{flag}" ]; then printf "%s\\n" "{value}"; '
                'else /usr/bin/uname "$@"; fi\n', encoding="utf-8")
            command.chmod(0o755)
            assert_no_target(run_installer(script, wrong, common,
                extra_env={"PATH": f"{shim}:/usr/bin:/bin"}), wrong, wrong / ".agents/skills/work")
    else:
        wrong = root / "wrong Windows architecture"
        wrong.mkdir()
        assert_no_target(run_installer(script, wrong, common,
            extra_env={"PROCESSOR_ARCHITECTURE": "x86"}), wrong, wrong / ".agents/skills/work")

    print(f"Package native E2E passed: {platform}, version {version}, {script.name}")
def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--zip", required=True, type=Path)
    parser.add_argument("--checksums", required=True, type=Path)
    parser.add_argument("--platform", required=True, choices=("macos-arm64", "windows-x64-msvc"))
    parser.add_argument("--source-install", action="store_true", help="also build and install through Source")
    args = parser.parse_args()
    if args.platform == "windows-x64-msvc" and sys.platform != "win32":
        parser.error("Windows ZIP requires a Windows runner")
    if args.platform == "macos-arm64" and sys.platform != "darwin":
        parser.error("macOS ZIP requires a macOS runner")
    if args.platform == "macos-arm64":
        assert_macos_network_sandbox()
    verify_readme_download(args.zip, args.checksums, args.platform)
    repository = Path(__file__).resolve().parents[3]
    with tempfile.TemporaryDirectory(prefix=".issue95-release-e2e.", dir=repository) as directory:
        root = Path(directory)
        if root.resolve().parent != repository.resolve():
            raise AssertionError("Release E2E fixture path escaped the repository")
        package = root / "package"
        package.mkdir()
        extract_checked(args.zip, package)
        verify_cli_entries(package, args.platform)
        (root / "original.zip").write_bytes(args.zip.read_bytes())
        exercise_package(package, root, args.platform, args.source_install)
    print(f"Final ZIP E2E passed: {args.platform}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
