"""Tests for the workspace Rust toolchain selector."""

import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "release"))
from rust_toolchain import rust_toolchain


class RustToolchainTests(unittest.TestCase):
    def test_uses_rust_version_instead_of_package_version(self):
        with tempfile.TemporaryDirectory() as directory:
            manifest = Path(directory) / "rust/Cargo.toml"
            manifest.parent.mkdir()
            manifest.write_text('[workspace.package]\nversion = "1.0.0"\nrust-version = "1.99.1"\n', encoding="utf-8")
            self.assertEqual(rust_toolchain(repository=directory), "1.99.1")
            self.assertEqual(rust_toolchain("aarch64-apple-darwin", directory),
                             "1.99.1-aarch64-apple-darwin")
            with self.assertRaisesRegex(ValueError, "invalid Rust target"):
                rust_toolchain("bad\nRUSTUP_TOOLCHAIN=other", directory)

    def test_rejects_non_exact_rust_version(self):
        with tempfile.TemporaryDirectory() as directory:
            manifest = Path(directory) / "rust/Cargo.toml"
            manifest.parent.mkdir()
            manifest.write_text('[workspace.package]\nrust-version = "1.99"\n', encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "exact stable Rust release"):
                rust_toolchain(repository=directory)


if __name__ == "__main__":
    unittest.main()
