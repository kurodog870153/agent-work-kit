"""Unit tests for main release tag creation."""

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "release"))
from release_tag_creation import create_tag, main


class ReleaseTagCreationTests(unittest.TestCase):
    def test_stale_main_commit_blocks_tag(self):
        sha = "a" * 40
        with patch("release_tag_creation.api", return_value={"commit": {"sha": "b" * 40}}), \
             patch("release_tag_creation.subprocess.run") as run:
            with self.assertRaisesRegex(ValueError, "main changed before tag creation"):
                create_tag(Path("."), "owner/repo", sha)
            run.assert_not_called()

    def test_main_change_before_tag_blocks_release(self):
        sha = "b" * 40
        checkout = subprocess.CompletedProcess(["git", "rev-parse", "HEAD"], 0, stdout=sha)
        with patch("release_tag_creation.api", side_effect=[
                 {"commit": {"sha": sha}}, {"commit": {"sha": "c" * 40}}]) as api, \
             patch("release_tag_creation.checked_version", return_value="1.0.4"), \
             patch("release_tag_creation.tag_exists", return_value=False), \
             patch("release_tag_creation.release_exists", return_value=False), \
             patch("release_tag_creation.subprocess.run", return_value=checkout):
            with self.assertRaisesRegex(ValueError, "main changed before tag creation"):
                create_tag(Path("."), "owner/repo", sha)
            self.assertEqual(api.call_count, 2)

    def test_existing_tag_blocks_rerun(self):
        sha = "c" * 40
        checkout = subprocess.CompletedProcess(["git", "rev-parse", "HEAD"], 0, stdout=sha)
        with patch("release_tag_creation.api", return_value={"commit": {"sha": sha}}), \
             patch("release_tag_creation.checked_version", return_value="1.0.1"), \
             patch("release_tag_creation.tag_exists", return_value=True), \
             patch("release_tag_creation.subprocess.run", return_value=checkout):
            with self.assertRaisesRegex(ValueError, "tag already exists"):
                create_tag(Path("."), "owner/repo", sha)

    def test_tag_creation_outputs_version_tag(self):
        sha = "e" * 40
        checkout = subprocess.CompletedProcess(["git", "rev-parse", "HEAD"], 0, stdout=sha)
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "github-output"
            arguments = ["release_tag_creation.py", "--repository", "owner/repo", "--expected-sha", sha]
            with patch("sys.argv", arguments), patch.dict("os.environ", {"GITHUB_OUTPUT": str(output)}), \
                 patch("release_tag_creation.api", side_effect=[
                     {"commit": {"sha": sha}}, {"commit": {"sha": sha}}, {}]) as api, \
                 patch("release_tag_creation.checked_version", return_value="1.0.4"), \
                 patch("release_tag_creation.tag_exists", return_value=False), \
                 patch("release_tag_creation.release_exists", return_value=False), \
                 patch("release_tag_creation.subprocess.run", return_value=checkout):
                self.assertEqual(main(), 0)
                api.assert_any_call("repos/owner/repo/git/refs", "--method", "POST", "-f", "ref=refs/tags/v1.0.4", "-f", f"sha={sha}")
                self.assertEqual(output.read_text(encoding="utf-8"), "tag=v1.0.4\n")

    def test_tag_creation_failure_never_outputs_tag(self):
        sha = "f" * 40
        checkout = subprocess.CompletedProcess(["git", "rev-parse", "HEAD"], 0, stdout=sha)
        failed_tag = subprocess.CalledProcessError(1, ["gh", "api", "--method", "POST"])
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "github-output"
            arguments = ["release_tag_creation.py", "--repository", "owner/repo", "--expected-sha", sha]
            with patch("sys.argv", arguments), patch.dict("os.environ", {"GITHUB_OUTPUT": str(output)}), \
                 patch("release_tag_creation.api", side_effect=[
                     {"commit": {"sha": sha}}, {"commit": {"sha": sha}}, failed_tag]), \
                 patch("release_tag_creation.checked_version", return_value="1.0.4"), \
                 patch("release_tag_creation.tag_exists", return_value=False), \
                 patch("release_tag_creation.release_exists", return_value=False), \
                 patch("release_tag_creation.subprocess.run", return_value=checkout):
                self.assertEqual(main(), 1)
                self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
