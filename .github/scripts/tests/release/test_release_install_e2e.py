"""Tests for the README download and metadata checks before installation."""

import hashlib
import io
import json
import sys
import tempfile
import unittest
import zipfile
from contextlib import redirect_stdout
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "release"))
from release_install_e2e import run_installer, verify_readme_download


class ReleaseInstallE2ETests(unittest.TestCase):
    def test_installer_output_is_retained_for_assertions(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            if sys.platform == "win32":
                script = home / "installer.bat"
                script.write_bytes(b"@echo off\r\necho Version: test\r\necho diagnostic 1>&2\r\n")
            else:
                script = home / "installer.sh"
                script.write_bytes(b"printf 'Version: test\\n'\nprintf 'diagnostic\\n' >&2\n")
            code, stdout, stderr = run_installer(script, home, answers="y\n")
            self.assertEqual(code, 0)
            self.assertIn(b"Version: test", stdout)
            self.assertIn(b"diagnostic", stderr)

    def test_interrupted_installer_logs_captured_output(self):
        with tempfile.TemporaryDirectory() as directory:
            def interrupt(*_args, **kwargs):
                kwargs["stdout"].write(b"Replace Global AGENTS.md? (y/n)\n")
                kwargs["stderr"].write(b"installer diagnostic\n")
                raise KeyboardInterrupt

            log = io.StringIO()
            with patch("release_install_e2e.subprocess.run", side_effect=interrupt):
                with redirect_stdout(log), self.assertRaises(KeyboardInterrupt):
                    run_installer(Path("installer"), Path(directory))
            self.assertIn("reason=KeyboardInterrupt", log.getvalue())
            self.assertIn("Replace Global AGENTS.md? (y/n)", log.getvalue())
            self.assertIn("installer diagnostic", log.getvalue())

    def test_outer_digest_and_runner_metadata(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive = root / "work-v1.2.3-macos-arm64.zip"
            checksums = root / "SHA256SUMS.txt"
            environment = {field: "value" for field in (
                "runner_os", "cpu_architecture", "runner_image_name", "runner_image_version",
                "os_name", "os_version", "os_build",
            )}
            environment["runner_label"] = "macos-latest"
            with zipfile.ZipFile(archive, "w") as stream:
                stream.writestr("release-info.json", json.dumps({"tested_environment": environment}))
            digest = hashlib.sha256(archive.read_bytes()).hexdigest()
            checksums.write_text(f"{digest}  {archive.name}\n", encoding="ascii")
            verify_readme_download(archive, checksums, "macos-arm64")
            checksums.write_text(f"{'0' * 64}  {archive.name}\n", encoding="ascii")
            with self.assertRaisesRegex(ValueError, "digest"):
                verify_readme_download(archive, checksums, "macos-arm64")
            checksums.write_text(f"{digest}  {archive.name}\n", encoding="ascii")
            with self.assertRaisesRegex(ValueError, "runner label"):
                verify_readme_download(archive, checksums, "windows-x64-msvc")


if __name__ == "__main__":
    unittest.main()
