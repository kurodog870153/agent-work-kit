"""Tests for release package metadata."""

import json
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "release"))
from release_package import DOWNLOADERS, PLATFORMS, build_downloaders, build_package, checked_workspace_version, tracked_skill_files


class ReleasePackageTests(unittest.TestCase):
    def test_both_skills_and_new_instruction_modules_are_packaged(self):
        repository = Path(__file__).resolve().parents[4]
        paths = tracked_skill_files(repository)
        self.assertIn(Path("skills/work/SKILL.md"), paths)
        self.assertIn(Path("skills/work/references/instruction-loading/tool-policy-integration.md"), paths)
        self.assertIn(Path("skills/tooling/SKILL.md"), paths)
        self.assertIn(Path("skills/tooling/references/instruction-loading.md"), paths)
        self.assertNotIn(Path("skills/tooling/references/instructions/interfaces/repl/instructions.md"), paths)
        self.assertFalse(any(path.name == ".DS_Store" for path in paths))
        self.assertNotIn(Path("skills/work/references/instruction-loading/handoff.md"), paths)

        with patch("release_package.subprocess.run", side_effect=[SimpleNamespace(stdout=b"skills/work/SKILL.md\0"), SimpleNamespace(stdout=b"")]):
            with self.assertRaisesRegex(ValueError, "Tooling skill entry is missing"):
                tracked_skill_files(repository)

    def test_standalone_downloaders_are_bound_to_the_release_tag(self):
        repository = Path(__file__).resolve().parents[4]
        version = (repository / "VERSION").read_text(encoding="ascii").strip()
        major, minor, patch_number = map(int, version.split("."))
        next_version = f"{major}.{minor}.{patch_number + 1}"
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            names = {path.name for path in build_downloaders(repository, output)}
            self.assertEqual(names, {Path(path).name for path in DOWNLOADERS.values()})
            for relative in DOWNLOADERS.values():
                source = (repository / relative).read_bytes()
                self.assertIn(b"__WORK_RELEASE_TAG__", source)
                self.assertNotIn(b"WORK_RELEASE_TEST_MODE", source)
                self.assertNotIn(b"WORK_RELEASE_TEST_BASE_URL", source)
                self.assertEqual((output / Path(relative).name).read_bytes(), source.replace(b"__WORK_RELEASE_TAG__", f"v{version}".encode("ascii")))
            with self.assertRaisesRegex(ValueError, "already exists"):
                build_downloaders(repository, output)
            with patch("release_package.checked_version", return_value=next_version):
                with tempfile.TemporaryDirectory() as other:
                    paths = build_downloaders(repository, Path(other))
                    for path in paths:
                        self.assertIn(f"v{next_version}".encode("ascii"), path.read_bytes())
                        self.assertNotIn(b"__WORK_RELEASE_TAG__", path.read_bytes())

    def test_records_actual_tested_environment(self):
        workspace_version = checked_workspace_version(Path(__file__).resolve().parents[4])
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            repository = root / "repository"
            output = root / "output"
            repository.mkdir()
            sources = ["AGENTS.md", "LICENSE", "docs/release/README.md",
                       "os-scripts/common/installer/options.tsv", "os-scripts/common/installer/install-state.json"]
            config = PLATFORMS["macos-arm64"]
            sources.extend((config["installer"], *config["modules"]))
            for name in sources:
                path = repository / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(name, encoding="utf-8")
            binary = root / "work"
            binary.write_bytes(b"binary")
            tooling_binary = root / "tooling"
            tooling_binary.write_bytes(b"tooling binary")
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
            skill_files = [
                "skills/work/SKILL.md", "skills/work/references/instruction-loading.md",
                "skills/tooling/SKILL.md", "skills/tooling/references/instruction-loading.md",
            ]
            for name in skill_files:
                path = repository / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(name, encoding="utf-8")
            with patch("release_package.checked_version", return_value="1.2.3"), patch(
                "release_package.checked_workspace_version", return_value=workspace_version
            ), patch(
                "release_package.tracked_skill_files", return_value=[Path(name) for name in skill_files]
            ), patch("release_package.subprocess.run") as run:
                def compile_helper(command, check):
                    self.assertTrue(check)
                    Path(command[command.index("-o") + 1]).write_bytes(b"native lock helper")
                run.side_effect = compile_helper
                archive_path = build_package(repository, output, "macos-arm64", binary, tooling_binary, "a" * 40, environment)
            with zipfile.ZipFile(archive_path) as archive:
                info = json.loads(archive.read("release-info.json"))
                self.assertEqual(archive.read("bin/work-install-lock"), b"native lock helper")
                self.assertEqual(archive.read("bin/work"), b"binary")
                self.assertEqual(archive.read("bin/tooling"), b"tooling binary")
                for name in skill_files:
                    self.assertEqual(archive.read(name), name.encode())
                self.assertEqual(archive.read("installer/cli/options.tsv"), b"os-scripts/common/installer/options.tsv")
                self.assertEqual(archive.read("installer/state/install-state.json"),
                                 b"os-scripts/common/installer/install-state.json")
                for source in config["modules"]:
                    relative = Path(source).relative_to("os-scripts/mac/installer")
                    self.assertEqual(archive.read(Path("installer", relative).as_posix()), source.encode())
            self.assertEqual(info["version"], "1.2.3")
            self.assertEqual(info["work_version"], workspace_version)
            self.assertEqual(info["tooling_version"], workspace_version)
            self.assertEqual(info["tested_environment"], environment)
            self.assertNotIn("e2e_runner", info)
            self.assertNotIn("minimum_os", info)

            for field in environment:
                incomplete = environment.copy()
                incomplete[field] = ""
                with self.subTest(field=field), self.assertRaises(ValueError):
                    build_package(repository, output, "macos-arm64", binary, tooling_binary, "a" * 40, incomplete)
            wrong_label = environment.copy()
            wrong_label["runner_label"] = "windows-latest"
            with self.assertRaises(ValueError):
                build_package(repository, output, "macos-arm64", binary, tooling_binary, "a" * 40, wrong_label)


if __name__ == "__main__":
    unittest.main()
