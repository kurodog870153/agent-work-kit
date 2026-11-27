"""Exercise the final extracted release ZIP on its intended native runner."""

import argparse
import hashlib
import json
import os
import re
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
import zipfile
from pathlib import Path

from release_package_integrity import verify_tree, verify_zip
from host_simulation import physical, read_installed_entry
from rust_toolchain import rust_toolchain
if sys.platform == "win32":
    from windows_job import CREATE_SUSPENDED, WindowsInstallerJob


CLI_OPTIONS = ("--host", "--global-instructions", "--install-dir",
               "--instructions-dir", "--non-interactive", "--dry-run", "--verbose",
               "--help")

# Reported by every installer log line so a cancelled release run can be traced
# back to the scenario that was executing at the time.
PHASE = {"current": "startup"}



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
    target = {"macos-arm64": "aarch64-apple-darwin", "windows-x64-msvc": "x86_64-pc-windows-msvc"}[platform]
    if (info.get("host") not in ("codex", "claude") or info.get("target") != target or
            name != f"agent-work-kit-v{info.get('version')}-{info['host']}-{platform}.zip"):
        raise ValueError("ZIP Host, platform or version identity is invalid")
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
    for name in ("work", "skill"):
        binary = package / "bin" / (name + suffix)
        result = subprocess.run([str(binary), "--help"], capture_output=True, timeout=30, check=True)
        if name not in result.stdout.decode("utf-8", "replace"):
            raise AssertionError(f"{name} release CLI entry returned an invalid help text")
        listing = json.loads(subprocess.run([str(binary), "list"], capture_output=True, timeout=30, check=True).stdout)
        if listing.get("status") != "ok" or not listing["data"]["commands"]:
            raise AssertionError(f"{name} release CLI entry returned an invalid contract index")


def set_phase(phase):
    PHASE["current"] = phase


def terminate_process_tree(process, grace=30, job=None):
    """Stop the installer and every descendant so a cancelled run leaves nothing behind."""
    if process is None:
        return True
    if job is not None:
        try:
            stopped = job.terminate_and_wait(grace)
            if stopped:
                process.wait(timeout=grace)
            return stopped
        except (OSError, RuntimeError) as error:
            print(f"Installer E2E Job Object termination failed: pid={process.pid}, "
                  f"reason={type(error).__name__}: {error}", flush=True)
            return False
    if sys.platform == "win32" and process.poll() is not None:
        print(f"Installer E2E cannot verify descendants after parent exited: pid={process.pid}", flush=True)
        return False
    try:
        if sys.platform == "win32":
            taskkill = Path(os.environ.get("SystemRoot", r"C:\Windows")) / "System32" / "taskkill.exe"
            result = subprocess.run([str(taskkill), "/F", "/T", "/PID", str(process.pid)],
                                    capture_output=True, timeout=60, check=False)
            if result.returncode:
                print(f"Installer E2E taskkill failed: pid={process.pid}, exit={result.returncode}, "
                      f"stderr={result.stderr.decode('utf-8', errors='replace')}", flush=True)
                return False
        else:
            # The new session's process group ID remains the launch PID even if
            # the installer exits before its descendants.
            os.killpg(process.pid, signal.SIGKILL)
    except (OSError, AttributeError, subprocess.SubprocessError) as error:
        print(f"Installer E2E process tree termination failed: pid={process.pid}, "
              f"reason={type(error).__name__}: {error}", flush=True)
        return False
    try:
        process.wait(timeout=grace)
    except subprocess.TimeoutExpired:
        print(f"Installer E2E process tree still alive after termination: pid={process.pid}",
              flush=True)
        return False
    except AttributeError:
        return False
    return True


def register_windows_e2e_clis(*programs):
    """Add firewall restrictions for extracted/installed CLI paths before use.

    The parent Windows PowerShell job owns the firewall rule lifecycle. This
    helper only registers concrete executables created in the E2E fixture.
    """
    rule = os.environ.get("AGENT_WORK_KIT_E2E_RULE_PREFIX")
    if not rule:
        return
    if sys.platform != "win32":
        raise AssertionError("scoped Windows firewall registration on a non-Windows runner")
    binaries = sorted({str(path.resolve()) for path in programs if path.is_file()})
    if not binaries:
        return
    register = Path(__file__).resolve().parent / "windows" / "register_programs.ps1"
    with tempfile.TemporaryDirectory(prefix="e2e-firewall-") as directory:
        paths_file = Path(directory) / "programs.json"
        paths_file.write_text(json.dumps(binaries), encoding="utf-8")
        result = subprocess.run(
            ["powershell.exe", "-NoProfile", "-NonInteractive", "-File", str(register),
             "-Rule", rule, "-PathsFile", str(paths_file)],
            capture_output=True, text=True, timeout=60,
        )
        if result.returncode != 0:
            raise AssertionError(
                f"cannot isolate E2E CLI binaries {binaries!r}: {result.stdout} {result.stderr}"
            )


def run_installer_for_host(script, home, options=(), answers=None, extra_env=None, stream_output=False, *, host, popen_factory=None):
    if tuple(options) not in (("--help",), ("-h",)) and "--host" not in options:
        options = ("--host", host, *options)
    environment = os.environ.copy()
    spawn = {}
    if sys.platform == "win32":
        system_root = Path(os.environ.get("SystemRoot", r"C:\Windows"))
        system_paths = (system_root / "System32", system_root / "System32/WindowsPowerShell/v1.0")
        environment["PATH"] = os.pathsep.join(map(str, system_paths))
        command = ["cmd.exe", "/D", "/C", str(script), *options]
        # Keep the installer out of this console's process group. A runner
        # cancellation then reaches only this Python process, so the installer
        # never stops on "Terminate batch job" and steals the next answer from
        # its own stdin pipe.
        spawn["creationflags"] = subprocess.CREATE_NEW_PROCESS_GROUP | CREATE_SUSPENDED
    else:
        environment["PATH"] = "/usr/bin:/bin"
        command = ["bash", str(script), *options]
        spawn["start_new_session"] = True
    environment.update({"USERPROFILE": str(home), "HOME": str(home), "CI": "true"})
    environment.update(extra_env or {})
    input_data = answers.encode("utf-8") if answers is not None else None
    stdin = subprocess.PIPE if answers is not None else subprocess.DEVNULL
    started = time.monotonic()
    print(f"Installer E2E started: {script.name}, case={home.name}, "
          f"options={ascii(list(options))}, run={os.environ.get('GITHUB_RUN_ID', 'local')}, "
          f"attempt={os.environ.get('GITHUB_RUN_ATTEMPT', 'local')}, "
          f"job={os.environ.get('GITHUB_JOB', 'local')}, phase={PHASE['current']}, "
          f"timeout=600s", flush=True)
    stopped = threading.Event()

    def log_wait():
        while not stopped.wait(30):
            print(f"Installer E2E still running: {script.name}, case={home.name}, "
                  f"elapsed={time.monotonic() - started:.1f}s, phase={PHASE['current']}",
                  flush=True)

    def log_tail():
        if stream_output:
            return
        for label, output in (("stdout", stdout_file), ("stderr", stderr_file)):
            output.seek(0)
            tail = output.read()[-4096:]
            if tail:
                print(f"Installer E2E {label} tail: {tail.decode('utf-8', errors='replace')}",
                      flush=True)

    reporter = threading.Thread(target=log_wait, daemon=True)
    with tempfile.TemporaryFile() as stdout_file, tempfile.TemporaryFile() as stderr_file:
        reporter.start()
        process = None
        job = None
        child_exit_at_interrupt = "not-started"
        try:
            if sys.platform == "win32":
                job = WindowsInstallerJob()
            process = (popen_factory or subprocess.Popen)(command, cwd=home, env=environment, stdin=stdin,
                                       stdout=None if stream_output else stdout_file,
                                       stderr=None if stream_output else stderr_file, **spawn)
            if job is not None:
                if isinstance(process, subprocess.Popen):
                    try:
                        job.attach_and_resume(process)
                    except BaseException:
                        process.kill()
                        process.wait(timeout=30)
                        raise
                else:
                    # Synthetic test processes do not have a Windows process handle.
                    job.close()
                    job = None
            try:
                print(f"Installer E2E process started: {script.name}, case={home.name}, "
                      f"pid={process.pid}", flush=True)
                try:
                    process.communicate(input=input_data, timeout=600)
                except subprocess.TimeoutExpired:
                    child_exit_at_interrupt = process.poll()
                    print(f"Installer E2E timed out: {script.name}, case={home.name}, "
                          f"pid={process.pid}, elapsed={time.monotonic() - started:.1f}s, "
                          f"phase={PHASE['current']}", flush=True)
                    if not terminate_process_tree(process, job=job):
                        raise RuntimeError(f"installer process tree cleanup failed after timeout: pid={process.pid}")
                    raise
                except KeyboardInterrupt:
                    child_exit_at_interrupt = process.poll()
                    print(f"Installer E2E cancelled: {script.name}, case={home.name}, "
                          f"pid={process.pid}, elapsed={time.monotonic() - started:.1f}s, "
                          f"phase={PHASE['current']}, "
                          f"child_exit_at_cancel={child_exit_at_interrupt}", flush=True)
                    if not terminate_process_tree(process, job=job):
                        print(f"Installer E2E cleanup incomplete after cancellation: pid={process.pid}",
                              flush=True)
                    raise
                except BaseException:
                    child_exit_at_interrupt = process.poll()
                    raise
            finally:
                if process.poll() is None and not terminate_process_tree(process, job=job):
                    print(f"Installer E2E process tree may remain alive: pid={process.pid}", flush=True)
        except BaseException as error:
            print(f"Installer E2E interrupted: {script.name}, case={home.name}, "
                  f"elapsed={time.monotonic() - started:.1f}s, reason={type(error).__name__}, "
                  f"pid={process.pid if process else 'not-started'}, "
                  f"phase={PHASE['current']}, "
                  f"child_exit_at_interrupt={child_exit_at_interrupt}", flush=True)
            log_tail()
            raise
        finally:
            stopped.set()
            reporter.join()
            if job is not None:
                job.close()
        if stream_output:
            stdout = stderr = None
        else:
            stdout_file.seek(0)
            stderr_file.seek(0)
            stdout = stdout_file.read()
            stderr = stderr_file.read()
    print(f"Installer E2E finished: {script.name}, case={home.name}, "
          f"elapsed={time.monotonic() - started:.1f}s, pid={process.pid}, "
          f"exit={process.returncode}, phase={PHASE['current']}", flush=True)
    if process.returncode == 0 and sys.platform == "win32":
        layout = HostLayout(home, host, "windows-x64-msvc")
        register_windows_e2e_clis(layout.work, layout.skill)
    return process.returncode, stdout, stderr


def assert_success(result):
    code, stdout, stderr = result
    if code:
        raise AssertionError(f"installer failed ({code}): stdout={stdout!r}, stderr={stderr!r}")


def assert_failed(result, expected=None):
    if result[0] == 0 or (expected is not None and result[0] != expected):
        raise AssertionError(f"expected installer failure {expected}: {result!r}")


def assert_no_target(result, home, target):
    assert_failed(result)
    if target.exists() or any((home / name).exists() for name in (".agents", ".codex", ".claude")):
        raise AssertionError(f"rejected package changed the target: {result!r}")


class HostLayout:
    """Independent managed root and Host-specific discovery parent."""

    def __init__(self, home, host, platform, install_dir=None):
        suffix = ".exe" if platform == "windows-x64-msvc" else ""
        self.host = host
        self.home = home
        self.discovery = home / (".agents/skills" if host == "codex" else ".claude/skills")
        self.install = install_dir if install_dir is not None else self.discovery.parent / "agent-work-kit"
        self.skills = self.install / "skills"
        self.cli = self.install
        self.global_dir = home / (".codex" if host == "codex" else ".claude")
        self.global_file = self.global_dir / ("AGENTS.md" if host == "codex" else "CLAUDE.md")
        self.state = self.install / "install-state.json"
        self.work = self.install / "bin" / f"work{suffix}"
        self.skill = self.install / "bin" / f"skill{suffix}"

    def snapshot(self):
        files = {}
        if self.install.exists():
            files.update({str(p): hashlib.sha256(p.read_bytes()).hexdigest()
                          for p in self.install.rglob("*") if p.is_file()})
        for name in ("work", "tooling"):
            directory = self.discovery / name
            if physical(directory, directory=True):
                files.update({str(p): hashlib.sha256(p.read_bytes()).hexdigest()
                              for p in directory.rglob("*") if physical(p)})
            elif os.path.lexists(directory):
                files[str(directory)] = "unsafe entry"
        if self.global_file.is_file():
            files[str(self.global_file)] = hashlib.sha256(self.global_file.read_bytes()).hexdigest()
        return files


def assert_installation(layout, info, global_instructions):
    """Exact State, resource set, physical discovery entries and runnable binaries."""
    raw_state = layout.state.read_bytes().decode("utf-8")
    state = json.loads(raw_state)
    keys = {"host", "version", "rust_workspace_version", "install_dir",
            "global_instructions_dir", "status", "files"}
    if (set(state) != keys or state["host"] != layout.host or state["version"] != info["version"] or
            state["rust_workspace_version"] != info["rust_workspace_version"] or state["status"] != "installed" or
            Path(state["install_dir"]) != layout.install or
            state["global_instructions_dir"] != (str(layout.global_dir) if global_instructions else None)):
        raise AssertionError(f"unexpected installation state: {state!r}")
    for required in ("work/instructions/instructions.md", "tooling/instructions/instructions.md", "shared/bootstrap.md"):
        if not (layout.skills / required).is_file():
            raise AssertionError(f"installation omitted {required}")
    if {p.name for p in layout.skills.iterdir()} != {"work", "tooling", "shared"}:
        raise AssertionError("unexpected managed Skills entries")
    if {p.name for p in layout.install.iterdir()} != {"skills", "bin", "install-state.json"}:
        raise AssertionError("unexpected installation root entries (or README installed)")
    for name in ("work", "tooling"):
        paths = read_installed_entry(layout.discovery, name)
        if paths["state"] != layout.state:
            raise AssertionError("entry belongs to another installation")
    manifest = []
    for part in ("skills", "bin"):
        for path in (layout.install / part).rglob("*"):
            if physical(path):
                manifest.append({"path": path.relative_to(layout.install).as_posix(), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
            elif not physical(path, directory=True):
                raise AssertionError(f"unsafe managed resource: {path}")
    for name in ("work", "tooling"):
        path = layout.discovery / name / "SKILL.md"
        manifest.append({"path": f"host-skills/{name}/SKILL.md", "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
    if global_instructions:
        manifest.append({"path": f"global/{layout.global_file.name}", "sha256": hashlib.sha256(layout.global_file.read_bytes()).hexdigest()})
    if state["files"] != sorted(manifest, key=lambda file: file["path"]):
        raise AssertionError("State Manifest differs from installed resources")
    # Normalize equivalent JSON string escapes only; whitespace, key order, LF and EOF remain observable.
    normalized = re.sub(r'"(?:[^"\\]|\\.)*"', lambda token: json.dumps(json.loads(token.group()), ensure_ascii=False), raw_state)
    if (list(state) != ["host", "version", "rust_workspace_version", "install_dir", "global_instructions_dir", "status", "files"] or
            any(list(file) != ["path", "sha256"] for file in state["files"]) or
            normalized != json.dumps(state, ensure_ascii=False, indent=2) + "\n"):
        raise AssertionError("State JSON serialization is not canonical Pretty JSON")
    for obsolete in ("work/SKILL.md", "tooling/SKILL.md", "shared/SKILL.md", "work/scripts", "tooling/scripts", "contract"):
        if (layout.skills / obsolete).exists():
            raise AssertionError(f"obsolete resource installed: {obsolete}")
    if {p.name for p in (layout.install / "bin").iterdir()} != {layout.work.name, layout.skill.name}:
        raise AssertionError("CLI directory must hold exactly Work and Skill binaries")
    for binary in (layout.work, layout.skill):
        subprocess.run([str(binary), "--help"], check=True, capture_output=True, timeout=30)
    if any(layout.install.glob(".agent-work-kit-txn*")):
        raise AssertionError("a successful installation retained its transaction")
    if global_instructions != layout.global_file.is_file():
        raise AssertionError("Global Instructions presence does not match the selection")


def assert_installed_resolution(layout):
    """Both Skills resolve the same Shared tree and CLI independently of Global Instructions."""
    for name in ("work", "tooling"):
        paths = read_installed_entry(layout.discovery, name)
        result = subprocess.run([str(paths["skill"]), "validate", "--install", str(paths["install"])],
                                cwd=layout.home, capture_output=True, timeout=60, check=True)
        body = json.loads(result.stdout)
        checked = body["data"]["checked"]
        if not body["data"]["valid"] or sum(item == "shared:bootstrap.md" for item in checked) != 1:
            raise AssertionError(f"installed Skills failed validation: {body!r}")
        result = subprocess.run([str(paths["skill"]), "discover", "--install", str(paths["install"]), "--skill", name],
                                cwd=layout.home, capture_output=True, timeout=60, check=True)
        if not json.loads(result.stdout)["data"]["children"]:
            raise AssertionError(f"{name} has no discoverable instructions")


def run_installed_host_simulation(layout, report, zip_path, info, platform):
    scenario = Path(__file__).resolve().with_name("host_simulation.py")
    report.mkdir(parents=True, exist_ok=True)
    evidence = report / f"{layout.host}-{platform}.json"
    raw_evidence = report / f"{layout.host}-{platform}.tmp"
    record = {"host": layout.host, "platform": platform, "version": info["version"],
              "zip": zip_path.name, "zip_sha256": hashlib.sha256(zip_path.read_bytes()).hexdigest(),
              "installed_work": str(layout.work), "installed_skill": str(layout.skill),
              "commit": os.environ.get("GITHUB_SHA"), "run_id": os.environ.get("GITHUB_RUN_ID"),
              "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT"), "status": "failed"}
    started = time.monotonic()
    try:
        result = subprocess.run([sys.executable, "-B", str(scenario), "--host-skills-dir", str(layout.discovery),
                                 "--evidence", str(raw_evidence)], capture_output=True, text=True, timeout=300)
        if result.returncode:
            raise AssertionError(f"installed Host simulation failed: {result.stdout}\n{result.stderr}")
        record["simulation"] = json.loads(raw_evidence.read_text(encoding="utf-8"))
        record["status"] = "passed"
    except subprocess.TimeoutExpired:
        record["status"] = "timed_out"
        raise
    finally:
        record["duration_seconds"] = round(time.monotonic() - started, 3)
        evidence.write_text(json.dumps(record, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        raw_evidence.unlink(missing_ok=True)


def exercise_cli_entries(entries, root, run_installer):
    for entry in entries:
        help_home = root / f"help {entry.name}"
        help_home.mkdir()
        for option in ("--help", "-h"):
            result = run_installer(entry, help_home, [option])
            assert_success(result)
            if not all(flag.encode() in result[1] for flag in CLI_OPTIONS):
                raise AssertionError(f"{entry.name} {option} omitted a CLI option: {result!r}")
        dry = run_installer(entry, help_home, ["--non-interactive", "--global-instructions", "no", "--dry-run"])
        assert_success(dry)
        if b"Dry run:" not in dry[1] or b"version" not in dry[1].lower():
            raise AssertionError(f"{entry.name} dry run omitted its summary: {dry!r}")
        verbose = run_installer(entry, help_home, ["--non-interactive", "--global-instructions", "no", "--dry-run", "--verbose"])
        assert_success(verbose)
        if b"Installer" not in verbose[1]:
            raise AssertionError(f"{entry.name} verbose dry run omitted diagnostics: {verbose!r}")
        for invalid in (["--unknown"], ["--skills"], ["--install-dir"], ["--migrate"], ["--upgrade"],
                        ["--skills-dir"], ["--instructions-dir"],
                        ["--global-instructions", "yes", "--global-instructions", "no"],
                        ["--verbose", "--verbose"], ["--help", "-h"], ["--yes"], ["--force"]):
            assert_failed(run_installer(entry, help_home, invalid), 2)
        if any((help_home / name).exists() for name in (".agents", ".codex", ".claude")):
            raise AssertionError("read-only CLI actions wrote installation data")


def exercise_package(package, root, platform, source_install=False, host_evidence=None, zip_path=None):
    info = json.loads((package / "release-info.json").read_text(encoding="utf-8"))
    host = info["host"]
    if host not in ("codex", "claude"):
        raise AssertionError(f"unsupported release Host: {host!r}")
    started = time.monotonic()

    def log_phase(phase):
        set_phase(phase)
        print(f"Package E2E phase: host={host}, platform={platform}, phase={phase}, "
              f"elapsed={time.monotonic() - started:.1f}s", flush=True)

    def run_installer(script, home, options=(), answers=None, extra_env=None, stream_output=False):
        return run_installer_for_host(script, home, options, answers, extra_env, stream_output, host=host)

    suffix = ".bat" if platform == "windows-x64-msvc" else ".command"
    script = package / f"install-agent-work-kit-from-package{suffix}"
    source = Path(__file__).resolve().parents[3] / "os-scripts" / (
        "windows" if suffix == ".bat" else "mac") / f"install-agent-work-kit-from-source{suffix}"
    if not script.is_file() or not source.is_file():
        raise AssertionError("a formal installer entry is missing")

    log_phase("installer CLI checks")
    exercise_cli_entries((script, source), root, run_installer)

    log_phase("package installation")
    common = ["--non-interactive", "--global-instructions", "no"]
    mismatch = root / "wrong Host selection"
    mismatch.mkdir()
    wrong = HostLayout(mismatch, "claude" if host == "codex" else "codex", platform)
    assert_no_target(run_installer_for_host(script, mismatch, common, host=wrong.host), mismatch, wrong.skills)

    home = root / "first install"
    home.mkdir()
    layout = HostLayout(home, host, platform)
    assert_success(run_installer(script, home, [*common, "--dry-run"]))
    if any((home / name).exists() for name in (".agents", ".codex", ".claude")):
        raise AssertionError("dry run changed the installation target")
    # Third-party Skill symlinks remain outside our managed source boundary.
    foreign = root / "unrelated-skill"
    foreign.mkdir(exist_ok=True)
    layout.discovery.mkdir(parents=True, exist_ok=True)
    foreign_link = layout.discovery / "find-skills"
    if sys.platform == "win32":
        subprocess.run(["cmd.exe", "/D", "/C", "mklink", "/J", str(foreign_link), str(foreign)],
                       check=True, capture_output=True, timeout=30)
    else:
        os.symlink(foreign, foreign_link, target_is_directory=True)
    assert_success(run_installer(script, home, common))
    assert_installation(layout, info, global_instructions=False)
    if foreign_link.resolve() != foreign.resolve():
        raise AssertionError("installer modified unrelated Skill")
    assert_installed_resolution(layout)
    # Tampered entry paths must fail closed, even when correct binaries still exist.
    entry = layout.discovery / "work/SKILL.md"
    original = entry.read_bytes()
    try:
        lines = original.decode("utf-8").splitlines()
        entry.write_text("\n".join(line if not line.startswith("- Skill CLI: ")
                                   else "- Skill CLI: " + json.dumps(str(home / "missing-cli"))
                                   for line in lines) + "\n", encoding="utf-8")
        try:
            read_installed_entry(layout.discovery, "work")
        except AssertionError:
            pass
        else:
            raise AssertionError("tampered entry path was accepted")
        assert_failed(run_installer(script, home, common))
    finally:
        entry.write_bytes(original)
    first = layout.snapshot()
    assert_success(run_installer(script, home, common))
    assert_installation(layout, info, global_instructions=False)
    if layout.snapshot() != first:
        raise AssertionError("reinstalling changed the installed resources")

    log_phase("Global Instructions selection")
    assert_success(run_installer(script, home, ["--non-interactive", "--global-instructions", "yes"]))
    assert_installation(layout, info, global_instructions=True)
    if layout.global_file.read_bytes() != (package / "instructions/global-instructions.md").read_bytes():
        raise AssertionError("Global Instructions were not installed from the package")
    assert_success(run_installer(script, home, ["--non-interactive", "--global-instructions", "yes"]))
    assert_installation(layout, info, global_instructions=True)

    log_phase("unknown content is never taken over")
    unknown = root / "unknown content"
    unknown.mkdir()
    unknown_layout = HostLayout(unknown, host, platform)
    (unknown_layout.skills / "work").mkdir(parents=True)
    marker = unknown_layout.skills / "work" / "mine.txt"
    marker.write_text("keep", encoding="utf-8")
    before = unknown_layout.snapshot()
    assert_failed(run_installer(script, unknown, common))
    if marker.read_text(encoding="utf-8") != "keep" or unknown_layout.snapshot() != before or unknown_layout.state.exists():
        raise AssertionError("an unknown Skills directory was changed or taken over")
    stale = root / "invalid state"
    stale.mkdir()
    stale_layout = HostLayout(stale, host, platform)
    stale_layout.install.mkdir(parents=True)
    stale_layout.state.write_text("{\"status\": \"installed\"}\n", encoding="utf-8")
    before = stale_layout.snapshot()
    assert_failed(run_installer(script, stale, common))
    if stale_layout.snapshot() != before or (stale_layout.skills / "work").exists():
        raise AssertionError("an invalid state was repaired or taken over")

    log_phase("first-install Host entry rollback")
    entry_home = root / "entry failure"
    entry_home.mkdir()
    entry_layout = HostLayout(entry_home, host, platform)
    assert_failed(run_installer(script, entry_home, common, extra_env={"INSTALLER_TEST_FAILURE": "apply-host-work"}))
    if entry_layout.install.exists() or os.path.lexists(entry_layout.discovery / "work"):
        raise AssertionError("failed install retained managed data or Host entries")

    log_phase("custom installation root")
    custom_home = root / "custom installation"
    custom_home.mkdir()
    custom_root = root / "自訂 custom target" / "agent-work-kit"
    custom = HostLayout(custom_home, host, platform, install_dir=custom_root)
    custom_options = [*common, "--install-dir", str(custom_root)]
    assert_success(run_installer(script, custom_home, custom_options))
    assert_installation(custom, info, global_instructions=False)
    assert_installed_resolution(custom)
    before_custom = custom.snapshot()
    assert_failed(run_installer(script, custom_home, common))
    if custom.snapshot() != before_custom:
        raise AssertionError("default install changed custom installation")
    assert_success(run_installer(script, custom_home, custom_options))
    assert_installation(custom, info, global_instructions=False)

    log_phase("failure injection and interruption recovery")
    failure_home = root / "failure injection"
    failure_home.mkdir()
    failure = HostLayout(failure_home, host, platform)
    assert_success(run_installer(script, failure_home, common))
    before = failure.snapshot()
    for point in ("prepare", "apply-work", "apply-shared", "apply-host-work", "verify", "commit"):
        assert_failed(run_installer(script, failure_home, common, extra_env={"INSTALLER_TEST_FAILURE": point}))
        if failure.snapshot() != before or any(failure.install.glob(".agent-work-kit-txn*")):
            raise AssertionError(f"failure at {point} did not restore the previous installation")
    assert_failed(run_installer(script, failure_home, common, extra_env={"INSTALLER_TEST_CRASH": "verify"}))
    assert_success(run_installer(script, failure_home, common))
    assert_installation(failure, info, global_instructions=False)
    if failure.snapshot() != before:
        raise AssertionError("recovering an interrupted installation changed the resources")

    if host_evidence is not None:
        log_phase("Host simulation on the installed CLIs")
        run_installed_host_simulation(layout, host_evidence, zip_path, info, platform)

    if source_install:
        log_phase("Source installation")
        source_home = root / "source install"
        source_home.mkdir()
        source_layout = HostLayout(source_home, host, platform)
        target = "x86_64-pc-windows-msvc" if suffix == ".bat" else "aarch64-apple-darwin"
        source_environment = {
            "PATH": os.environ["PATH"],
            "RUSTUP_TOOLCHAIN": rust_toolchain(target),
            "RUSTUP_AUTO_INSTALL": "0",
            "CARGO_NET_OFFLINE": "true",
            "RUSTUP_HOME": os.environ.get("RUSTUP_HOME", str(Path.home() / ".rustup")),
            "CARGO_HOME": os.environ.get("CARGO_HOME", str(Path.home() / ".cargo")),
        }
        assert_success(run_installer(source, source_home, common, extra_env=source_environment, stream_output=True))
        assert_installation(source_layout, info, global_instructions=False)
        assert_installed_resolution(source_layout)
    log_phase("done")


def exercise_coexistence(packages, root, platform):
    """Both Hosts share one home; installing or rejecting one never changes the other."""
    if set(packages) != {"codex", "claude"}:
        raise AssertionError("coexistence requires both Host packages")
    set_phase("dual Host coexistence")
    home = root / "dual Host coexistence"
    home.mkdir()
    suffix = ".bat" if platform == "windows-x64-msvc" else ".command"
    options = ("--non-interactive", "--global-instructions", "yes")
    scripts = {host: package / f"install-agent-work-kit-from-package{suffix}" for host, package in packages.items()}
    layouts = {host: HostLayout(home, host, platform) for host in packages}
    infos = {host: json.loads((package / "release-info.json").read_text(encoding="utf-8")) for host, package in packages.items()}
    for host, other in (("codex", "claude"), ("claude", "codex"), ("codex", "claude")):
        other_before = layouts[other].snapshot()
        assert_success(run_installer_for_host(scripts[host], home, options, host=host))
        assert_installation(layouts[host], infos[host], global_instructions=True)
        if layouts[other].snapshot() != other_before:
            raise AssertionError(f"installing {host} changed {other}")
    for host, other in (("codex", "claude"), ("claude", "codex")):
        layouts[host].global_file.write_bytes(f"user-owned {host} instructions\n".encode())
        own_before, other_before = layouts[host].snapshot(), layouts[other].snapshot()
        assert_failed(run_installer_for_host(scripts[host], home, options, host=host))
        if layouts[host].snapshot() != own_before or layouts[other].snapshot() != other_before:
            raise AssertionError(f"rejected {host} update changed either Host")
    print(f"Dual Host coexistence E2E passed: {platform}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--zip", required=True, type=Path)
    parser.add_argument("--checksums", required=True, type=Path)
    parser.add_argument("--platform", required=True, choices=("macos-arm64", "windows-x64-msvc"))
    parser.add_argument("--source-install", action="store_true", help="also build and install through Source")
    parser.add_argument("--peer-zip", type=Path, help="exercise both Host ZIPs in one home")
    parser.add_argument("--installed-host-evidence", type=Path,
                        help="save the Host simulation evidence produced against the installed CLIs")
    args = parser.parse_args()
    if args.peer_zip and args.source_install:
        parser.error("--peer-zip cannot be combined with --source-install")
    if args.platform == "windows-x64-msvc" and sys.platform != "win32":
        parser.error("Windows ZIP requires a Windows runner")
    if args.platform == "macos-arm64" and sys.platform != "darwin":
        parser.error("macOS ZIP requires a macOS runner")
    if args.platform == "macos-arm64":
        assert_macos_network_sandbox()
    verify_readme_download(args.zip, args.checksums, args.platform)
    if args.peer_zip:
        verify_readme_download(args.peer_zip, args.checksums, args.platform)
    repository = Path(__file__).resolve().parents[3]
    with tempfile.TemporaryDirectory(prefix=".installer-test-release-e2e.", dir=repository) as directory:
        root = Path(directory)
        if root.resolve().parent != repository.resolve():
            raise AssertionError("Release E2E fixture path escaped the repository")
        package = root / "package"
        package.mkdir()
        extract_checked(args.zip, package)
        register_windows_e2e_clis(package / "bin/work.exe", package / "bin/skill.exe")
        verify_cli_entries(package, args.platform)
        (root / "original.zip").write_bytes(args.zip.read_bytes())
        if args.peer_zip:
            peer = root / "peer-package"
            peer.mkdir()
            extract_checked(args.peer_zip, peer)
            register_windows_e2e_clis(peer / "bin/work.exe", peer / "bin/skill.exe")
            verify_cli_entries(peer, args.platform)
            packages = {json.loads((path / "release-info.json").read_text(encoding="utf-8"))["host"]: path
                        for path in (package, peer)}
            if len(packages) != 2:
                raise AssertionError("coexistence ZIPs have the same Host")
            exercise_coexistence(packages, root, args.platform)
        else:
            exercise_package(package, root, args.platform, args.source_install,
                             args.installed_host_evidence, args.zip)
    print(f"Final ZIP E2E passed: {args.platform}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
