"""Exercise the final extracted release ZIP on its intended native runner."""

import argparse
import hashlib
import json
import os
import re
import shutil
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
from rust_toolchain import rust_toolchain
if sys.platform == "win32":
    from windows_job import CREATE_SUSPENDED, WindowsInstallerJob


CLI_OPTIONS = ("--host", "--global-instructions", "--skills-dir",
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
    for name, schema in (("work", "work-cli-result"), ("tooling", "tooling-cli-result")):
        binary = package / "bin" / (name + suffix)
        result = subprocess.run([str(binary), "--help"], capture_output=True, timeout=30, check=True)
        response = json.loads(result.stdout)
        help_text = response.get("data", {}).get("help", "")
        if (response.get("schema") != schema or response.get("status") != "success" or
                not help_text or name not in help_text):
            raise AssertionError(f"{name} release CLI entry returned an invalid response")


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


def assert_state_for_host(work, version, workspace_version, global_agents, global_dir=None, *, host):
    state = json.loads((work.parent / "install-state.json").read_text(encoding="utf-8"))
    if (state["host"] != host or state["version"] != version or state["rust_workspace_version"] != workspace_version or
            Path(state["skill_install_dir"]) != work.parent or
            state["global_instructions_dir"] != (str(global_dir) if global_agents else None) or
            state["status"] != "installed" or
            set(state) != {"host", "version", "rust_workspace_version", "skill_install_dir", "global_instructions_dir", "status"}):
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
    smoke = subprocess.run([sys.executable, "-B", str(Path(__file__).with_name("tooling_cli_smoke.py")),
                            "--skill-root", str(tooling)],
                           capture_output=True, text=True, timeout=120)
    if smoke.returncode:
        raise AssertionError(f"installed Tooling public smoke failed: {smoke.stdout} {smoke.stderr}")
    if (tooling / "references/instructions/interfaces/repl").exists():
        raise AssertionError("installed Tooling retained the removed REPL interface")
    contract = work.parent / "contract"
    if contract.is_symlink() or contract.resolve() != work.parent.resolve() / "contract":
        raise AssertionError("installed Contract Skill escapes the shared Skills parent")
    for required in ("SKILL.md", "references/instruction-loading.md", "references/instruction-loading/queries.md"):
        if not (contract / required).is_file():
            raise AssertionError(f"installed Contract omitted {required}")
    for executable, args, expected_id in (
        (binary, ("--project-root", str(work.parent), "contract", "describe", "work-cli-result"), "work-cli-result"),
        (tooling_binary, ("contract", "describe", "tooling-cli-result"), "tooling-cli-result"),
    ):
        result = subprocess.run([str(executable), *args], check=True, capture_output=True, timeout=30)
        response = json.loads(result.stdout)
        if response.get("status") != "success" or response.get("data", {}).get("contract_id") != expected_id:
            raise AssertionError(f"installed Contract query failed: {expected_id}")


def assert_no_work_transactions(work):
    parent = work.parent
    if (any(parent.glob(".work-install.*")) or any(parent.glob(".agents-install.*")) or
            any(parent.glob(".tooling-install.*"))):
        raise AssertionError(f"successful installation retained a transaction under {parent}")


def snapshot_host(home, host):
    skills = home / (".agents/skills" if host == "codex" else ".claude/skills")
    instructions = home / (".codex/AGENTS.md" if host == "codex" else ".claude/CLAUDE.md")
    files = {str(path.relative_to(home)): hashlib.sha256(path.read_bytes()).hexdigest()
             for path in skills.rglob("*") if path.is_file()}
    state = skills / "install-state.json"
    if not state.is_file():
        raise AssertionError(f"{host} has no parent install state")
    if not instructions.is_file():
        raise AssertionError(f"{host} has no global instructions")
    files[str(instructions.relative_to(home))] = hashlib.sha256(instructions.read_bytes()).hexdigest()
    return files


def exercise_coexistence(packages, root, platform):
    if set(packages) != {"codex", "claude"}:
        raise AssertionError("coexistence requires both Host packages")
    set_phase("dual Host coexistence")
    home = root / "dual Host coexistence"
    home.mkdir()
    suffix = ".bat" if platform == "windows-x64-msvc" else ".command"
    options = ("--non-interactive", "--global-instructions", "yes")
    scripts = {host: package / f"install-agent-work-kit-from-package{suffix}"
               for host, package in packages.items()}
    def assert_host_installed(host):
        work = home / (".agents/skills/work" if host == "codex" else ".claude/skills/work")
        info = json.loads((packages[host] / "release-info.json").read_text(encoding="utf-8"))
        global_dir = home / (".codex" if host == "codex" else ".claude")
        assert_state_for_host(work, info["version"], info["rust_workspace_version"], True, global_dir, host=host)
        assert_no_work_transactions(work)
        snapshot_host(home, host)

    for host in ("codex", "claude"):
        codex_before = snapshot_host(home, "codex") if host == "claude" else None
        assert_success(run_installer_for_host(scripts[host], home, options, host=host))
        assert_host_installed(host)
        if codex_before is not None and snapshot_host(home, "codex") != codex_before:
            raise AssertionError("installing Claude changed Codex")

    for host, other in (("codex", "claude"), ("claude", "codex"), ("codex", "claude")):
        other_before = snapshot_host(home, other)
        assert_success(run_installer_for_host(scripts[host], home, options, host=host))
        assert_host_installed(host)
        if snapshot_host(home, other) != other_before:
            raise AssertionError(f"updating {host} changed {other}")

    for host, other in (("codex", "claude"), ("claude", "codex")):
        instructions = home / (".codex/AGENTS.md" if host == "codex" else ".claude/CLAUDE.md")
        instructions.write_bytes(f"user-owned {host} instructions\n".encode())
        own_before, other_before = snapshot_host(home, host), snapshot_host(home, other)
        assert_failed(run_installer_for_host(scripts[host], home, options, host=host))
        if snapshot_host(home, host) != own_before or snapshot_host(home, other) != other_before:
            raise AssertionError(f"rejected {host} update changed either Host")
        assert_success(run_installer_for_host(scripts[host], home,
            ("--global-instructions", "yes"), answers="\n\ny\ny\n", host=host))
        assert_host_installed(host)
        if snapshot_host(home, other) != other_before:
            raise AssertionError(f"recovering {host} changed {other}")
        if instructions.read_bytes() != (packages[host] / "instructions/global-instructions.md").read_bytes():
            raise AssertionError(f"{host} instructions were not recovered")

    for host, other in (("codex", "claude"), ("claude", "codex")):
        skills = home / (".agents/skills" if host == "codex" else ".claude/skills")
        state_file = skills / "install-state.json"
        original = state_file.read_bytes()
        state = json.loads(original)
        state["host"] = other
        state_file.write_text(json.dumps(state), encoding="utf-8")
        other_before = snapshot_host(home, other)
        assert_failed(run_installer_for_host(scripts[host], home, options, host=host))
        if state_file.read_text(encoding="utf-8") != json.dumps(state) or snapshot_host(home, other) != other_before:
            raise AssertionError(f"wrong Host state was changed during {host} rejection")
        state_file.write_bytes(original)
        assert_success(run_installer_for_host(scripts[host], home, options, host=host))
        assert_host_installed(host)
        if snapshot_host(home, other) != other_before:
            raise AssertionError(f"restoring {host} state changed {other}")
    print(f"Dual Host coexistence E2E passed: {platform}")


def run_installed_cli_smoke(binary, smoke_report, zip_path, version, host, platform):
    if not binary.is_file():
        raise AssertionError(f"installed Work CLI is missing: {binary}")
    report = smoke_report / f"{host}-{platform}.md"
    smoke_report.mkdir(parents=True, exist_ok=True)
    environment = os.environ.copy()
    environment["WORK_BINARY"] = str(binary)
    environment["SCENARIO_REPORT"] = str(report)
    scenario = Path(__file__).resolve().with_name("work_cli_smoke.py")
    result = subprocess.run([sys.executable, "-B", str(scenario)],
                            capture_output=True, text=True, env=environment)
    if result.returncode != 0:
        raise AssertionError(f"installed Work CLI smoke failed: {result.stdout}\n{result.stderr}")
    tooling = binary.parent.parent.parent / "tooling"
    tooling_scenario = Path(__file__).resolve().with_name("tooling_cli_smoke.py")
    tooling_result = subprocess.run([sys.executable, "-B", str(tooling_scenario),
                                     "--skill-root", str(tooling)],
                                    capture_output=True, text=True, timeout=120)
    if tooling_result.returncode:
        raise AssertionError(f"installed Tooling CLI smoke failed: {tooling_result.stdout}\n{tooling_result.stderr}")
    tooling_evidence = json.loads(tooling_result.stdout)
    if (tooling_evidence.get("target_boundary") != "target_valid" or
            tooling_evidence.get("incompatible_policy_blocked") is not True):
        raise AssertionError(f"installed Tooling CLI smoke evidence is invalid: {tooling_evidence!r}")
    header = (f"# Installed Rust CLI smoke evidence\n\n"
              f"ZIP: `{zip_path.name}`\n\n"
              f"ZIP SHA-256: `{hashlib.sha256(zip_path.read_bytes()).hexdigest()}`\n\n"
              f"Release version: `{version}`\n\n"
              f"Host: `{host}`; platform: `{platform}`\n\n"
              f"Installed Work CLI: `{binary}`\n\n"
              f"Installed Tooling Skill: `{tooling}`\n\n")
    work_evidence = report.read_text(encoding="utf-8")
    tooling_section = ("\n## Tooling CLI smoke\n\n```json\n" +
                       json.dumps(tooling_evidence, ensure_ascii=False, indent=2) + "\n```\n")
    report.write_text(header + work_evidence + tooling_section, encoding="utf-8")


def exercise_package(package, root, platform, source_install=False, smoke_report=None, zip_path=None):
    host = json.loads((package / "release-info.json").read_text(encoding="utf-8"))["host"]
    if host not in ("codex", "claude"):
        raise AssertionError(f"unsupported release Host: {host!r}")
    exercise_started = time.monotonic()
    def log_phase(phase):
        set_phase(phase)
        print(f"Package E2E phase: host={host}, platform={platform}, phase={phase}, "
              f"elapsed={time.monotonic() - exercise_started:.1f}s", flush=True)

    log_phase("installer CLI checks")
    skill_path = ".agents/skills/work" if host == "codex" else ".claude/skills/work"
    instructions_path = ".codex/AGENTS.md" if host == "codex" else ".claude/CLAUDE.md"
    instructions_dir = ".codex" if host == "codex" else ".claude"
    def run_installer(script, home, options=(), answers=None, extra_env=None, stream_output=False):
        return run_installer_for_host(script, home, options, answers, extra_env, stream_output, host=host)

    def assert_state(work, version, workspace_version, global_agents, global_dir=None):
        return assert_state_for_host(work, version, workspace_version, global_agents, global_dir, host=host)
    suffix = ".bat" if platform == "windows-x64-msvc" else ".command"
    script = package / f"install-agent-work-kit-from-package{suffix}"
    source = Path(__file__).resolve().parents[3] / "os-scripts" / (
        "windows" if suffix == ".bat" else "mac") / f"install-agent-work-kit-from-source{suffix}"
    if not script.is_file() or not source.is_file():
        raise AssertionError("a formal installer entry is missing")
    for entry in (script, source):
        log_phase(f"CLI checks for {entry.name}")
        help_home = root / f"help {entry.name}"
        help_home.mkdir()
        for option in ("--help", "-h"):
            result = run_installer(entry, help_home, [option])
            assert_success(result)
            if not all(flag.encode() in result[1] for flag in CLI_OPTIONS):
                raise AssertionError(f"{entry.name} {option} omitted a CLI option: {result!r}")
        dry = run_installer(entry, help_home,
            ["--non-interactive", "--global-instructions", "no", "--dry-run"])
        assert_success(dry)
        if b"Version:" not in dry[1] or b"Tooling target:" not in dry[1] or b"Dry run:" not in dry[1]:
            raise AssertionError(f"{entry.name} dry run omitted its summary: {dry!r}")
        verbose = run_installer(entry, help_home,
            ["--non-interactive", "--global-instructions", "no", "--dry-run", "--verbose"])
        assert_success(verbose)
        if b"Installer" not in verbose[1] or b"Dry run:" not in verbose[1]:
            raise AssertionError(f"{entry.name} verbose dry run omitted diagnostics: {verbose!r}")
        for invalid in (["--unknown"], ["--install-dir", "old/work"], ["--skills"],
                        ["--skills", "unknown"], ["--list-skills"],
                        ["--work-install-dir"], ["--skills-dir"], ["--instructions-dir"],
                        ["--global-instructions", "yes", "--global-instructions", "no"],
                        ["--verbose", "--verbose"], ["--help", "-h"],
                        ["--yes"], ["--force"]):
            assert_failed(run_installer(entry, help_home, invalid), 2)
        if any((help_home / name).exists() for name in (".agents", ".codex", ".claude")):
            raise AssertionError("read-only CLI actions wrote installation data")

    log_phase("package installation and state checks")
    home = root / "default install"
    home.mkdir()
    work = home / skill_path
    common = ["--non-interactive", "--global-instructions", "no"]
    release_info = json.loads((package / "release-info.json").read_text(encoding="utf-8"))
    version = release_info["version"]
    workspace_version = release_info["rust_workspace_version"]
    mismatch_home = root / "wrong Host selection"
    mismatch_home.mkdir()
    wrong_host = "claude" if host == "codex" else "codex"
    assert_no_target(run_installer(script, mismatch_home,
        ["--host", wrong_host, *common]), mismatch_home, mismatch_home / skill_path)
    assert_success(run_installer(script, home, [*common, "--dry-run"]))
    if work.exists():
        raise AssertionError("dry run created Work")
    assert_success(run_installer(script, home, common))
    assert_state(work, version, workspace_version, False)
    assert_no_work_transactions(work)
    if (home / instructions_path).exists():
        raise AssertionError("disabled Global AGENTS created a target")
    if smoke_report is not None:
        log_phase("installed Work CLI public contract and TASK smoke")
        binary = work / "scripts" / ("work.exe" if sys.platform == "win32" else "work")
        run_installed_cli_smoke(binary, smoke_report, zip_path, version, host, platform)

    default_home = root / "first install defaults"
    default_home.mkdir()
    assert_success(run_installer(script, default_home, ["--non-interactive"]))
    assert_state(default_home / skill_path, version, workspace_version, True, default_home / instructions_dir)
    if (default_home / instructions_path).read_bytes() != (package / "instructions/global-instructions.md").read_bytes():
        raise AssertionError("first installation did not enable the default Global AGENTS")

    saved_home = root / "saved Global AGENTS directory"
    saved_home.mkdir()
    saved_work = saved_home / skill_path
    saved_global = root / "自訂 Global AGENTS & path"
    saved_options = ["--non-interactive", "--global-instructions", "yes",
                     "--instructions-dir", str(saved_global)]
    assert_success(run_installer(script, saved_home, saved_options))
    assert_state(saved_work, version, workspace_version, True, saved_global)
    assert_success(run_installer(script, saved_home, ["--non-interactive"]))
    assert_state(saved_work, version, workspace_version, True, saved_global)
    if (saved_home / instructions_path).exists():
        raise AssertionError("saved Global AGENTS directory was replaced by the default")

    empty_home = root / "empty Work directory"
    empty_home.mkdir()
    empty_work = empty_home / skill_path
    empty_work.mkdir(parents=True)
    assert_success(run_installer(script, empty_home, common))
    assert_state(empty_work, version, workspace_version, False)
    assert_no_work_transactions(empty_work)

    cancelled_home = root / "cancelled installation"
    cancelled_home.mkdir()
    cancelled_work = cancelled_home / skill_path
    assert_no_target(run_installer(script, cancelled_home,
        ["--global-instructions", "no"], answers="\nn\n"), cancelled_home, cancelled_work)

    conflict_home = root / "Global AGENTS conflict"
    conflict_home.mkdir()
    global_target = conflict_home / instructions_path
    global_target.parent.mkdir()
    global_target.write_bytes(b"user-owned Global AGENTS\r\n")
    conflict_work = conflict_home / skill_path
    assert_failed(run_installer(script, conflict_home,
        ["--non-interactive", "--global-instructions", "yes"]))
    if conflict_work.exists() or global_target.read_bytes() != b"user-owned Global AGENTS\r\n":
        raise AssertionError("Global AGENTS conflict changed existing data")
    assert_success(run_installer(script, conflict_home,
        ["--global-instructions", "yes"], answers="\n\ny\ny\n"))
    assert_state(conflict_work, version, workspace_version, True, conflict_home / instructions_dir)
    if global_target.read_bytes() != (package / "instructions/global-instructions.md").read_bytes():
        raise AssertionError("confirmed Global AGENTS replacement did not publish source bytes")
    assert_no_work_transactions(conflict_work)

    retained_home = root / "retained Global AGENTS"
    retained_home.mkdir()
    retained_target = retained_home / instructions_path
    retained_target.parent.mkdir()
    retained_target.write_bytes(b"keep user Global AGENTS\n")
    assert_success(run_installer(script, retained_home,
        ["--global-instructions", "yes"], answers="\n\nn\ny\n"))
    assert_state(retained_home / skill_path, version, workspace_version, True, retained_home / instructions_dir)
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
    custom_options = [*common, "--skills-dir", str(custom.parent)]
    assert_success(run_installer(script, custom_home, custom_options))
    assert_state(custom, version, workspace_version, False)
    for mode in ("task", "execute"):
        path = custom / "references/instructions" / mode / "web/frontend/css/tailwind/instructions.md"
        if not path.is_file():
            raise AssertionError(f"all hierarchy omitted {path}")
    assert_no_work_transactions(custom)

    migration_home = root / "migrate default Work"
    migration_home.mkdir()
    old_work = migration_home / skill_path
    shutil.copytree(work, old_work)
    old_binary = (old_work / "scripts" / ("work.exe" if suffix == ".bat" else "work")).read_bytes()
    moved_work = root / "migrated Work" / "work"
    assert_success(run_installer(script, migration_home,
        ["--non-interactive", "--global-instructions", "no", "--skills-dir", str(moved_work.parent)]))
    if (old_work / "scripts" / ("work.exe" if suffix == ".bat" else "work")).read_bytes() != old_binary:
        raise AssertionError("independent installation changed the previous Work")
    assert_state(moved_work, version, workspace_version, False)
    assert_no_work_transactions(moved_work)

    if source_install:
        log_phase("source installation")
        source_home = root / "source install"
        source_home.mkdir()
        source_work = source_home / skill_path
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
                                     ["--global-instructions", "no"],
                                     answers="\ny\n", extra_env=source_environment,
                                     stream_output=True))
        assert_state(source_interactive_home / skill_path, version, workspace_version, False)

    log_phase("update and invalid state checks")
    first_update = root / "first update"
    first_update.mkdir()
    legacy = first_update / skill_path
    shutil.copytree(work, legacy)
    legacy_state = legacy / "install-state.json"
    legacy_state.write_bytes(b"legacy state\n")
    rejected = run_installer(script, first_update, common)
    assert_failed(rejected)
    assert_failed(run_installer(script, first_update, ["--global-instructions", "no"], answers="\nYES\ny\n"))
    if not (legacy / "scripts" / ("work.exe" if suffix == ".bat" else "work")).is_file() or legacy_state.read_bytes() != b"legacy state\n":
        raise AssertionError("legacy Work state or binary changed after rejection")

    declined_home = root / "declined first update"
    declined_home.mkdir()
    declined = declined_home / skill_path
    shutil.copytree(work, declined)
    prior_binary = (declined / "scripts" / ("work.exe" if suffix == ".bat" else "work")).read_bytes()
    assert_failed(run_installer(script, declined_home,
        ["--global-instructions", "no", "--skills-dir", str(declined.parent)], answers="no\n"))
    if (declined / "install-state.json").exists() or (declined / "scripts" / ("work.exe" if suffix == ".bat" else "work")).read_bytes() != prior_binary:
        raise AssertionError("declined first update changed Work")

    invalid_home = root / "invalid install state"
    invalid_home.mkdir()
    invalid_work = invalid_home / skill_path
    shutil.copytree(work, invalid_work)
    invalid_state = invalid_work / "install-state.json"
    invalid_state.write_bytes(b"{invalid json\n")
    assert_failed(run_installer(script, invalid_home, common))
    if invalid_state.read_bytes() != b"{invalid json\n":
        raise AssertionError("noninteractive first update changed invalid state")
    assert_failed(run_installer(script, invalid_home,
        ["--global-instructions", "no"], answers="\nYES\ny\n"))
    if invalid_state.read_bytes() != b"{invalid json\n":
        raise AssertionError("interactive attempt changed invalid legacy state")

    for name, raw in (("empty", b""), ("missing field", b'{"version":"1.0.5"}'),
                      ("incompatible", b'{"schema_version":999}')):
        case_home = root / f"invalid state {name}"
        case_home.mkdir()
        case_work = case_home / skill_path
        case_work.mkdir(parents=True)
        marker = case_work / "SKILL.md"
        marker.write_bytes(b"existing Work\n")
        state_file = case_work.parent / "install-state.json"
        shutil.copyfile(work.parent / "install-state.json", state_file)
        state_file.write_bytes(raw)
        assert_failed(run_installer(script, case_home, common))
        if state_file.read_bytes() != raw or marker.read_bytes() != b"existing Work\n":
            raise AssertionError(f"{name} state changed on noninteractive rejection")
        assert_failed(run_installer(script, case_home,
            ["--global-instructions", "no", "--skills-dir", str(case_work.parent)],
            answers="YES\ny\n"))
        if state_file.read_bytes() != raw or marker.read_bytes() != b"existing Work\n":
            raise AssertionError(f"{name} state changed on interactive rejection")

    unknown_home = root / "unknown contents"
    unknown_home.mkdir()
    unknown = unknown_home / skill_path
    unknown.mkdir(parents=True)
    marker = unknown / "user-notes.txt"
    marker.write_text("keep", encoding="utf-8")
    assert_failed(run_installer(script, unknown_home, common))
    unknown_result = run_installer(script, unknown_home,
        ["--global-instructions", "no", "--skills-dir", str(unknown.parent)], answers="y\n")
    assert_failed(unknown_result)
    diagnostic = unknown_result[1] + unknown_result[2]
    if b"parent install-state.json" not in diagnostic or str(unknown).encode() not in diagnostic:
        raise AssertionError("unsafe Work diagnostic omitted the unmanaged target")
    if marker.read_text(encoding="utf-8") != "keep" or (unknown / "install-state.json").exists():
        raise AssertionError("unknown Work content was modified")

    link_home = root / "unsafe managed link"
    link_home.mkdir()
    linked_work = link_home / skill_path
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
        ["--global-instructions", "no", "--skills-dir", str(linked_work.parent)],
        answers="y\n", stream_output=True))
    if (outside / "marker.txt").read_text(encoding="utf-8") != "keep" or not linked_directory.exists():
        raise AssertionError("unsafe Work link or linked data changed after rejection")

    log_phase("tampered package checks")
    for scenario in ("hash", "extra", "missing", "missing-options", "duplicate-manifest", "unsafe-manifest"):
        bad = root / f"bad {scenario}"
        shutil.copytree(package, bad)
        if scenario == "hash":
            (bad / "instructions/global-instructions.md").write_bytes(b"tampered")
        elif scenario == "extra":
            (bad / "extra.txt").write_bytes(b"extra")
        elif scenario == "missing":
            (bad / "instructions/global-instructions.md").unlink()
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
        target = bad_home / skill_path
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
                extra_env={"PATH": f"{shim}:/usr/bin:/bin"}), wrong, wrong / skill_path)
    else:
        wrong = root / "wrong Windows architecture"
        wrong.mkdir()
        assert_no_target(run_installer(script, wrong, common,
            extra_env={"PROCESSOR_ARCHITECTURE": "x86"}), wrong, wrong / skill_path)

    log_phase("complete")
    print(f"Package native E2E passed: {host}, {platform}, version {version}, {script.name}", flush=True)
def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--zip", required=True, type=Path)
    parser.add_argument("--checksums", required=True, type=Path)
    parser.add_argument("--platform", required=True, choices=("macos-arm64", "windows-x64-msvc"))
    parser.add_argument("--source-install", action="store_true", help="also build and install through Source")
    parser.add_argument("--peer-zip", type=Path, help="exercise both Host ZIPs in one home")
    parser.add_argument("--cli-smoke-report", type=Path,
                        help="save public contract and TASK smoke evidence from the installed CLI")
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
        verify_cli_entries(package, args.platform)
        (root / "original.zip").write_bytes(args.zip.read_bytes())
        if args.peer_zip:
            peer = root / "peer-package"
            peer.mkdir()
            extract_checked(args.peer_zip, peer)
            verify_cli_entries(peer, args.platform)
            packages = {json.loads((path / "release-info.json").read_text(encoding="utf-8"))["host"]: path
                        for path in (package, peer)}
            if len(packages) != 2:
                raise AssertionError("coexistence ZIPs have the same Host")
            exercise_coexistence(packages, root, args.platform)
        else:
            exercise_package(package, root, args.platform, args.source_install,
                             args.cli_smoke_report, args.zip)
    print(f"Final ZIP E2E passed: {args.platform}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
