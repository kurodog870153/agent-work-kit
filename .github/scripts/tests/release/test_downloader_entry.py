"""Exercise the native Release downloader against shared loopback fixtures."""

import json
import os
import stat
import subprocess
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT / ".github/scripts/release"))
sys.path.insert(0, str(ROOT / ".github/scripts/tests/release/common"))
from downloader_fixture import build_test_downloaders
from downloader_server import start_server, write_assets, write_checksum


TAG = f"v{(ROOT / 'VERSION').read_text(encoding='ascii').strip()}"
WINDOWS_PACKAGE = b'@echo off\r\npowershell.exe -NoProfile -File "%~dp0capture.ps1" %*\r\nexit /b %errorlevel%\r\n'
WINDOWS_CAPTURE = b'[IO.File]::WriteAllLines($env:WORK_RELEASE_TEST_ARGV, [string[]]$args, [Text.UTF8Encoding]::new($false))\r\nexit [int]$env:WORK_RELEASE_TEST_EXIT\r\n'
MAC_PACKAGE = b'#!/bin/bash\nprintf "%s\\n" "$@" > "$WORK_RELEASE_TEST_ARGV"\nexit "${WORK_RELEASE_TEST_EXIT:-0}"\n'


@unittest.skipUnless(sys.platform in ("darwin", "win32"), "native macOS or Windows runner required")
class DownloaderTests(unittest.TestCase):
    def setUp(self):
        self.windows = sys.platform == "win32"
        self.temp = tempfile.TemporaryDirectory(prefix="work-downloader-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.platform = "windows-x64-msvc" if self.windows else "macos-arm64"
        self.archive = f"agent-work-kit-{TAG}-codex-{self.platform}.zip"
        self.entry = "install-agent-work-kit-from-package.bat" if self.windows else "install-agent-work-kit-from-package.command"
        self.downloader = self.root / "downloaders" / (
            "install-agent-work-kit-from-github-release.bat" if self.windows else "install-agent-work-kit-from-github-release.command")
        build_test_downloaders(ROOT, self.downloader.parent)
        self.assets = self.root / TAG
        self.assets.mkdir()
        self.scratch = self.root / "scratch"
        self.scratch.mkdir()
        self.server = start_server(self.root, self.archive, self.addCleanup)
        self.environment = os.environ.copy()
        self.environment.update({
            "WORK_RELEASE_TEST_MODE": "1",
            "WORK_RELEASE_TEST_BASE_URL": f"http://127.0.0.1:{self.server.server_port}",
            "WORK_RELEASE_TEST_ARGV": str(self.root / "argv.txt"),
            "WORK_RELEASE_TEST_EXIT": "0",
        })
        if self.windows:
            self.environment.update({"TEMP": str(self.scratch), "TMP": str(self.scratch)})
        else:
            shims = self.root / "shims"
            shims.mkdir()
            self.uname = shims / "uname"
            self.uname.write_text(
                '#!/bin/sh\ncase "$1" in -s) echo Darwin ;; -m) echo arm64 ;; *) /usr/bin/uname "$@" ;; esac\n',
                encoding="ascii",
            )
            self.uname.chmod(0o755)
            self.environment.update({"PATH": f"{shims}:/usr/bin:/bin", "TMPDIR": str(self.scratch)})

    def package(self, members=None):
        if members is None:
            members = ({"install-agent-work-kit-from-package.bat": WINDOWS_PACKAGE, "capture.ps1": WINDOWS_CAPTURE}
                       if self.windows else {"install-agent-work-kit-from-package.command": MAC_PACKAGE})
        members = {**members}
        members.setdefault("release-info.json", json.dumps({"tag": TAG, "host": "codex",
                                                           "target": "x86_64-pc-windows-msvc" if self.windows else "aarch64-apple-darwin"}).encode("ascii"))
        write_assets(self.assets, self.archive, members, 0o644 if self.windows else 0o755)

    def run_downloader(self, *options, environment=None):
        if options not in (("--help",), ("-h",)) and "--host" not in options:
            options = ("--host", "codex", *options)
        command = (["cmd.exe", "/D", "/C", str(self.downloader), *options] if self.windows else
                   ["/bin/bash", str(self.downloader), *options])
        result = subprocess.run(command, env=environment if environment is not None else self.environment,
                                stdin=subprocess.DEVNULL, capture_output=True,
                                timeout=35 if self.windows else 30)
        scratch_pattern = "work-release-*" if self.windows else "work-release.*"
        self.assertEqual([], list(self.scratch.glob(scratch_pattern)), result.stderr.decode(errors="replace"))
        return result

    def test_success_forwarding_and_downstream_status(self):
        self.package()
        skills = str(self.root / "中文 path & symbols")
        global_dir = str(self.root / "全域 path & symbols")
        result = self.run_downloader("--skills-dir", skills,
                                     "--instructions-dir", global_dir)
        self.assertEqual(0, result.returncode, (result.stdout, result.stderr))
        self.assertEqual(["--host", "codex", "--skills-dir", skills, "--instructions-dir", global_dir],
                         (self.root / "argv.txt").read_text(encoding="utf-8").splitlines())
        failed = {**self.environment, "WORK_RELEASE_TEST_EXIT": "17"}
        self.assertEqual(17, self.run_downloader(environment=failed).returncode)
        self.assertEqual(17, self.run_downloader("--unknown", environment=failed).returncode)
        if not self.windows:
            self.assertEqual(130, self.run_downloader(
                environment={**self.environment, "WORK_RELEASE_TEST_EXIT": "130"}).returncode)

    def test_help_does_not_download(self):
        self.assertEqual(0, self.run_downloader("--help").returncode)
        self.assertFalse((self.root / "argv.txt").exists())

    def test_missing_and_tampered_assets(self):
        self.assertNotEqual(0, self.run_downloader().returncode)
        self.assertEqual(1, self.server.requests)
        self.assertFalse((self.root / "argv.txt").exists())
        self.package()
        (self.assets / self.archive).unlink()
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
        (self.assets / self.archive).write_bytes(b"tampered")
        self.assertNotEqual(0, self.run_downloader().returncode)
        self.package()
        (self.assets / self.archive).write_bytes(b"not a ZIP")
        write_checksum(self.assets, self.archive)
        self.assertNotEqual(0, self.run_downloader().returncode)

    def test_other_release_tag_is_not_used(self):
        other = self.root / "v999.0.0"
        other.mkdir()
        write_assets(other, self.archive, {self.entry: WINDOWS_PACKAGE if self.windows else b"exit 0\n"},
                     0o644 if self.windows else 0o755)
        self.assertNotEqual(0, self.run_downloader().returncode)
        self.assertEqual(1, self.server.requests)
        self.assertFalse((self.root / "argv.txt").exists())

    def test_package_release_tag_mismatch(self):
        self.package({self.entry: WINDOWS_PACKAGE if self.windows else b"exit 0\n",
                      "release-info.json": b'{"tag":"v999.0.0"}'})
        self.assertNotEqual(0, self.run_downloader().returncode)
        self.assertFalse((self.root / "argv.txt").exists())

    def test_interrupted_transfer(self):
        self.package()
        self.server.interrupt_archive = True
        self.assertNotEqual(0, self.run_downloader().returncode)
        self.assertEqual(2, self.server.requests)

    def test_abandoned_owned_scratch_is_removed(self):
        stale = self.scratch / ("work-release-123456789" if self.windows else "work-release.ABCDEFGH")
        stale.mkdir()
        (stale / ".work-release-owner").write_text(
            "work-release-v1|99999999" if self.windows else "work-release-v1 99999999\n", encoding="ascii")
        (stale / "SHA256SUMS.txt").write_text("partial", encoding="ascii")
        self.package()
        self.assertEqual(0, self.run_downloader().returncode)
        self.assertFalse(stale.exists())

    def test_unsafe_zip_member(self):
        self.package({"../outside": b"escape"})
        self.assertNotEqual(0, self.run_downloader().returncode)
        self.assertFalse((self.root / "outside").exists())
        with zipfile.ZipFile(self.assets / self.archive, "w") as archive:
            archive.writestr("release-info.json", json.dumps({"tag": TAG}))
            info = zipfile.ZipInfo(self.entry)
            info.create_system = 3
            info.external_attr = (stat.S_IFLNK | 0o777) << 16
            archive.writestr(info, "outside")
        write_checksum(self.assets, self.archive)
        self.assertNotEqual(0, self.run_downloader().returncode)

    @unittest.skipUnless(sys.platform == "darwin", "macOS architecture check")
    def test_wrong_platform_is_rejected_before_download(self):
        self.uname.write_text('#!/bin/sh\ncase "$1" in -s) echo Darwin ;; -m) echo x86_64 ;; esac\n',
                              encoding="ascii")
        result = self.run_downloader()
        self.assertNotEqual(0, result.returncode)
        self.assertIn(b"requires macOS ARM64", result.stderr)
        self.assertFalse((self.root / "argv.txt").exists())


if __name__ == "__main__":
    if sys.platform not in ("darwin", "win32"):
        raise SystemExit("native macOS or Windows runner required")
    unittest.main()
