"""Native Windows loopback fixtures for the standalone Release downloader."""

import os
import json
import stat
import subprocess
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[6]
sys.path.insert(0, str(ROOT / ".github/scripts/release"))
sys.path.insert(0, str(ROOT / ".github/scripts/tests/release/common"))
from downloader_fixture import build_test_downloaders
from downloader_server import start_server, write_assets, write_checksum

TAG = f"v{(ROOT / 'VERSION').read_text(encoding='ascii').strip()}"
ARCHIVE = f"work-{TAG}-windows-x64-msvc.zip"
PACKAGE = b'@echo off\r\npowershell.exe -NoProfile -File "%~dp0capture.ps1" %*\r\nexit /b %errorlevel%\r\n'
CAPTURE = b'[IO.File]::WriteAllLines($env:WORK_RELEASE_TEST_ARGV, [string[]]$args, [Text.UTF8Encoding]::new($false))\r\nexit [int]$env:WORK_RELEASE_TEST_EXIT\r\n'


class DownloaderTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="work-downloader-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.downloader = self.root / "downloaders/install-work-from-github-release.bat"
        build_test_downloaders(ROOT, self.downloader.parent)
        self.assets = self.root / TAG
        self.assets.mkdir()
        self.scratch = self.root / "scratch"
        self.scratch.mkdir()
        self.server = start_server(self.root, ARCHIVE, self.addCleanup)
        self.environment = os.environ.copy()
        self.environment.update({
            "TEMP": str(self.scratch),
            "TMP": str(self.scratch),
            "WORK_RELEASE_TEST_MODE": "1",
            "WORK_RELEASE_TEST_BASE_URL": f"http://127.0.0.1:{self.server.server_port}",
            "WORK_RELEASE_TEST_ARGV": str(self.root / "argv.txt"),
            "WORK_RELEASE_TEST_EXIT": "0",
        })

    def package(self, members=None):
        if members is None:
            members = {"install-work-from-package.bat": PACKAGE, "capture.ps1": CAPTURE}
        members = {**members}
        members.setdefault("release-info.json", json.dumps({"tag": TAG}).encode("ascii"))
        write_assets(self.assets, ARCHIVE, members, 0o644)

    def run_downloader(self, *options, environment=None):
        result = subprocess.run(
            ["cmd.exe", "/D", "/C", str(self.downloader), *options],
            env=environment or self.environment,
            stdin=subprocess.DEVNULL,
            capture_output=True,
            timeout=35,
        )
        self.assertEqual([], list(self.scratch.glob("work-release-*")), result.stderr.decode(errors="replace"))
        return result

    def test_success_forwarding_and_downstream_status(self):
        self.package()
        location = str(self.root / "中文 path & symbols")
        global_dir = str(self.root / "全域 path & symbols")
        result = self.run_downloader("--skills-install-dir", location,
                                     "--global-agent-install-dir", global_dir)
        self.assertEqual(0, result.returncode, (result.stdout, result.stderr))
        self.assertEqual(
            ["--skills-install-dir", location,
             "--global-agent-install-dir", global_dir],
            (self.root / "argv.txt").read_text(encoding="utf-8").splitlines(),
        )
        env = {**self.environment, "WORK_RELEASE_TEST_EXIT": "17"}
        self.assertEqual(17, self.run_downloader(environment=env).returncode)
        self.assertEqual(17, self.run_downloader("--unknown", environment=env).returncode)

    def test_help_does_not_download(self):
        result = self.run_downloader("--help")
        self.assertEqual(0, result.returncode)
        self.assertFalse((self.root / "argv.txt").exists())

    def test_missing_and_tampered_assets(self):
        self.assertNotEqual(0, self.run_downloader().returncode)
        self.assertEqual(1, self.server.requests)
        self.assertFalse((self.root / "argv.txt").exists())
        self.package()
        (self.assets / ARCHIVE).unlink()
        before = self.server.requests
        self.assertNotEqual(0, self.run_downloader().returncode)
        self.assertEqual(before + 2, self.server.requests)
        self.assertFalse((self.root / "argv.txt").exists())
        self.package()
        (self.assets / "SHA256SUMS.txt").unlink()
        before = self.server.requests
        self.assertNotEqual(0, self.run_downloader().returncode)
        self.assertEqual(before + 1, self.server.requests)
        self.assertFalse((self.root / "argv.txt").exists())
        self.package()
        (self.assets / "SHA256SUMS.txt").write_text("invalid\n", encoding="ascii")
        self.assertNotEqual(0, self.run_downloader().returncode)
        self.assertFalse((self.root / "argv.txt").exists())
        self.package()
        (self.assets / ARCHIVE).write_bytes(b"tampered")
        self.assertNotEqual(0, self.run_downloader().returncode)
        self.package()
        (self.assets / ARCHIVE).write_bytes(b"not a ZIP")
        write_checksum(self.assets, ARCHIVE)
        self.assertNotEqual(0, self.run_downloader().returncode)

    def test_other_release_tag_is_not_used(self):
        other = self.root / "v999.0.0"
        other.mkdir()
        write_assets(other, ARCHIVE, {"install-work-from-package.bat": PACKAGE}, 0o644)
        self.assertNotEqual(0, self.run_downloader().returncode)
        self.assertEqual(1, self.server.requests)
        self.assertFalse((self.root / "argv.txt").exists())

    def test_package_release_tag_mismatch(self):
        self.package({"install-work-from-package.bat": PACKAGE,
                      "release-info.json": b'{"tag":"v999.0.0"}'})
        self.assertNotEqual(0, self.run_downloader().returncode)
        self.assertFalse((self.root / "argv.txt").exists())

    def test_interrupted_transfer(self):
        self.package()
        self.server.interrupt_archive = True
        self.assertNotEqual(0, self.run_downloader().returncode)
        self.assertEqual(2, self.server.requests)

    def test_abandoned_owned_scratch_is_removed(self):
        stale = self.scratch / "work-release-123456789"
        stale.mkdir()
        (stale / ".work-release-owner").write_text("work-release-v1|99999999", encoding="ascii")
        (stale / "SHA256SUMS.txt").write_text("partial", encoding="ascii")
        self.package()
        self.assertEqual(0, self.run_downloader().returncode)
        self.assertFalse(stale.exists())

    def test_unsafe_zip_member(self):
        self.package({"../outside": b"escape"})
        self.assertNotEqual(0, self.run_downloader().returncode)
        self.assertFalse((self.root / "outside").exists())
        with zipfile.ZipFile(self.assets / ARCHIVE, "w") as archive:
            info = zipfile.ZipInfo("install-work-from-package.bat")
            info.create_system = 3
            info.external_attr = (stat.S_IFLNK | 0o777) << 16
            archive.writestr(info, "outside")
        write_checksum(self.assets, ARCHIVE)
        self.assertNotEqual(0, self.run_downloader().returncode)


if __name__ == "__main__":
    if sys.platform != "win32":
        raise SystemExit("native Windows runner required")
    unittest.main()
