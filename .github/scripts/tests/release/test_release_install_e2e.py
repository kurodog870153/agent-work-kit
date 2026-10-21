"""Tests for the README download and metadata checks before installation."""

import hashlib
import json
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "release"))
from release_install_e2e import verify_readme_download


class ReleaseInstallE2ETests(unittest.TestCase):
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
