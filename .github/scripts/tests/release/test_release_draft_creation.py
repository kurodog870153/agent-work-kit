"""Tests for retrying unpublished release drafts."""

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "release"))
from release_draft_creation import prepare_draft


class DraftCreationTests(unittest.TestCase):
    def test_existing_draft_is_refreshed(self):
        sha = "a" * 40
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ("asset.zip", "installer.command", "installer.bat", "SHA256SUMS.txt"):
                (root / name).write_text(name, encoding="utf-8")
            title, notes = root / "title.txt", root / "notes.md"
            title.write_text("Title\n", encoding="utf-8")
            notes.write_text("Notes\n", encoding="utf-8")
            draft = {"tag_name": "v1", "draft": True, "name": "Title", "body": "Notes\n", "assets": []}
            with patch("release_draft_creation.api", side_effect=[{"object": {"sha": sha}}, [[draft]]]), \
                 patch("release_draft_creation.expected_assets", return_value={"asset.zip", "installer.command", "installer.bat", "SHA256SUMS.txt"}), \
                 patch("release_draft_creation.subprocess.run") as run, patch("release_draft_creation.check_assets") as check:
                prepare_draft("owner/repo", "v1", sha, root, title, notes)
                self.assertEqual([call.args[0][2] for call in run.call_args_list], ["upload"])
                self.assertIn("--clobber", run.call_args_list[0].args[0])
                check.assert_called_once()

    def test_published_release_is_not_changed(self):
        sha = "a" * 40
        with patch("release_draft_creation.api", side_effect=[{"object": {"sha": sha}}, [[{"tag_name": "v1", "draft": False}]]]), \
             patch("release_draft_creation.subprocess.run") as run:
            with self.assertRaisesRegex(ValueError, "published"):
                prepare_draft("owner/repo", "v1", sha, Path("."), Path("title"), Path("notes"))
            run.assert_not_called()

    def test_changed_draft_is_not_overwritten(self):
        sha = "a" * 40
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ("asset.zip", "installer.command", "installer.bat", "SHA256SUMS.txt"):
                (root / name).write_text(name, encoding="utf-8")
            title, notes = root / "title.txt", root / "notes.md"
            title.write_text("Title\n", encoding="utf-8")
            notes.write_text("Notes\n", encoding="utf-8")
            draft = {"tag_name": "v1", "draft": True, "name": "Other", "body": "Notes\n", "assets": []}
            with patch("release_draft_creation.api", side_effect=[{"object": {"sha": sha}}, [[draft]]]), \
                 patch("release_draft_creation.expected_assets", return_value={"asset.zip", "installer.command", "installer.bat", "SHA256SUMS.txt"}), \
                 patch("release_draft_creation.subprocess.run") as run:
                with self.assertRaisesRegex(ValueError, "differ"):
                    prepare_draft("owner/repo", "v1", sha, root, title, notes)
                run.assert_not_called()


if __name__ == "__main__":
    unittest.main()
