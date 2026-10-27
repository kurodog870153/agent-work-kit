"""Regression tests for the pull request version guard."""

import importlib.util
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock, patch


SPEC = importlib.util.spec_from_file_location("version_guard", Path(__file__).resolve().parents[2] / "version-guard/version_guard.py")
guard = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(guard)


class VersionGuardTests(unittest.TestCase):
    def test_baseline_uses_project_version(self):
        with patch.object(guard.subprocess, "run", return_value=Mock(returncode=0, stdout="1.0.1\n")) as run:
            self.assertEqual(guard.baseline_version(Path("."), "main"), "1.0.1\n")
        run.assert_called_once()
        self.assertEqual(run.call_args.args[0], ["git", "show", "main:VERSION"])

    def test_baseline_migrates_from_cargo(self):
        old = Mock(returncode=0, stdout='[workspace.package]\nversion = "1.0.0"\n')
        for message in (
            "fatal: path 'VERSION' does not exist in 'main'",
            "fatal: path 'VERSION' exists on disk, but not in 'main'",
        ):
            with self.subTest(message=message):
                missing = Mock(returncode=128, stderr=message)
                with patch.object(guard.subprocess, "run", side_effect=[missing, old]) as run:
                    self.assertEqual(guard.baseline_version(Path("."), "main"), "1.0.0")
                self.assertEqual(run.call_args_list[1].args[0], ["git", "show", "main:rust/Cargo.toml"])

    def test_semver_ordering(self):
        for newer, older in [
            ("0.10.0", "0.9.9"),
            ("1.0.0", "1.0.0-rc.9"),
            ("1.0.0-rc.10", "1.0.0-rc.9"),
            ("2.0.0", "1.999.999"),
        ]:
            self.assertGreater(guard.compare_versions(newer, older), 0)
            self.assertLess(guard.compare_versions(older, newer), 0)

    def test_invalid_semver(self):
        for version in ["1.0", "01.0.0", "1.0.0-01", "1.0.0+", "v1.0.0"]:
            with self.subTest(version=version), self.assertRaises(ValueError):
                guard.parse_version(version)

    def test_workspace_and_lock(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            rust = root / "rust"
            (rust / "crates" / "sample").mkdir(parents=True)
            baseline = "0.1.0\n"
            (root / "VERSION").write_text("0.2.0\n", encoding="utf-8")
            (rust / "crates" / "sample" / "Cargo.toml").write_text(
                '[package]\nname = "sample"\nversion.workspace = true\n', encoding="utf-8"
            )
            (rust / "Cargo.toml").write_text(
                '[workspace]\nmembers = ["crates/sample"]\n[workspace.package]\nversion = "0.1.0"\n',
                encoding="utf-8",
            )
            lock = rust / "Cargo.lock"
            lock.write_text('[[package]]\nname = "sample"\nversion = "0.1.0"\n', encoding="utf-8")
            self.assertEqual(guard.validate_workspace(root, baseline), "0.2.0")
            for version in ["0.1.0", "0.0.9", "invalid"]:
                (root / "VERSION").write_text(f"{version}\n", encoding="utf-8")
                with self.subTest(version=version), self.assertRaises(ValueError):
                    guard.validate_workspace(root, baseline)
            (root / "VERSION").write_text("0.2.0\n", encoding="utf-8")
            (rust / "Cargo.toml").write_text(
                '[workspace]\nmembers = ["crates/sample"]\n[workspace.package]\nversion = "0.2.0"\n',
                encoding="utf-8",
            )
            with self.assertRaises(ValueError):
                guard.validate_workspace(root, baseline)
            (rust / "Cargo.toml").write_text(
                '[workspace]\nmembers = ["crates/sample"]\n[workspace.package]\nversion = "0.1.0"\n',
                encoding="utf-8",
            )
            lock.write_text('[[package]]\nname = "sample"\nversion = "0.2.0"\n', encoding="utf-8")
            with self.assertRaises(ValueError):
                guard.validate_workspace(root, baseline)

    def test_tooling_inherits_workspace_version(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            rust = root / "rust"
            tooling = rust / "crates/tooling/tooling-cli"
            tooling.mkdir(parents=True)
            (root / "VERSION").write_text("2.0.0\n", encoding="utf-8")
            (rust / "Cargo.toml").write_text(
                '[workspace]\nmembers = ["crates/tooling/tooling-cli"]\n'
                '[workspace.package]\nversion = "2.0.0"\n', encoding="utf-8"
            )
            manifest = '[package]\nname = "tooling-cli"\nversion.workspace = true\n'
            (tooling / "Cargo.toml").write_text(manifest, encoding="utf-8")
            (rust / "Cargo.lock").write_text(
                '[[package]]\nname = "tooling-cli"\nversion = "2.0.0"\n', encoding="utf-8"
            )
            self.assertEqual(guard.validate_lock(root, guard.tomllib.loads((rust / "Cargo.toml").read_text())), "2.0.0")
            (tooling / "Cargo.toml").write_text(manifest.replace("version.workspace = true", 'version = "2.0.0"'), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "must inherit workspace"):
                guard.validate_lock(root, guard.tomllib.loads((rust / "Cargo.toml").read_text()))
            (tooling / "Cargo.toml").write_text(manifest, encoding="utf-8")
            (rust / "Cargo.lock").write_text(
                '[[package]]\nname = "tooling-cli"\nversion = "1.0.0"\n', encoding="utf-8"
            )
            with self.assertRaisesRegex(ValueError, "Cargo.lock workspace versions"):
                guard.validate_lock(root, guard.tomllib.loads((rust / "Cargo.toml").read_text()))

    def test_glob_members_validate_every_package_and_reject_empty_pattern(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            rust = root / "rust"
            (root / "VERSION").write_text("2.0.0\n", encoding="utf-8")
            for name in ("skill-api", "skill-model"):
                crate = rust / "crates/skill" / name
                crate.mkdir(parents=True)
                (crate / "Cargo.toml").write_text(
                    f'[package]\nname = "{name}"\nversion.workspace = true\n', encoding="utf-8"
                )
            manifest_path = rust / "Cargo.toml"
            manifest_path.write_text(
                '[workspace]\nmembers = ["crates/skill/*"]\n'
                '[workspace.package]\nversion = "2.0.0"\n', encoding="utf-8"
            )
            (rust / "Cargo.lock").write_text(
                '[[package]]\nname = "skill-api"\nversion = "2.0.0"\n'
                '[[package]]\nname = "skill-model"\nversion = "2.0.0"\n', encoding="utf-8"
            )
            manifest = guard.tomllib.loads(manifest_path.read_text())
            self.assertEqual(guard.validate_lock(root, manifest), "2.0.0")
            (rust / "crates/skill/skill-model/Cargo.toml").write_text(
                '[package]\nname = "skill-model"\nversion = "2.0.0"\n', encoding="utf-8"
            )
            with self.assertRaisesRegex(ValueError, "must inherit workspace"):
                guard.validate_lock(root, manifest)
            manifest["workspace"]["members"] = ["crates/missing/*"]
            with self.assertRaisesRegex(ValueError, "matches no directories"):
                guard.validate_lock(root, manifest)


if __name__ == "__main__":
    unittest.main()
