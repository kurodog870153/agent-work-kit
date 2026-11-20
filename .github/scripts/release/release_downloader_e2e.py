"""Install an actual Release ZIP through the standalone downloader over loopback."""

import argparse
import functools
import hashlib
import http.server
import json
import os
import shutil
import subprocess
import sys
import tempfile
import threading
import zipfile
from pathlib import Path

from release_install_e2e import verify_readme_download
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tests/release/common"))
from downloader_server import build_test_downloaders


class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass


def exercise_public_crash(entry, home, environment, platform, info, skills_dir, global_dir=None):
    """An interrupted public install leaves a recoverable transaction; the next run restores service."""
    options = ["--host", info["host"], "--non-interactive", "--global-instructions", "no", "--skills-dir", str(skills_dir)]
    command = (["cmd.exe", "/D", "/C", str(entry)] if platform == "windows-x64-msvc"
               else ["/bin/bash", str(entry)]) + options

    def invoke(**extra):
        return subprocess.run(command, cwd=home, env={**environment, **extra}, stdin=subprocess.DEVNULL,
                              capture_output=True, timeout=180)

    initial = invoke()
    if initial.returncode:
        raise AssertionError(f"public entry initial install failed: {initial.stdout!r}, {initial.stderr!r}")
    suffix = ".exe" if platform == "windows-x64-msvc" else ""
    binary = skills_dir.parent / "agent-work-kit" / "bin" / f"work{suffix}"
    before = {str(path.relative_to(skills_dir)): path.read_bytes() for path in skills_dir.rglob("*") if path.is_file()}
    interrupted = invoke(INSTALLER_TEST_CRASH="verify")
    if interrupted.returncode == 0:
        raise AssertionError("public entry did not stop at the injected interruption")
    restarted = invoke()
    if restarted.returncode:
        raise AssertionError(f"public entry recovery failed: {restarted.stdout!r}, {restarted.stderr!r}")
    after = {str(path.relative_to(skills_dir)): path.read_bytes() for path in skills_dir.rglob("*") if path.is_file()}
    state = json.loads((skills_dir / "install-state.json").read_text(encoding="utf-8"))
    if after != before or state["status"] != "installed" or state["version"] != info["version"] or any(skills_dir.glob(".agent-work-kit-txn*")):
        raise AssertionError("public entry recovery did not restore the installed resources")
    subprocess.run([str(binary), "--help"], check=True, capture_output=True, timeout=30)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--zip", required=True, type=Path)
    parser.add_argument("--checksums", required=True, type=Path)
    parser.add_argument("--platform", required=True, choices=("macos-arm64", "windows-x64-msvc"))
    args = parser.parse_args()
    if args.platform == "macos-arm64" and sys.platform != "darwin":
        parser.error("macOS runner required")
    if args.platform == "windows-x64-msvc" and sys.platform != "win32":
        parser.error("Windows runner required")
    verify_readme_download(args.zip, args.checksums, args.platform)
    with tempfile.TemporaryDirectory(prefix="agent-work-kit-downloader-e2e-") as directory:
        root = Path(directory).resolve()
        with zipfile.ZipFile(args.zip) as archive:
            info = json.loads(archive.read("release-info.json"))
        assets = root / info["tag"]
        assets.mkdir()
        shutil.copyfile(args.zip, assets / args.zip.name)
        shutil.copyfile(args.checksums, assets / "SHA256SUMS.txt")
        home = root / "home"
        scratch = root / "scratch"
        home.mkdir()
        scratch.mkdir()
        target = root / "中文 path & symbols" / "skills"
        global_dir = root / "全域 path & symbols"
        handler = functools.partial(QuietHandler, directory=str(root))
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            environment = os.environ.copy()
            environment.update({
                "HOME": str(home), "USERPROFILE": str(home),
                "TMPDIR": str(scratch), "TEMP": str(scratch), "TMP": str(scratch),
                "AGENT_WORK_KIT_RELEASE_TEST_MODE": "1",
                "AGENT_WORK_KIT_RELEASE_TEST_BASE_URL": f"http://127.0.0.1:{server.server_port}",
            })
            repo = Path(__file__).resolve().parents[3]
            downloaders = {path.name: path for path in build_test_downloaders(repo, root / "downloaders")}
            if args.platform == "windows-x64-msvc":
                entry = downloaders["install-agent-work-kit-from-github-release.bat"]
                command = ["cmd.exe", "/D", "/C", str(entry)]
            else:
                entry = downloaders["install-agent-work-kit-from-github-release.command"]
                command = ["/bin/bash", str(entry)]
            command += ["--host", info["host"], "--non-interactive", "--global-instructions", "yes", "--skills-dir", str(target.parent),
                        "--instructions-dir", str(global_dir)]
            entry_command = command[:4] if args.platform == "windows-x64-msvc" else command[:2]
            help_result = subprocess.run(entry_command + ["--help"], cwd=home, env=environment,
                                         stdin=subprocess.DEVNULL, capture_output=True, timeout=30)
            if help_result.returncode or list(scratch.glob("agent-work-kit-release*")):
                raise AssertionError(f"Downloader help did not exit cleanly without temporary files: "
                                     f"{help_result.returncode}, {help_result.stdout!r}, {help_result.stderr!r}")
            short_help = subprocess.run(entry_command + ["-h"], cwd=home, env=environment,
                                        stdin=subprocess.DEVNULL, capture_output=True, timeout=30)
            if short_help.returncode or short_help.stdout != help_result.stdout or list(scratch.glob("agent-work-kit-release*")):
                raise AssertionError("Downloader short help did not match zero-download help")
            result = subprocess.run(command, cwd=home, env=environment, stdin=subprocess.DEVNULL,
                                    capture_output=True, timeout=180)
            if result.returncode:
                raise AssertionError(f"Downloader failed: {result.returncode}, {result.stdout!r}, {result.stderr!r}")
            skills_dir = target.parent
            state_path = skills_dir / "install-state.json"
            state = json.loads(state_path.read_text(encoding="utf-8"))
            if (state["host"] != info["host"] or state["version"] != info["version"] or
                    state["rust_workspace_version"] != info["rust_workspace_version"] or
                    Path(state["skill_install_dir"]) != skills_dir or state["status"] != "installed" or
                    state["global_instructions_dir"] != str(global_dir)):
                raise AssertionError(f"Downloader installed the wrong state: {state!r}")
            for required in ("work/SKILL.md", "tooling/SKILL.md", "shared/bootstrap.md"):
                if not (skills_dir / required).is_file():
                    raise AssertionError(f"Downloader omitted {required}")
            if any((skills_dir / name).exists() for name in ("contract", "shared/SKILL.md")):
                raise AssertionError("Downloader installed an obsolete resource")
            suffix = ".exe" if sys.platform == "win32" else ""
            cli_bin = Path(state["cli_install_dir"]) / "bin"
            if {path.name for path in cli_bin.iterdir()} != {f"work{suffix}", f"skill{suffix}"}:
                raise AssertionError("Downloader installed the wrong CLI set")
            instruction_name = "AGENTS.md" if info["host"] == "codex" else "CLAUDE.md"
            repository = Path(__file__).resolve().parents[3]
            if (global_dir / instruction_name).read_bytes() != (repository / "instructions/global-instructions.md").read_bytes():
                raise AssertionError("Downloader did not install Global Instructions at the requested path")
            before_state = state_path.read_bytes()
            before_agents = (global_dir / instruction_name).read_bytes()
            for options, expected in ((["--host", info["host"], "--non-interactive", "--dry-run",
                                      "--skills-dir", str(skills_dir), "--instructions-dir", str(global_dir)], 0),
                                      (["--host", info["host"], "--unknown"], 2),
                                      (["--host", info["host"], "--skills"], 2),
                                      (["--host", info["host"], "--migrate"], 2),
                                      (["--host", info["host"], "--install-dir", "old/work"], 2),
                                      (["--host", info["host"], "--global-instructions", "no", "--instructions-dir", str(global_dir)], 2),
                                      (["--host", info["host"], "--verbose", "--verbose"], 2)):
                check = subprocess.run(entry_command + options, cwd=home, env=environment,
                                       stdin=subprocess.DEVNULL, capture_output=True, timeout=180)
                if check.returncode != expected:
                    raise AssertionError(f"Downloader CLI status for {options!r}: {check.returncode}")
                if "--dry-run" in options and b"Dry run:" not in check.stdout:
                    raise AssertionError("Downloader omitted Package dry-run summary")
                if state_path.read_bytes() != before_state or (global_dir / instruction_name).read_bytes() != before_agents:
                    raise AssertionError(f"Downloader CLI probe changed an installation target: {options!r}")
            if list(scratch.glob("agent-work-kit-release*")):
                raise AssertionError("Downloader retained temporary files")
            exercise_public_crash(entry, home, environment, args.platform, info, root / "downloader-crash/skills")
            print(f"Public Downloader interruption recovery passed: {args.platform}", flush=True)
            if list(scratch.glob("agent-work-kit-release*")):
                raise AssertionError("public crash recovery retained downloader scratch files")
        finally:
            server.shutdown()
            thread.join()
            server.server_close()
    print(f"Actual ZIP Downloader E2E passed: {args.platform}, {info['tag']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
