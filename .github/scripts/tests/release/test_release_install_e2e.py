"""Tests for the README download and metadata checks before installation."""

import hashlib
import io
import json
import subprocess
import sys
import tempfile
import time
import unittest
import zipfile
from contextlib import redirect_stdout
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "release"))
from release_install_e2e import HostLayout, assert_installation, exercise_package, register_windows_e2e_clis, run_installer_for_host, terminate_process_tree, verify_readme_download


class ReleaseInstallE2ETests(unittest.TestCase):
    def test_host_layout_snapshot_covers_skills_cli_and_instructions(self):
        for platform in ("macos-arm64", "windows-x64-msvc"):
            for host, skills_path, instructions_path in (
                ("codex", ".agents/skills", ".codex/AGENTS.md"),
                ("claude", ".claude/skills", ".claude/CLAUDE.md"),
            ):
                with self.subTest(platform=platform, host=host), tempfile.TemporaryDirectory() as directory:
                    home = Path(directory)
                    layout = HostLayout(home, host, platform)
                    self.assertEqual(layout.discovery, home / skills_path)
                    self.assertEqual(layout.skills, layout.install / "skills")
                    self.assertEqual(layout.global_file, home / instructions_path)
                    self.assertEqual(layout.cli, layout.install)
                    self.assertEqual(layout.work.suffix, ".exe" if platform == "windows-x64-msvc" else "")
                    self.assertEqual(layout.snapshot(), {})
                    (layout.skills / "work").mkdir(parents=True)
                    (layout.skills / "work/SKILL.md").write_text(host, encoding="utf-8")
                    (layout.cli / "bin").mkdir(parents=True)
                    layout.work.write_text(host, encoding="utf-8")
                    layout.global_dir.mkdir(parents=True, exist_ok=True)
                    layout.global_file.write_text(host, encoding="utf-8")
                    original = layout.snapshot()
                    self.assertEqual(len(original), 3)
                    layout.global_file.write_text(host + " updated", encoding="utf-8")
                    self.assertNotEqual(layout.snapshot(), original)

    def test_both_platforms_pass_zip_host_into_native_installer(self):
        for platform, suffix in (("macos-arm64", ".command"), ("windows-x64-msvc", ".bat")):
            for host in ("codex", "claude"):
                with self.subTest(platform=platform, host=host), tempfile.TemporaryDirectory() as directory:
                    root = Path(directory)
                    package = root / "package"
                    package.mkdir()
                    (package / "release-info.json").write_text(json.dumps({"host": host}), encoding="utf-8")
                    (package / f"install-agent-work-kit-from-package{suffix}").touch()
                    with patch("release_install_e2e.run_installer_for_host",
                               side_effect=RuntimeError("installer reached")) as installer:
                        with self.assertRaisesRegex(RuntimeError, "installer reached"):
                            exercise_package(package, root, platform)
                    self.assertEqual(installer.call_args.kwargs["host"], host)

    def test_windows_cli_registration_uses_concrete_binary_paths(self):
        with tempfile.TemporaryDirectory() as directory:
            program = Path(directory) / "work.exe"
            program.write_bytes(b"binary")
            recorded = []

            def register(command, **kwargs):
                self.assertIn("register_programs.ps1", " ".join(command))
                self.assertEqual(command[-4], "-Rule")
                self.assertEqual(command[-3], "unit-test")
                self.assertEqual(command[-2], "-PathsFile")
                recorded.extend(json.loads(Path(command[-1]).read_text(encoding="utf-8")))
                return subprocess.CompletedProcess(command, 0, "", "")

            with patch.dict("os.environ", {"AGENT_WORK_KIT_E2E_RULE_PREFIX": "unit-test"}):
                with patch("release_install_e2e.sys.platform", "win32"):
                    with patch("release_install_e2e.subprocess.run", side_effect=register):
                        register_windows_e2e_clis(program, program, Path(directory) / "missing.exe")
            self.assertEqual(recorded, [str(program.resolve())])

    def test_windows_cli_registration_is_inert_without_isolation(self):
        with patch.dict("os.environ", {"AGENT_WORK_KIT_E2E_RULE_PREFIX": ""}):
            with patch("release_install_e2e.subprocess.run") as run:
                register_windows_e2e_clis(Path("work.exe"))
                run.assert_not_called()

    def test_installer_output_is_retained_for_assertions(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            if sys.platform == "win32":
                script = home / "installer.bat"
                script.write_bytes(b"@echo off\r\necho Version: test\r\necho diagnostic 1>&2\r\n")
            else:
                script = home / "installer.sh"
                script.write_bytes(b"printf 'Version: test\\n'\nprintf 'diagnostic\\n' >&2\n")
            code, stdout, stderr = run_installer_for_host(script, home, answers="y\n", host="codex")
            self.assertEqual(code, 0)
            self.assertIn(b"Version: test", stdout)
            self.assertIn(b"diagnostic", stderr)

    def test_installation_state_is_accepted_only_for_its_own_host_and_resources(self):
        with tempfile.TemporaryDirectory() as directory:
            layout = HostLayout(Path(directory), "claude", "macos-arm64")
            layout.install.mkdir(parents=True)
            state = {"host": "claude", "version": "1.2.3", "rust_workspace_version": "1.2.3",
                     "install_dir": str(layout.install),
                     "global_instructions_dir": None, "status": "installed", "files": []}
            layout.state.write_text(json.dumps(state), encoding="utf-8")
            info = {"version": "1.2.3", "rust_workspace_version": "1.2.3"}
            layout.state.write_text(json.dumps({**state, "host": "codex"}), encoding="utf-8")
            with self.assertRaisesRegex(AssertionError, "unexpected installation state"):
                assert_installation(layout, info, False)
            layout.state.write_text(json.dumps(state), encoding="utf-8")
            with self.assertRaisesRegex(AssertionError, "omitted work/SKILL.md"):
                assert_installation(layout, info, False)

    def test_claude_host_is_passed_to_installer(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            if sys.platform == "win32":
                script = home / "installer.bat"
                script.write_bytes(b"@echo off\r\necho %*\r\n")
            else:
                script = home / "installer.sh"
                script.write_bytes(b"printf '%s\\n' \"$*\"\n")
            code, stdout, _ = run_installer_for_host(script, home, host="claude")
            self.assertEqual(code, 0)
            self.assertIn(b"--host claude", stdout)

    def test_interrupted_installer_logs_captured_output(self):
        with tempfile.TemporaryDirectory() as directory:
            class InterruptedProcess:
                pid = 1234

                def __init__(self, *_args, **kwargs):
                    self.stdout = kwargs["stdout"]
                    self.stderr = kwargs["stderr"]

                def __enter__(self):
                    return self

                def __exit__(self, *_args):
                    return False

                def communicate(self, **_kwargs):
                    self.stdout.write(b"Replace Global AGENTS.md? (y/n)\n")
                    self.stderr.write(b"installer diagnostic\n")
                    raise KeyboardInterrupt

                def poll(self):
                    return None

            for cleanup_ok in (True, False):
                log = io.StringIO()
                with self.subTest(cleanup_ok=cleanup_ok):
                    with patch("release_install_e2e.terminate_process_tree", return_value=cleanup_ok):
                        with redirect_stdout(log), self.assertRaises(KeyboardInterrupt):
                            run_installer_for_host(Path("installer"), Path(directory), host="codex",
                                                   popen_factory=InterruptedProcess)
                    self.assertIn("reason=KeyboardInterrupt", log.getvalue())
                    self.assertIn("pid=1234, phase=", log.getvalue())
                    self.assertIn("child_exit_at_cancel=None", log.getvalue())
                    self.assertIn("Replace Global AGENTS.md? (y/n)", log.getvalue())
                    self.assertIn("installer diagnostic", log.getvalue())
                    if not cleanup_ok:
                        self.assertIn("cleanup incomplete after cancellation", log.getvalue())

    def test_cancelled_installer_leaves_no_descendant_running(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            marker = home / "descendant-finished"
            if sys.platform == "win32":
                script = home / "installer.bat"
                script.write_bytes(
                    b"@echo off\r\nstart /b cmd.exe /D /C \"ping -n 6 127.0.0.1 >nul & echo done>"
                    + str(marker).encode() + b"\"\r\nping -n 30 127.0.0.1 >nul\r\n")
            else:
                script = home / "installer.sh"
                script.write_bytes(
                    b"(sleep 5; echo done > '" + str(marker).encode() + b"') &\nsleep 30\n")
            original_popen = subprocess.Popen

            class CancelledPopen(original_popen):
                def communicate(self, input=None, timeout=None):
                    if timeout is None:
                        return super().communicate(input=input)
                    time.sleep(1)
                    raise KeyboardInterrupt

            log = io.StringIO()
            with redirect_stdout(log), self.assertRaises(KeyboardInterrupt):
                run_installer_for_host(script, home, host="codex", popen_factory=CancelledPopen)
            self.assertIn("Installer E2E cancelled:", log.getvalue())
            self.assertNotIn("termination failed", log.getvalue())
            time.sleep(8)
            self.assertFalse(marker.exists(), "installer descendant kept running after cancellation")

    def test_timed_out_installer_cleanup_is_bounded(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            started_marker = home / "descendant-started"
            finished_marker = home / "descendant-finished"
            if sys.platform == "win32":
                script = home / "installer.bat"
                script.write_bytes(
                    b"@echo off\r\nstart /b cmd.exe /D /C \"echo started>"
                    + str(started_marker).encode() + b" & ping -n 6 127.0.0.1 >nul & echo done>"
                    + str(finished_marker).encode() + b"\"\r\nping -n 30 127.0.0.1 >nul\r\n")
            else:
                script = home / "installer.sh"
                script.write_bytes(b"(echo started > '" + str(started_marker).encode()
                                   + b"'; sleep 5; echo done > '" + str(finished_marker).encode()
                                   + b"') &\nsleep 30\n")
            original_popen = subprocess.Popen

            class TimedOutPopen(original_popen):
                def communicate(self, input=None, timeout=None):
                    deadline = time.monotonic() + 5
                    while not started_marker.exists() and time.monotonic() < deadline:
                        time.sleep(0.05)
                    if not started_marker.exists():
                        raise AssertionError("installer descendant did not start")
                    raise subprocess.TimeoutExpired(self.args, timeout)

            started = time.monotonic()
            with self.assertRaises(subprocess.TimeoutExpired):
                run_installer_for_host(script, home, host="codex", popen_factory=TimedOutPopen)
            self.assertLess(time.monotonic() - started, 15, "timeout cleanup took too long")
            time.sleep(7)
            self.assertFalse(finished_marker.exists(), "installer descendant kept running after timeout")

    def test_parent_exit_before_cleanup_is_reported_on_windows(self):
        if sys.platform != "win32":
            self.skipTest("Windows taskkill requires a live parent PID")

        class ExitedParent:
            pid = 1234

            def poll(self):
                return 0

        log = io.StringIO()
        with redirect_stdout(log):
            self.assertFalse(terminate_process_tree(ExitedParent()))
        self.assertIn("cannot verify descendants", log.getvalue())

    @unittest.skipUnless(sys.platform == "win32", "Windows Job Object requires Windows")
    def test_windows_job_stops_descendant_after_parent_exits(self):
        from windows_job import CREATE_SUSPENDED, WindowsInstallerJob

        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            started = home / "descendant-started"
            finished = home / "descendant-finished"
            script = home / "installer.bat"
            script.write_bytes(
                b"@echo off\r\nstart /b cmd.exe /D /C \"echo started>"
                + str(started).encode() + b" & ping -n 6 127.0.0.1 >nul & echo done>"
                + str(finished).encode() + b"\"\r\nping -n 2 127.0.0.1 >nul\r\n")
            job = WindowsInstallerJob()
            process = None
            try:
                process = subprocess.Popen(["cmd.exe", "/D", "/C", str(script)], cwd=home,
                                           stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                           stderr=subprocess.DEVNULL,
                                           creationflags=subprocess.CREATE_NEW_PROCESS_GROUP | CREATE_SUSPENDED)
                job.attach_and_resume(process)
                process.wait(timeout=10)
                self.assertTrue(started.exists(), "descendant did not start before parent exit")
                self.assertGreater(job.active_processes(), 0, "descendant was not retained in the job")
                self.assertTrue(terminate_process_tree(process, job=job))
                self.assertEqual(job.active_processes(), 0)
                time.sleep(6)
                self.assertFalse(finished.exists(), "descendant kept running after Job Object cleanup")
            finally:
                job.close()
                if process is not None and process.poll() is None:
                    process.kill()
                    process.wait(timeout=10)

    @unittest.skipIf(sys.platform == "win32", "Windows reports unverifiable descendants after parent exit")
    def test_descendant_is_killed_after_parent_exits(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            marker = home / "descendant-finished"
            script = home / "installer.sh"
            script.write_bytes(b"(sleep 5; echo done > '" + str(marker).encode() + b"') &\n")
            original_popen = subprocess.Popen

            class ExitedPopen(original_popen):
                def communicate(self, input=None, timeout=None):
                    self.wait(timeout=5)
                    raise subprocess.TimeoutExpired(self.args, timeout)

            with self.assertRaises(subprocess.TimeoutExpired):
                run_installer_for_host(script, home, host="codex", popen_factory=ExitedPopen)
            time.sleep(7)
            self.assertFalse(marker.exists(), "descendant survived its parent and cleanup")

    def test_outer_digest_and_runner_metadata(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive = root / "agent-work-kit-v1.2.3-codex-macos-arm64.zip"
            checksums = root / "SHA256SUMS.txt"
            environment = {field: "value" for field in (
                "runner_os", "cpu_architecture", "runner_image_name", "runner_image_version",
                "os_name", "os_version", "os_build",
            )}
            environment["runner_label"] = "macos-latest"
            with zipfile.ZipFile(archive, "w") as stream:
                stream.writestr("release-info.json", json.dumps({"host": "codex", "target": "aarch64-apple-darwin",
                                                                "version": "1.2.3", "tested_environment": environment}))
            digest = hashlib.sha256(archive.read_bytes()).hexdigest()
            checksums.write_text(f"{digest}  {archive.name}\n", encoding="ascii")
            verify_readme_download(archive, checksums, "macos-arm64")
            checksums.write_text(f"{'0' * 64}  {archive.name}\n", encoding="ascii")
            with self.assertRaisesRegex(ValueError, "digest"):
                verify_readme_download(archive, checksums, "macos-arm64")
            checksums.write_text(f"{digest}  {archive.name}\n", encoding="ascii")
            with self.assertRaisesRegex(ValueError, "identity"):
                verify_readme_download(archive, checksums, "windows-x64-msvc")
            environment["runner_label"] = "windows-latest"
            with zipfile.ZipFile(archive, "w") as stream:
                stream.writestr("release-info.json", json.dumps({"host": "codex", "target": "aarch64-apple-darwin",
                                                                "version": "1.2.3", "tested_environment": environment}))
            digest = hashlib.sha256(archive.read_bytes()).hexdigest()
            checksums.write_text(f"{digest}  {archive.name}\n", encoding="ascii")
            with self.assertRaisesRegex(ValueError, "runner label"):
                verify_readme_download(archive, checksums, "macos-arm64")


if __name__ == "__main__":
    unittest.main()
