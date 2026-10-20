"""Tests for release package metadata."""

import json
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "release"))
from release_package import PLATFORMS, build_package


class ReleasePackageTests(unittest.TestCase):
    def test_records_actual_tested_environment(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            repository = root / "repository"
            output = root / "output"
            repository.mkdir()
            sources = ["AGENTS.md", "LICENSE", "docs/release/README.md"]
            config = PLATFORMS["macos-arm64"]
            sources.extend((config["installer"], *config["modules"]))
            for name in sources:
                path = repository / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(name, encoding="utf-8")
            binary = root / "work"
            binary.write_bytes(b"binary")
            environment = {
                "runner_label": "macos-latest",
                "runner_os": "macOS",
                "cpu_architecture": "ARM64",
                "runner_image_name": "macos-15-arm64",
                "runner_image_version": "20261001.1",
                "os_name": "macOS",
                "os_version": "15.7",
                "os_build": "24G207",
            }
            with patch("release_package.checked_version", return_value="1.2.3"), patch(
                "release_package.tracked_skill_files", return_value=[]
            ):
                archive_path = build_package(repository, output, "macos-arm64", binary, "a" * 40, environment)
            with zipfile.ZipFile(archive_path) as archive:
                info = json.loads(archive.read("release-info.json"))
            self.assertEqual(info["version"], "1.2.3")
            self.assertEqual(info["tested_environment"], environment)
            self.assertNotIn("e2e_runner", info)
            self.assertNotIn("minimum_os", info)

            for field in environment:
                incomplete = environment.copy()
                incomplete[field] = ""
                with self.subTest(field=field), self.assertRaises(ValueError):
                    build_package(repository, output, "macos-arm64", binary, "a" * 40, incomplete)
            wrong_label = environment.copy()
            wrong_label["runner_label"] = "windows-latest"
            with self.assertRaises(ValueError):
                build_package(repository, output, "macos-arm64", binary, "a" * 40, wrong_label)


if __name__ == "__main__":
    unittest.main()
