"""Native macOS loopback fixtures for the standalone Release downloader."""

import hashlib
import json
import os
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
ARCHIVE = f"work-{TAG}-macos-arm64.zip"


class DownloaderTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="work-downloader-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.downloader = self.root / "downloaders/install-work-from-github-release.command"
        build_test_downloaders(ROOT, self.downloader.parent)
        self.assets = self.root / TAG
        self.assets.mkdir()
        self.shims = self.root / "shims"
        self.shims.mkdir()
        uname = self.shims / "uname"
        uname.write_text(
            '#!/bin/sh\ncase "$1" in -s) echo Darwin ;; -m) echo arm64 ;; *) /usr/bin/uname "$@" ;; esac\n',
            encoding="ascii",
        )
        uname.chmod(0o755)
        self.scratch = self.root / "scratch"
        self.scratch.mkdir()
        self.server = start_server(self.root, ARCHIVE, self.addCleanup)
        self.environment = os.environ.copy()
        self.environment.update({
            "PATH": f"{self.shims}:/usr/bin:/bin",
            "TMPDIR": str(self.scratch),
            "WORK_RELEASE_TEST_MODE": "1",
            "WORK_RELEASE_TEST_BASE_URL": f"http://127.0.0.1:{self.server.server_port}",
            "WORK_RELEASE_TEST_ARGV": str(self.root / "argv.txt"),
        })

    def package(self, members=None):
        members = members or {"install-work-from-package.command": b'#!/bin/bash\nprintf "%s\\n" "$@" > "$WORK_RELEASE_TEST_ARGV"\nexit "${WORK_RELEASE_TEST_EXIT:-0}"\n'}
        members = {**members}
        members.setdefault("release-info.json", json.dumps({"tag": TAG}).encode("ascii"))
        write_assets(self.assets, ARCHIVE, members, 0o755)

    def run_downloader(self, *options, environment=None):
        result = subprocess.run(
            ["/bin/bash", str(self.downloader), *options],
            env=environment or self.environment,
            stdin=subprocess.DEVNULL,
            capture_output=True,
            timeout=30,
        )
        self.assertEqual([], list(self.scratch.glob("work-release.*")), result.stderr.decode(errors="replace"))
        return result

    def test_success_forwarding_and_downstream_status(self):
        self.package()
        work = str(self.root / "中文 path & symbols/work")
        global_dir = str(self.root / "全域 path & symbols")
        result = self.run_downloader("--skills", "java,spring-boot", "--work-install-dir", work,
                                     "--global-agents-install-dir", global_dir)
        self.assertEqual(0, result.returncode, result.stderr)
        self.assertEqual(
            ["--skills", "java,spring-boot", "--work-install-dir", work,
             "--global-agents-install-dir", global_dir],
            (self.root / "argv.txt").read_text(encoding="utf-8").splitlines(),
        )
        env = {**self.environment, "WORK_RELEASE_TEST_EXIT": "17"}
        self.assertEqual(17, self.run_downloader(environment=env).returncode)
        env["WORK_RELEASE_TEST_EXIT"] = "130"
        self.assertEqual(130, self.run_downloader(environment=env).returncode)
        self.assertEqual(17, self.run_downloader("--unknown", environment={**self.environment,
                         "WORK_RELEASE_TEST_EXIT": "17"}).returncode)

    def test_wrong_platform_is_rejected_before_download(self):
        uname = self.shims / "uname"
        uname.write_text('#!/bin/sh\ncase "$1" in -s) echo Darwin ;; -m) echo x86_64 ;; esac\n', encoding="ascii")
        result = self.run_downloader()
        self.assertNotEqual(0, result.returncode)
        self.assertIn(b"requires macOS ARM64", result.stderr)
        self.assertFalse((self.root / "argv.txt").exists())

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
        write_assets(other, ARCHIVE, {"install-work-from-package.command": b"exit 0\n"}, 0o755)
        self.assertNotEqual(0, self.run_downloader().returncode)
        self.assertEqual(1, self.server.requests)
        self.assertFalse((self.root / "argv.txt").exists())

    def test_package_release_tag_mismatch(self):
        self.package({"install-work-from-package.command": b"exit 0\n",
                      "release-info.json": b'{"tag":"v999.0.0"}'})
        self.assertNotEqual(0, self.run_downloader().returncode)
        self.assertFalse((self.root / "argv.txt").exists())

    def test_unsafe_zip_member(self):
        self.package({"../outside": b"escape"})
        self.assertNotEqual(0, self.run_downloader().returncode)
        self.assertFalse((self.root / "outside").exists())
        self.package({"install-work-from-package.command": b"ok"})
        with zipfile.ZipFile(self.assets / ARCHIVE, "w") as archive:
            info = zipfile.ZipInfo("install-work-from-package.command")
            info.create_system = 3
            info.external_attr = (stat.S_IFLNK | 0o777) << 16
            archive.writestr(info, "outside")
        write_checksum(self.assets, ARCHIVE)
        self.assertNotEqual(0, self.run_downloader().returncode)

    def test_interrupted_transfer(self):
        self.package()
        self.server.interrupt_archive = True
        self.assertNotEqual(0, self.run_downloader().returncode)
        self.assertEqual(2, self.server.requests)

    def test_abandoned_owned_scratch_is_removed(self):
        stale = self.scratch / "work-release.ABCDEFGH"
        stale.mkdir()
        (stale / ".work-release-owner").write_text("work-release-v1 99999999\n", encoding="ascii")
        (stale / "SHA256SUMS.txt").write_text("partial", encoding="ascii")
        self.package()
        self.assertEqual(0, self.run_downloader().returncode)
        self.assertFalse(stale.exists())


if __name__ == "__main__":
    if sys.platform != "darwin":
        raise SystemExit("native macOS runner required")
    unittest.main()
