"""Exercise the final extracted release ZIP on its intended native runner."""

import argparse
import os
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path

from release_integrity import verify_tree, verify_zip


def extract_checked(archive_path, destination):
    verify_zip(archive_path)
    with zipfile.ZipFile(archive_path) as archive:
        archive.extractall(destination)
        for info in archive.infolist():
            if not info.is_dir() and sys.platform == "darwin":
                (destination / info.filename).chmod((info.external_attr >> 16) & 0o777)
    verify_tree(destination)


def run_windows(script, home, prompts):
    system_root = os.environ.get("SystemRoot", r"C:\Windows")
    environment = os.environ.copy()
    system_paths = [Path(system_root) / "System32", Path(system_root) / "System32/WindowsPowerShell/v1.0"]
    environment.update({"USERPROFILE": str(home), "HOME": str(home), "PATH": os.pathsep.join(map(str, system_paths))})
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


def run_macos(script, home, answers):
    environment = os.environ.copy()
    environment.update({"HOME": str(home), "PATH": "/usr/bin:/bin"})
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


def e2e_windows(package, root):
    script = package / "install-release.bat"
    home = root / "windows default"
    home.mkdir()
    assert_success(run_windows(script, home, [
        ("Install Global AGENTS.md? [y/N] ", "n"),
        ("Select an installation location [1]: ", "1"),
        ('Select hierarchy numbers, enter "all", or press Enter for general only: ', "1"),
    ]))
    work = home / ".agents/skills/work"
    if not (work / "scripts/work.exe").is_file() or (home / ".codex").exists():
        raise AssertionError("Windows default installation content is incorrect")
    subprocess.run([str(work / "scripts/work.exe"), "--help"], check=True, capture_output=True)

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
    ]))
    if (global_target / "AGENTS.md").read_bytes() != (package / "AGENTS.md").read_bytes():
        raise AssertionError("Windows Global AGENTS.md differs from package")
    if not (custom / "skills/work/references/instructions/task/web/frontend/css/tailwind/instructions.md").is_file():
        raise AssertionError("Windows all hierarchy installation is incomplete")

    for scenario in ("hash", "extra", "missing", "duplicate-manifest"):
        bad = root / f"windows bad {scenario}"
        bad.mkdir()
        with zipfile.ZipFile(root / "original.zip") as archive:
            archive.extractall(bad)
        if scenario == "hash":
            (bad / "AGENTS.md").write_bytes(b"tampered")
        elif scenario == "extra":
            (bad / "extra.txt").write_bytes(b"extra")
        elif scenario == "missing":
            (bad / "AGENTS.md").unlink()
        else:
            manifest = bad / "SHA256SUMS"
            lines = manifest.read_bytes().splitlines(keepends=True)
            manifest.write_bytes(b"".join(lines + [lines[0]]))
        bad_home = root / f"target {scenario}"
        bad_home.mkdir()
        assert_failure_before_write(run_windows(bad / "install-release.bat", bad_home, []), bad_home)


def e2e_macos(package, root):
    script = package / "install-release.command"
    home = root / "macOS default"
    home.mkdir()
    assert_success(run_macos(script, home, ["n", "1", "1"]))
    binary = home / ".agents/skills/work/scripts/work"
    subprocess.run([str(binary), "--help"], check=True, capture_output=True)
    custom = root / "macOS 測試 user"
    custom.mkdir()
    assert_success(run_macos(script, custom, ["y", "1", "y", "2", str(custom), "all"]))
    if (custom / ".codex/AGENTS.md").read_bytes() != (package / "AGENTS.md").read_bytes():
        raise AssertionError("macOS Global AGENTS.md differs from package")
    if not (custom / "skills/work/references/instructions/task/web/frontend/css/tailwind/instructions.md").is_file():
        raise AssertionError("macOS all hierarchy installation is incomplete")
    bad = root / "macOS tampered"
    bad.mkdir()
    with zipfile.ZipFile(root / "original.zip") as archive:
        archive.extractall(bad)
    (bad / "AGENTS.md").write_bytes(b"tampered")
    bad_home = root / "macOS bad target"
    bad_home.mkdir()
    assert_failure_before_write(run_macos(bad / "install-release.command", bad_home, []), bad_home)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--zip", required=True, type=Path)
    parser.add_argument("--platform", required=True, choices=("macos-arm64", "windows-x64-msvc"))
    args = parser.parse_args()
    if args.platform == "windows-x64-msvc" and sys.platform != "win32":
        parser.error("Windows ZIP requires a Windows runner")
    if args.platform == "macos-arm64" and sys.platform != "darwin":
        parser.error("macOS ZIP requires a macOS runner")
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
