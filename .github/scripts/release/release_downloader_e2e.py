"""Install an actual Release ZIP through the standalone downloader over loopback."""

import argparse
import functools
import hashlib
import http.server
import json
import os
import shutil
import stat
import subprocess
import sys
import tempfile
import threading
import zipfile
from pathlib import Path

from release_install_e2e import extract_checked, verify_readme_download
from downloader_fixture import build_test_downloaders
from release_package_integrity import create_manifest, verify_zip


class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass


def make_crash_package(archive_path, root, platform):
    package = root / "crash-package"
    extract_checked(archive_path, package)
    relative = "installer/transaction/work.ps1" if platform == "windows-x64-msvc" else "installer/transaction/work.sh"
    script = package / relative
    content = script.read_text(encoding="utf-8")
    if platform == "windows-x64-msvc":
        needle = "            Move-Item -LiteralPath $targetPath -Destination $previous -ErrorAction Stop\n"
        injection = "            if ($env:WORK_RELEASE_CRASH_ONCE -eq '1') { [Diagnostics.Process]::GetCurrentProcess().Kill() }\n"
    else:
        needle = '        mv -- "$final" "$previous" || return 1\n'
        injection = '        if [[ "${WORK_RELEASE_CRASH_ONCE:-}" == 1 ]]; then kill -9 $$; fi\n'
    if content.count(needle) != 1:
        raise AssertionError("public Package crash injection point is ambiguous")
    script.write_text(content.replace(needle, needle + injection), encoding="utf-8")
    (package / "SHA256SUMS").unlink()
    create_manifest(package)
    crash_zip = root / "crash-package.zip"
    with zipfile.ZipFile(crash_zip, "x", compression=zipfile.ZIP_DEFLATED) as output:
        for path in sorted(package.rglob("*")):
            if not path.is_file():
                continue
            info = zipfile.ZipInfo(path.relative_to(package).as_posix(), date_time=(1980, 1, 1, 0, 0, 0))
            info.create_system = 3
            info.external_attr = (stat.S_IFREG | (path.stat().st_mode & 0o777)) << 16
            output.writestr(info, path.read_bytes(), compress_type=zipfile.ZIP_DEFLATED)
    verify_zip(crash_zip)
    return package, crash_zip


def exercise_public_crash(entry, target, home, environment, platform, version):
    options = ["--non-interactive", "--no-global-agent", "--skills-install-dir", str(target.parent)]
    command = (["cmd.exe", "/D", "/C", str(entry)] if platform == "windows-x64-msvc"
               else ["/bin/bash", str(entry)]) + options
    def invoke(crash=False):
        env = {**environment, "WORK_RELEASE_CRASH_ONCE": "1" if crash else "0"}
        return subprocess.run(command, cwd=home, env=env, stdin=subprocess.DEVNULL,
                              capture_output=True, timeout=180)

    initial = invoke()
    if initial.returncode:
        raise AssertionError(f"public entry initial install failed: {initial.stdout!r}, {initial.stderr!r}")
    binary = target / "scripts" / ("work.exe" if platform == "windows-x64-msvc" else "work")
    prior_binary = binary.read_bytes()
    interrupted = invoke(crash=True)
    transactions = list(target.parent.glob(".work-install.*"))
    if interrupted.returncode == 0 or len(transactions) != 1 or not (transactions[0] / "previous").is_dir():
        raise AssertionError(f"public entry did not interrupt during Work switch: {interrupted.returncode}, {transactions!r}")
    previous = transactions[0] / "previous"
    pending_state = json.loads((target.parent / "install-state.json").read_text(encoding="utf-8"))
    if (pending_state["version"] != version or pending_state["status"] != "installing" or
            (previous / "scripts" / binary.name).read_bytes() != prior_binary):
        raise AssertionError("public entry interruption lost the previous Work installation")
    restarted = invoke()
    if restarted.returncode:
        raise AssertionError(f"public entry recovery failed: {restarted.stdout!r}, {restarted.stderr!r}")
    state = json.loads((target.parent / "install-state.json").read_text(encoding="utf-8"))
    if (state["version"] != version or state["status"] != "installed" or
            list(target.parent.glob(".work-install.*"))):
        raise AssertionError("public entry recovery retained an incomplete transaction")
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
    with tempfile.TemporaryDirectory(prefix="work-downloader-e2e-") as directory:
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
        target = root / "中文 path & symbols" / "work"
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
                "WORK_RELEASE_TEST_MODE": "1",
                "WORK_RELEASE_TEST_BASE_URL": f"http://127.0.0.1:{server.server_port}",
            })
            repo = Path(__file__).resolve().parents[3]
            downloaders = {path.name: path for path in build_test_downloaders(repo, root / "downloaders")}
            if args.platform == "windows-x64-msvc":
                entry = downloaders["install-work-from-github-release.bat"]
                command = ["cmd.exe", "/D", "/C", str(entry)]
            else:
                entry = downloaders["install-work-from-github-release.command"]
                command = ["/bin/bash", str(entry)]
            command += ["--non-interactive", "--global-agent", "--skills-install-dir", str(target.parent),
                        "--global-agent-install-dir", str(global_dir)]
            entry_command = command[:4] if args.platform == "windows-x64-msvc" else command[:2]
            help_result = subprocess.run(entry_command + ["--help"], cwd=home, env=environment,
                                         stdin=subprocess.DEVNULL, capture_output=True, timeout=30)
            if help_result.returncode or list(scratch.glob("work-release*")):
                raise AssertionError(f"Downloader help did not exit cleanly without temporary files: "
                                     f"{help_result.returncode}, {help_result.stdout!r}, {help_result.stderr!r}")
            short_help = subprocess.run(entry_command + ["-h"], cwd=home, env=environment,
                                        stdin=subprocess.DEVNULL, capture_output=True, timeout=30)
            if short_help.returncode or short_help.stdout != help_result.stdout or list(scratch.glob("work-release*")):
                raise AssertionError("Downloader short help did not match zero-download help")
            result = subprocess.run(command, cwd=home, env=environment, stdin=subprocess.DEVNULL,
                                    capture_output=True, timeout=180)
            if result.returncode:
                raise AssertionError(f"Downloader failed: {result.returncode}, {result.stdout!r}, {result.stderr!r}")
            state_path = target.parent / "install-state.json"
            state = json.loads(state_path.read_text(encoding="utf-8"))
            if (state != {"version": info["version"], "work_version": info["work_version"],
                          "tooling_version": info["tooling_version"], "skill_install_dir": str(target.parent),
                          "global_agent_dir": str(global_dir), "status": "installed"} or
                    (target / "install-state.json").exists()):
                raise AssertionError(f"Downloader installed the wrong state: {state!r}")
            tooling = target.parent / "tooling"
            if ((tooling / "install-state.json").exists() or
                    not (tooling / "scripts" / ("tooling.exe" if sys.platform == "win32" else "tooling")).is_file()):
                raise AssertionError("Downloader installed the wrong Tooling entry")
            if (tooling / "references/instructions/interfaces/repl").exists():
                raise AssertionError("Downloader installed the removed REPL interface")
            if (global_dir / "AGENTS.md").read_bytes() != (Path(__file__).resolve().parents[3] / "AGENTS.md").read_bytes():
                raise AssertionError("Downloader did not install Global AGENTS at the requested path")
            before_state = state_path.read_bytes()
            before_agents = (global_dir / "AGENTS.md").read_bytes()
            for options, expected in ((["--non-interactive", "--dry-run",
                                      "--skills-install-dir", str(target.parent), "--global-agent-install-dir", str(global_dir)], 0),
                                      (["--unknown"], 2), (["--skills"], 2), (["--list-skills"], 2),
                                      (["--install-dir", "old/work"], 2),
                                      (["--global-agent", "--no-global-agent"], 2),
                                      (["--verbose", "--verbose"], 2)):
                check = subprocess.run(entry_command + options, cwd=home, env=environment,
                                       stdin=subprocess.DEVNULL, capture_output=True, timeout=180)
                if check.returncode != expected:
                    raise AssertionError(f"Downloader CLI status for {options!r}: {check.returncode}")
                if "--dry-run" in options and b"Dry run:" not in check.stdout:
                    raise AssertionError("Downloader omitted Package dry-run summary")
                if (state_path.read_bytes() != before_state or
                        (global_dir / "AGENTS.md").read_bytes() != before_agents):
                    raise AssertionError(f"Downloader CLI probe changed an installation target: {options!r}")
            binary = target / "scripts" / ("work.exe" if sys.platform == "win32" else "work")
            subprocess.run([str(binary), "--help"], check=True, capture_output=True, timeout=30)
            if list(scratch.glob("work-release*")):
                raise AssertionError("Downloader retained temporary files")
            if (home / ".codex").exists():
                raise AssertionError("disabled Global AGENTS created a target")
            crash_package, crash_zip = make_crash_package(args.zip, root, args.platform)
            package_entry = crash_package / ("install-work-from-package.bat" if args.platform == "windows-x64-msvc"
                                              else "install-work-from-package.command")
            exercise_public_crash(package_entry, root / "package-crash/work", home,
                                  environment, args.platform, info["version"])
            print(f"Public Package interruption recovery passed: {args.platform}", flush=True)
            shutil.copyfile(crash_zip, assets / args.zip.name)
            digest = hashlib.sha256(crash_zip.read_bytes()).hexdigest()
            (assets / "SHA256SUMS.txt").write_text(f"{digest}  {args.zip.name}\n", encoding="ascii")
            exercise_public_crash(entry, root / "downloader-crash/work", home,
                                  environment, args.platform, info["version"])
            print(f"Public Downloader interruption recovery passed: {args.platform}", flush=True)
            if list(scratch.glob("work-release*")) or (home / ".codex").exists():
                raise AssertionError("public crash recovery retained downloader scratch or Global AGENTS")
        finally:
            server.shutdown()
            thread.join()
            server.server_close()
    print(f"Actual ZIP Downloader E2E passed: {args.platform}, {info['tag']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
