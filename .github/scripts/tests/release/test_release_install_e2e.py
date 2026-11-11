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
from release_install_e2e import assert_state_for_host, exercise_package, run_installer_for_host, snapshot_host, terminate_process_tree, verify_readme_download


class ReleaseInstallE2ETests(unittest.TestCase):
    def test_host_snapshot_includes_parent_state_and_instructions(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            for host, skills_path, instructions_path in (
                ("codex", ".agents/skills", ".codex/AGENTS.md"),
                ("claude", ".claude/skills", ".claude/CLAUDE.md"),
            ):
                skills = home / skills_path
                instructions = home / instructions_path
                skills.mkdir(parents=True, exist_ok=True)
                instructions.parent.mkdir(parents=True, exist_ok=True)
                (skills / "install-state.json").write_text(host, encoding="utf-8")
                (skills / "work").mkdir()
                (skills / "work/SKILL.md").write_text(host, encoding="utf-8")
                instructions.write_text(host, encoding="utf-8")
                original = snapshot_host(home, host)
                instructions.write_text(host + " updated", encoding="utf-8")
                self.assertNotEqual(snapshot_host(home, host), original)
                instructions.unlink()
                with self.assertRaisesRegex(AssertionError, "no global instructions"):
                    snapshot_host(home, host)

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

    def test_claude_state_is_accepted_only_for_claude(self):
        with tempfile.TemporaryDirectory() as directory:
            work = Path(directory) / "skills/work"
            work.mkdir(parents=True)
            state = {"host": "claude", "version": "1.2.3", "rust_workspace_version": "1.2.3",
                     "skill_install_dir": str(work.parent),
                     "global_instructions_dir": None, "status": "installed"}
            (work.parent / "install-state.json").write_text(json.dumps(state), encoding="utf-8")
            with self.assertRaisesRegex(AssertionError, "unexpected Skills state"):
                assert_state_for_host(work, "1.2.3", "1.2.3", False, host="codex")

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

            log = io.StringIO()
            with patch("release_install_e2e.terminate_process_tree", return_value=True):
                with redirect_stdout(log), self.assertRaises(KeyboardInterrupt):
                    run_installer_for_host(Path("installer"), Path(directory), host="codex",
                                           popen_factory=InterruptedProcess)
            self.assertIn("reason=KeyboardInterrupt", log.getvalue())
            self.assertIn("pid=1234, phase=", log.getvalue())
            self.assertIn("child_exit_at_cancel=None", log.getvalue())
            self.assertIn("Replace Global AGENTS.md? (y/n)", log.getvalue())
            self.assertIn("installer diagnostic", log.getvalue())

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
