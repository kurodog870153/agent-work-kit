"""Adversarial fixtures for the release integrity checks."""

import importlib.util
import os
import subprocess
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path


SPEC = importlib.util.spec_from_file_location("release_integrity", Path(__file__).with_name("release_integrity.py"))
integrity = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(integrity)


class ReleaseIntegrityTests(unittest.TestCase):
    def test_tree_round_trip_and_mutations(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "skills").mkdir()
            (root / "skills/work.txt").write_bytes(b"work")
            integrity.create_manifest(root)
            self.assertEqual(set(integrity.verify_tree(root)), {"skills/work.txt"})
            extra = root / "extra"
            extra.write_bytes(b"extra")
            with self.assertRaises(ValueError):
                integrity.verify_tree(root)
            extra.unlink()
            (root / "skills/work.txt").write_bytes(b"changed")
            with self.assertRaises(ValueError):
                integrity.verify_tree(root)
            (root / "skills/work.txt").unlink()
            with self.assertRaises(ValueError):
                integrity.verify_tree(root)

    def test_bad_manifest_entries(self):
        digest = "0" * 64
        for path in ["../outside", "/absolute", "a/../b", "a\\b", "a//b", "./a"]:
            with self.subTest(path=path), self.assertRaises(ValueError):
                integrity.read_manifest(f"{digest}  {path}\n".encode())
        with self.assertRaises(ValueError):
            integrity.read_manifest(f"{digest}  a\n{digest}  a\n".encode())
        with self.assertRaises(ValueError):
            integrity.read_manifest(f"{digest}  a\r\n".encode())

    def test_zip_slip_and_duplicate_member(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "data").write_bytes(b"okay")
            integrity.create_manifest(root)
            archive_path = root / "release.zip"
            with zipfile.ZipFile(archive_path, "w") as archive:
                archive.write(root / "data", "data")
                archive.write(root / "SHA256SUMS", "SHA256SUMS")
            self.assertEqual(set(integrity.verify_zip(archive_path)), {"data"})
            with zipfile.ZipFile(archive_path, "a") as archive:
                archive.writestr("../outside", "bad")
            with self.assertRaises(ValueError):
                integrity.verify_zip(archive_path)
            with zipfile.ZipFile(archive_path, "w") as archive:
                archive.write(root / "data", "data")
                archive.write(root / "SHA256SUMS", "SHA256SUMS")
                archive.writestr("data", "duplicate")
            with self.assertRaises(ValueError):
                integrity.verify_zip(archive_path)

    @unittest.skipUnless(sys.platform == "win32", "Windows PowerShell verifier")
    def test_windows_native_verifier(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            item = root / "work.txt"
            item.write_bytes(b"work")
            integrity.create_manifest(root)
            script = Path(__file__).parents[3] / "os-scripts/windows/installer/verify-package.ps1"
            command = ["powershell.exe", "-NoProfile", "-File", str(script), "-PackageRoot", str(root)]
            environment = os.environ.copy()
            system_root = Path(os.environ.get("SystemRoot", r"C:\Windows"))
            environment["PATH"] = os.pathsep.join((str(system_root / "System32"), str(system_root / "System32/WindowsPowerShell/v1.0")))
            result = subprocess.run(command, capture_output=True, text=True, env=environment)
            self.assertEqual(result.returncode, 0, result.stderr)
            item.write_bytes(b"tampered")
            result = subprocess.run(command, capture_output=True, text=True, env=environment)
            self.assertNotEqual(result.returncode, 0)


if __name__ == "__main__":
    unittest.main()
