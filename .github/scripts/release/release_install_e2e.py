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
import zipfile
from pathlib import Path

from release_package_integrity import verify_tree, verify_zip


HIERARCHIES = {
    2: "web",
    3: "web/backend",
    4: "programming-language/java",
    5: "programming-language/java/persistence/jpa",
    6: "programming-language/java/persistence/mybatis",
    7: "web/frontend",
    8: "programming-language/typescript",
    9: "web/frontend/astro",
    10: "web/frontend/css",
    11: "web/frontend/css/tailwind",
    12: "programming-language/java/spring-boot",
}


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


def assert_hierarchy(work, selected):
    for mode in ("task", "execute"):
        root = work / "references/instructions" / mode
        if not (root / "general/instructions.md").is_file():
            raise AssertionError("general hierarchy is missing")
        parts = HIERARCHIES[selected].split("/")
        for count in range(1, len(parts) + 1):
            path = root.joinpath(*parts[:count], "instructions.md")
            if not path.is_file():
                raise AssertionError(f"selected hierarchy omitted {path}")


def extract_checked(archive_path, destination):
    verify_zip(archive_path)
    with zipfile.ZipFile(archive_path) as archive:
        archive.extractall(destination)
        for info in archive.infolist():
            if not info.is_dir() and sys.platform == "darwin":
                (destination / info.filename).chmod((info.external_attr >> 16) & 0o777)
    verify_tree(destination)


def run_windows(script, home, prompts, extra_env=None):
    system_root = os.environ.get("SystemRoot", r"C:\Windows")
    environment = os.environ.copy()
    system_paths = [Path(system_root) / "System32", Path(system_root) / "System32/WindowsPowerShell/v1.0"]
    environment.update({"USERPROFILE": str(home), "HOME": str(home), "PATH": os.pathsep.join(map(str, system_paths))})
    environment["CI"] = "true"
    environment.update(extra_env or {})
    process = subprocess.Popen(
        ["cmd.exe", "/C", str(script)],
        cwd=home,
        env=environment,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    output = bytearray()
    next_prompt = 0
    while byte := process.stdout.read(1):
        output.extend(byte)
        if next_prompt < len(prompts):
            prompt, answer = prompts[next_prompt]
            if output.endswith(prompt.encode("utf-8")):
                process.stdin.write(answer.encode("utf-8") + b"\r\n")
                process.stdin.flush()
                next_prompt += 1
                if next_prompt == len(prompts):
                    process.stdin.close()
    stderr = process.stderr.read()
    code = process.wait(timeout=120)
    if next_prompt != len(prompts):
        raise AssertionError(f"installer did not show expected prompt {next_prompt}: {output!r}, {stderr!r}")
    return code, bytes(output), stderr


def run_macos(script, home, answers, extra_env=None):
    environment = os.environ.copy()
    environment.update({"HOME": str(home), "PATH": "/usr/bin:/bin"})
    environment.update(extra_env or {})
    result = subprocess.run(
        ["bash", str(script)],
        cwd=home,
        env=environment,
        input=("\n".join(answers) + "\n").encode("utf-8"),
        capture_output=True,
        timeout=120,
    )
    return result.returncode, result.stdout, result.stderr


def assert_success(result):
    code, stdout, stderr = result
    if code:
        raise AssertionError(f"installer failed ({code}): stdout={stdout!r}, stderr={stderr!r}")


def assert_failure_before_write(result, home):
    code, stdout, stderr = result
    if code == 0 or (home / ".codex").exists() or (home / ".agents").exists():
        raise AssertionError(f"invalid package reached installation target: {code}, {stdout!r}, {stderr!r}")


def assert_failed(result):
    if result[0] == 0:
        raise AssertionError(f"expected installer failure: {result[1]!r}, {result[2]!r}")


def recovery_directory(result, label):
    output = result[1].decode("utf-8", errors="replace") + result[2].decode("utf-8", errors="replace")
    match = re.search(re.escape(label) + r'[^\n]*?"([^"]+)"', output)
    if not match:
        raise AssertionError(f"recovery directory not reported: {output}")
    return Path(match.group(1))


def forbidden_command_blockers(root):
    directory = root / "blocked commands"
    directory.mkdir()
    log = root / "forbidden-commands.log"
    for name in ("cargo", "rustc", "git", "gh", "curl", "wget", "xcrun"):
        command = directory / name
        command.write_text('#!/bin/sh\nprintf "%s\\n" "$0" >> "$FORBIDDEN_COMMAND_LOG"\nexit 97\n', encoding="utf-8")
        command.chmod(0o755)
    return {"PATH": f"{directory}:/usr/bin:/bin", "FORBIDDEN_COMMAND_LOG": str(log)}, log


def windows_forbidden_command_blockers(root):
    directory = root / "Windows blocked commands"
    directory.mkdir()
    log = root / "windows-forbidden-commands.log"
    for name in ("cargo", "rustc", "git", "gh", "curl", "wget"):
        (directory / f"{name}.cmd").write_bytes(
            b'@echo off\r\necho %~f0>>"%FORBIDDEN_COMMAND_LOG%"\r\nexit /b 97\r\n'
        )
    system_root = Path(os.environ.get("SystemRoot", r"C:\Windows"))
    system_paths = (system_root / "System32", system_root / "System32/WindowsPowerShell/v1.0")
    return {
        "PATH": os.pathsep.join((str(directory), *(str(path) for path in system_paths))),
        "FORBIDDEN_COMMAND_LOG": str(log),
    }, log


def windows_default_prompts():
    return [
        ("Install Global AGENTS.md? [y/N] ", "n"),
        ("Select an installation location [1]: ", "1"),
        ('Select hierarchy numbers, enter "all", or press Enter for general only: ', "1"),
    ]


def macos_mismatched_uname(root, flag, value):
    directory = root / f"uname mismatch {flag}"
    directory.mkdir()
    command = directory / "uname"
    command.write_text(
        f'#!/bin/sh\nif [ "$1" = "{flag}" ]; then printf "%s\\n" "{value}"; '
        'else /usr/bin/uname "$@"; fi\n', encoding="utf-8"
    )
    command.chmod(0o755)
    return {"PATH": f"{directory}:/usr/bin:/bin"}


def e2e_windows(package, root):
    script = package / "install-release.bat"
    wrong_arch_home = root / "Windows wrong CPU"
    wrong_arch_home.mkdir()
    assert_failure_before_write(
        run_windows(script, wrong_arch_home, [], {"PROCESSOR_ARCHITECTURE": "x86"}), wrong_arch_home
    )
    blocked_environment, blocked_log = windows_forbidden_command_blockers(root)
    home = root / "windows default"
    home.mkdir()
    assert_success(run_windows(script, home, windows_default_prompts(), blocked_environment))
    work = home / ".agents/skills/work"
    if not (work / "scripts/work.exe").is_file() or (home / ".codex").exists():
        raise AssertionError("Windows default installation content is incorrect")
    subprocess.run([str(work / "scripts/work.exe"), "--help"], check=True, capture_output=True)
    (work / "previous-marker").write_text("keep", encoding="utf-8")
    repeat = run_windows(script, home, [
        ("Install Global AGENTS.md? [y/N] ", "n"),
        ("Select an installation location [1]: ", "1"),
        ('Select hierarchy numbers, enter "all", or press Enter for general only: ', "8 11"),
    ], blocked_environment)
    assert_success(repeat)
    recovery = recovery_directory(repeat, "Installation recovery directory:")
    if (recovery / "previous/previous-marker").read_text(encoding="utf-8") != "keep":
        raise AssertionError("Windows Work backup did not preserve the full prior installation")
    for hierarchy in (
        "programming-language/typescript", "web/frontend/css/tailwind", "web/frontend/css", "web/frontend", "web"
    ):
        if not (work / f"references/instructions/task/{hierarchy}/instructions.md").is_file():
            raise AssertionError(f"Windows multi-selection omitted {hierarchy}")

    custom = root / "Windows 測試 user"
    custom.mkdir()
    global_target = custom / "Global 指引"
    assert_success(run_windows(script, custom, [
        ("Install Global AGENTS.md? [y/N] ", "y"),
        ("Select an installation location [1]: ", "2"),
        ("Enter the target directory: ", str(global_target)),
        ("The directory does not exist. Create it? [y/N] ", "y"),
        ("Select an installation location [1]: ", "2"),
        ("Enter the installation directory: ", str(custom)),
        ('Select hierarchy numbers, enter "all", or press Enter for general only: ', "all"),
    ], blocked_environment))
    if (global_target / "AGENTS.md").read_bytes() != (package / "AGENTS.md").read_bytes():
        raise AssertionError("Windows Global AGENTS.md differs from package")
    if not (custom / "skills/work/references/instructions/task/web/frontend/css/tailwind/instructions.md").is_file():
        raise AssertionError("Windows all hierarchy installation is incomplete")
    for selected in HIERARCHIES:
        individual = root / f"Windows hierarchy {selected}"
        individual.mkdir()
        assert_success(run_windows(script, individual, [
            ("Install Global AGENTS.md? [y/N] ", "n"),
            ("Select an installation location [1]: ", "1"),
            ('Select hierarchy numbers, enter "all", or press Enter for general only: ', str(selected)),
        ]))
        assert_hierarchy(individual / ".agents/skills/work", selected)
    if blocked_log.exists():
        raise AssertionError(f"Windows installer called a forbidden tool: {blocked_log.read_text(encoding='utf-8')}")

    refusal = root / "Windows refusal"
    refusal.mkdir()
    (refusal / ".codex").mkdir()
    (refusal / ".codex/AGENTS.md").write_text("old", encoding="utf-8")
    assert_success(run_windows(script, refusal, [
        ("Install Global AGENTS.md? [y/N] ", "y"),
        ("Select an installation location [1]: ", "1"),
        ("AGENTS.md already exists. Overwrite it? [y/N] ", "n"),
        ("Select an installation location [1]: ", "1"),
        ('Select hierarchy numbers, enter "all", or press Enter for general only: ', "1"),
    ]))
    if (refusal / ".codex/AGENTS.md").read_text(encoding="utf-8") != "old":
        raise AssertionError("Windows rejected Global AGENTS overwrite changed the old file")
    missing_directory = root / "Windows no directory"
    assert_success(run_windows(script, refusal, [
        ("Install Global AGENTS.md? [y/N] ", "y"),
        ("Select an installation location [1]: ", "2"),
        ("Enter the target directory: ", str(missing_directory)),
        ("The directory does not exist. Create it? [y/N] ", "n"),
        ("Select an installation location [1]: ", "1"),
        ('Select hierarchy numbers, enter "all", or press Enter for general only: ', "1"),
    ]))
    if missing_directory.exists():
        raise AssertionError("Windows rejected directory creation still wrote a target")

    for fault in ("global_prepare", "global_copy", "global_backup", "global_replace", "global_restore"):
        target_home = root / f"Windows {fault}"
        target_home.mkdir()
        (target_home / ".codex").mkdir()
        target = target_home / ".codex/AGENTS.md"
        target.write_text("old", encoding="utf-8")
        result = run_windows(script, target_home, [
            ("Install Global AGENTS.md? [y/N] ", "y"),
            ("Select an installation location [1]: ", "1"),
            ("AGENTS.md already exists. Overwrite it? [y/N] ", "y"),
        ], {"WORK_INSTALL_TEST_FAIL": fault})
        assert_failed(result)
        if (target_home / ".agents").exists():
            raise AssertionError(f"Windows {fault} continued into Work installation")
        if fault == "global_restore":
            backups = list((target_home / ".codex").glob(".agents-install-*/previous"))
            if target.exists() or len(backups) != 1 or backups[0].read_text(encoding="utf-8") != "old":
                raise AssertionError("Windows Global AGENTS recovery evidence is missing")
        elif target.read_text(encoding="utf-8") != "old":
            raise AssertionError(f"Windows {fault} changed the prior Global AGENTS")

    for fault in ("work_prepare", "work_copy", "work_backup", "work_replace", "work_restore"):
        target_home = root / f"Windows {fault}"
        target_home.mkdir()
        assert_success(run_windows(script, target_home, windows_default_prompts()))
        installed = target_home / ".agents/skills/work"
        (installed / "previous-marker").write_text("old", encoding="utf-8")
        result = run_windows(script, target_home, windows_default_prompts(), {"WORK_INSTALL_TEST_FAIL": fault})
        assert_failed(result)
        if fault == "work_restore":
            backups = list((target_home / ".agents/skills").glob(".work-install-*/previous/previous-marker"))
            if installed.exists() or len(backups) != 1 or backups[0].read_text(encoding="utf-8") != "old":
                raise AssertionError("Windows Work recovery evidence is missing")
        elif (installed / "previous-marker").read_text(encoding="utf-8") != "old":
            raise AssertionError(f"Windows {fault} changed the prior Work installation")

    after_global = root / "Windows global first"
    after_global.mkdir()
    result = run_windows(script, after_global, [
        ("Install Global AGENTS.md? [y/N] ", "y"),
        ("Select an installation location [1]: ", "1"),
        ("The directory does not exist. Create it? [y/N] ", "y"),
        ("Select an installation location [1]: ", "1"),
        ('Select hierarchy numbers, enter "all", or press Enter for general only: ', "1"),
    ], {"WORK_INSTALL_TEST_FAIL": "work_prepare"})
    assert_failed(result)
    if (after_global / ".codex/AGENTS.md").read_bytes() != (package / "AGENTS.md").read_bytes():
        raise AssertionError("Windows Work failure rolled back completed Global AGENTS installation")

    for scenario in ("hash", "extra", "missing", "duplicate-manifest", "unsafe-manifest"):
        bad = root / f"windows bad {scenario}"
        shutil.copytree(package, bad)
        if scenario == "hash":
            (bad / "AGENTS.md").write_bytes(b"tampered")
        elif scenario == "extra":
            (bad / "extra.txt").write_bytes(b"extra")
        elif scenario == "missing":
            (bad / "AGENTS.md").unlink()
        elif scenario == "duplicate-manifest":
            manifest = bad / "SHA256SUMS"
            lines = manifest.read_bytes().splitlines(keepends=True)
            manifest.write_bytes(b"".join(lines + [lines[0]]))
        else:
            with (bad / "SHA256SUMS").open("ab") as stream:
                stream.write(b"0" * 64 + b"  ../outside\n")
        bad_home = root / f"target {scenario}"
        bad_home.mkdir()
        assert_failure_before_write(run_windows(bad / "install-release.bat", bad_home, []), bad_home)

    import ctypes
    kernel = ctypes.windll.kernel32
    kernel.CreateFileW.argtypes = [
        ctypes.c_wchar_p, ctypes.c_uint32, ctypes.c_uint32, ctypes.c_void_p,
        ctypes.c_uint32, ctypes.c_uint32, ctypes.c_void_p,
    ]
    kernel.CreateFileW.restype = ctypes.c_void_p
    handle = kernel.CreateFileW(str(work / "scripts/work.exe"), 0x80000000, 1, None, 3, 0x80, None)
    if handle == ctypes.c_void_p(-1).value:
        raise AssertionError("could not acquire Windows binary lock")
    try:
        result = run_windows(script, home, windows_default_prompts())
        assert_failed(result)
        if not (work / "scripts/work.exe").is_file():
            raise AssertionError("Windows locked binary installation lost the prior Work")
    finally:
        kernel.CloseHandle(ctypes.c_void_p(handle))

    source_home = root / "Windows source Global first"
    source_home.mkdir()
    source_script = Path(__file__).resolve().parents[3] / "os-scripts/windows/install-work.bat"
    result = run_windows(source_script, source_home, [
        ("Install Global AGENTS.md? [y/N] ", "y"),
        ("Select an installation location [1]: ", "1"),
        ("The directory does not exist. Create it? [y/N] ", "y"),
    ], {"CARGO_HOME": str(source_home / "missing-cargo")})
    assert_failed(result)
    if not (source_home / ".codex/AGENTS.md").is_file() or (source_home / ".agents").exists():
        raise AssertionError("Windows source installer did not keep Global AGENTS after Work toolchain failure")

def e2e_macos(package, root):
    script = package / "install-release.command"
    for flag, value in (("-s", "Linux"), ("-m", "x86_64")):
        wrong_platform_home = root / f"macOS wrong {flag}"
        wrong_platform_home.mkdir()
        assert_failure_before_write(
            run_macos(script, wrong_platform_home, [], macos_mismatched_uname(root, flag, value)),
            wrong_platform_home,
        )
    blocked_environment, blocked_log = forbidden_command_blockers(root)
    home = root / "macOS default"
    home.mkdir()
    assert_success(run_macos(script, home, ["n", "1", "1"], blocked_environment))
    binary = home / ".agents/skills/work/scripts/work"
    subprocess.run([str(binary), "--help"], check=True, capture_output=True)
    marker = home / ".agents/skills/work/previous-marker"
    marker.write_text("keep", encoding="utf-8")
    repeat = run_macos(script, home, ["n", "1", "8 11"], blocked_environment)
    assert_success(repeat)
    recovery = recovery_directory(repeat, "Installation recovery directory:")
    if (recovery / "previous/previous-marker").read_text(encoding="utf-8") != "keep":
        raise AssertionError("macOS Work backup did not preserve the full prior installation")
    for hierarchy in (
        "programming-language/typescript", "web/frontend/css/tailwind", "web/frontend/css", "web/frontend", "web"
    ):
        if not (home / f".agents/skills/work/references/instructions/task/{hierarchy}/instructions.md").is_file():
            raise AssertionError(f"macOS multi-selection omitted {hierarchy}")
    custom = root / "macOS 測試 user"
    custom.mkdir()
    assert_success(run_macos(script, custom, ["y", "1", "y", "2", str(custom), "all"], blocked_environment))
    if (custom / ".codex/AGENTS.md").read_bytes() != (package / "AGENTS.md").read_bytes():
        raise AssertionError("macOS Global AGENTS.md differs from package")
    if not (custom / "skills/work/references/instructions/task/web/frontend/css/tailwind/instructions.md").is_file():
        raise AssertionError("macOS all hierarchy installation is incomplete")
    for selected in HIERARCHIES:
        individual = root / f"macOS hierarchy {selected}"
        individual.mkdir()
        assert_success(run_macos(script, individual, ["n", "1", str(selected)]))
        assert_hierarchy(individual / ".agents/skills/work", selected)
    if blocked_log.exists():
        raise AssertionError(f"macOS installer called a forbidden tool: {blocked_log.read_text(encoding='utf-8')}")

    refusal = root / "macOS refusal"
    refusal.mkdir()
    (refusal / ".codex").mkdir()
    (refusal / ".codex/AGENTS.md").write_text("old", encoding="utf-8")
    assert_success(run_macos(script, refusal, ["y", "1", "n", "1", "1"]))
    if (refusal / ".codex/AGENTS.md").read_text(encoding="utf-8") != "old":
        raise AssertionError("macOS rejected Global AGENTS overwrite changed the old file")
    missing_directory = root / "macOS no directory"
    assert_success(run_macos(script, refusal, ["y", "2", str(missing_directory), "n", "1", "1"]))
    if missing_directory.exists():
        raise AssertionError("macOS rejected directory creation still wrote a target")

    for fault in ("global_prepare", "global_copy", "global_backup", "global_replace", "global_restore"):
        target_home = root / f"macOS {fault}"
        target_home.mkdir()
        (target_home / ".codex").mkdir()
        target = target_home / ".codex/AGENTS.md"
        target.write_text("old", encoding="utf-8")
        result = run_macos(script, target_home, ["y", "1", "y"], {"WORK_INSTALL_TEST_FAIL": fault})
        assert_failed(result)
        if (target_home / ".agents").exists():
            raise AssertionError(f"macOS {fault} continued into Work installation")
        if fault == "global_restore":
            backups = list((target_home / ".codex").glob(".agents-install.*/previous"))
            if target.exists() or len(backups) != 1 or backups[0].read_text(encoding="utf-8") != "old":
                raise AssertionError("macOS Global AGENTS recovery evidence is missing")
        elif target.read_text(encoding="utf-8") != "old":
            raise AssertionError(f"macOS {fault} changed the prior Global AGENTS")

    for fault in ("work_prepare", "work_copy", "work_backup", "work_replace", "work_restore"):
        target_home = root / f"macOS {fault}"
        target_home.mkdir()
        assert_success(run_macos(script, target_home, ["n", "1", "1"]))
        work = target_home / ".agents/skills/work"
        (work / "previous-marker").write_text("old", encoding="utf-8")
        result = run_macos(script, target_home, ["n", "1", "1"], {"WORK_INSTALL_TEST_FAIL": fault})
        assert_failed(result)
        if fault == "work_restore":
            backups = list((target_home / ".agents/skills").glob(".work-install.*/previous/previous-marker"))
            if work.exists() or len(backups) != 1 or backups[0].read_text(encoding="utf-8") != "old":
                raise AssertionError("macOS Work recovery evidence is missing")
        elif (work / "previous-marker").read_text(encoding="utf-8") != "old":
            raise AssertionError(f"macOS {fault} changed the prior Work installation")

    after_global = root / "macOS global first"
    after_global.mkdir()
    result = run_macos(script, after_global, ["y", "1", "y", "1", "1"], {"WORK_INSTALL_TEST_FAIL": "work_prepare"})
    assert_failed(result)
    if (after_global / ".codex/AGENTS.md").read_bytes() != (package / "AGENTS.md").read_bytes():
        raise AssertionError("macOS Work failure rolled back completed Global AGENTS installation")

    for scenario in ("hash", "extra", "missing", "duplicate-manifest", "unsafe-manifest", "permission"):
        bad = root / f"macOS bad {scenario}"
        shutil.copytree(package, bad)
        if scenario == "hash":
            (bad / "AGENTS.md").write_bytes(b"tampered")
        elif scenario == "extra":
            (bad / "extra.txt").write_bytes(b"extra")
        elif scenario == "missing":
            (bad / "AGENTS.md").unlink()
        elif scenario == "duplicate-manifest":
            manifest = bad / "SHA256SUMS"
            lines = manifest.read_bytes().splitlines(keepends=True)
            manifest.write_bytes(b"".join(lines + [lines[0]]))
        elif scenario == "unsafe-manifest":
            with (bad / "SHA256SUMS").open("ab") as stream:
                stream.write(b"0" * 64 + b"  ../outside\n")
        else:
            (bad / "bin/work").chmod(0o644)
        bad_home = root / f"macOS target {scenario}"
        bad_home.mkdir()
        assert_failure_before_write(run_macos(bad / "install-release.command", bad_home, []), bad_home)

    source_home = root / "macOS source Global first"
    source_home.mkdir()
    source_script = Path(__file__).resolve().parents[3] / "os-scripts/mac/install-work.command"
    result = run_macos(source_script, source_home, ["y", "1", "y"], {"RUSTUP_TOOLCHAIN": "work-release-e2e-uninstalled"})
    assert_failed(result)
    if not (source_home / ".codex/AGENTS.md").is_file() or (source_home / ".agents").exists():
        raise AssertionError("macOS source installer did not keep Global AGENTS after Work toolchain failure")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--zip", required=True, type=Path)
    parser.add_argument("--checksums", required=True, type=Path)
    parser.add_argument("--platform", required=True, choices=("macos-arm64", "windows-x64-msvc"))
    args = parser.parse_args()
    if args.platform == "windows-x64-msvc" and sys.platform != "win32":
        parser.error("Windows ZIP requires a Windows runner")
    if args.platform == "macos-arm64" and sys.platform != "darwin":
        parser.error("macOS ZIP requires a macOS runner")
    if args.platform == "macos-arm64":
        assert_macos_network_sandbox()
    verify_readme_download(args.zip, args.checksums, args.platform)
    with tempfile.TemporaryDirectory(prefix="work-release-e2e-") as directory:
        root = Path(directory)
        package = root / "package"
        package.mkdir()
        extract_checked(args.zip, package)
        (root / "original.zip").write_bytes(args.zip.read_bytes())
        if args.platform == "windows-x64-msvc":
            e2e_windows(package, root)
        else:
            e2e_macos(package, root)
    print(f"Final ZIP E2E passed: {args.platform}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
