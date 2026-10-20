"""Tests for release publication gates."""

import hashlib
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "release"))
from release_publication_checks import check_assets, check_first_publish_attempt, check_identity, check_immutable, expected_assets, release_notes


class ReleaseGateTests(unittest.TestCase):
    def test_published_identity_does_not_depend_on_newer_main(self):
        sha = "a" * 40
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "rust").mkdir()
            (root / "rust/Cargo.toml").write_text("[workspace]\n", encoding="utf-8")
            def api(path, *_):
                if "/git/ref/tags/" in path:
                    return {"object": {"sha": sha}}
                return {"commit": {"sha": "b" * 40}}
            with patch("version_guard.validate_lock", return_value="1.0.4"), \
                 patch("release_publication_checks.subprocess.run", return_value=subprocess.CompletedProcess([], 0, stdout=sha)), \
                 patch("release_publication_checks.api", side_effect=api):
                self.assertEqual(check_identity(root, "owner/repo", "v1.0.4", sha, require_latest_main=False), "1.0.4")
                with self.assertRaisesRegex(ValueError, "latest main"):
                    check_identity(root, "owner/repo", "v1.0.4", sha)

    def test_forward_only_blocks_reruns_redispatch_and_drafts(self):
        sha = "a" * 40
        run = {"id": 42, "head_sha": sha}
        with patch("release_publication_checks.api", side_effect=lambda path, *_: [{"workflow_runs": [run]}] if "/runs?" in path else [[]]):
            check_first_publish_attempt("owner/repo", "v1.2.3", sha, "42", "1")
        with self.assertRaisesRegex(ValueError, "rerun"):
            check_first_publish_attempt("owner/repo", "v1.2.3", sha, "42", "2")
        with patch("release_publication_checks.api", return_value=[{"workflow_runs": [run, dict(run, id=41)]}]):
            with self.assertRaisesRegex(ValueError, "another dispatch"):
                check_first_publish_attempt("owner/repo", "v1.2.3", sha, "42", "1")
        for draft in (True, False):
            with self.subTest(draft=draft), patch("release_publication_checks.api", side_effect=[
                [{"workflow_runs": [run]}], [[{"tag_name": "v1.2.3", "draft": draft}]],
            ]):
                with self.assertRaisesRegex(ValueError, "draft or published"):
                    check_first_publish_attempt("owner/repo", "v1.2.3", sha, "42", "1")

    def test_failed_publish_stages_cannot_be_dispatched_again(self):
        stages = ("identity", "macos_e2e", "windows_e2e", "attestation", "draft", "publish", "post_publish")
        sha = "a" * 40
        for stage in stages:
            with self.subTest(stage=stage):
                previous = {"id": 41, "head_sha": sha, "conclusion": "failure", "failed_stage": stage}
                current = {"id": 42, "head_sha": sha, "status": "in_progress"}
                def api(path, *_):
                    return [{"workflow_runs": [previous, current]}] if "/runs?" in path else [[]]
                with patch("release_publication_checks.api", side_effect=api):
                    with self.assertRaisesRegex(ValueError, "another dispatch"):
                        check_first_publish_attempt("owner/repo", "v1.2.3", sha, "42", "1")

    def test_failed_version_is_not_reused_as_next_version(self):
        old_sha, new_sha = "a" * 40, "b" * 40
        old_run = {"id": 41, "head_sha": old_sha, "conclusion": "failure"}
        new_run = {"id": 42, "head_sha": new_sha, "status": "in_progress"}
        def api(path, *_):
            if "/runs?" in path:
                return [{"workflow_runs": [old_run if "head_sha=" + old_sha in path else new_run]}]
            return [[]]
        with patch("release_publication_checks.api", side_effect=api):
            with self.assertRaisesRegex(ValueError, "another dispatch"):
                check_first_publish_attempt("owner/repo", "v1.2.3", old_sha, "42", "1")
            check_first_publish_attempt("owner/repo", "v1.2.4", new_sha, "42", "1")

    def test_release_notes_include_failed_versions_and_ignore_drafts(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            subprocess.run(["git", "init", "-q", str(root)], check=True)
            def git(*args):
                return subprocess.run(["git", *args], cwd=root, check=True, capture_output=True, text=True).stdout.strip()
            git("config", "user.name", "Release Test")
            git("config", "user.email", "release-test@example.invalid")
            commits = []
            for version in ("first", "published", "failed-one", "failed-two", "current"):
                (root / "history.txt").write_text(version, encoding="utf-8")
                git("add", "history.txt")
                git("commit", "-qm", version)
                git("tag", f"v-{version}")
                commits.append(git("rev-parse", "HEAD"))
            releases = [
                {"tag_name": "v-published", "draft": False, "prerelease": False, "published_at": "2026-01-01T00:00:00Z"},
                {"tag_name": "v-failed-two", "draft": True, "prerelease": False, "published_at": None},
            ]
            with patch("release_publication_checks.api", return_value=[releases]):
                baseline, label, included, notes = release_notes(root, "owner/repo", "v-current", commits[-1])
            self.assertEqual(baseline, "v-published")
            self.assertIn(commits[1], label)
            self.assertEqual(len(included), 3)
            self.assertIn("failed-one", notes)
            self.assertIn("failed-two", notes)
            with patch("release_publication_checks.api", return_value=[[]]):
                baseline, label, included, notes = release_notes(root, "owner/repo", "v-current", commits[-1])
            self.assertIsNone(baseline)
            self.assertEqual(label, "initial repository history")
            self.assertEqual(len(included), 5)
            self.assertIn("first", notes)
            with patch("release_publication_checks.api", return_value=[releases]):
                with self.assertRaisesRegex(ValueError, "tag does not match"):
                    release_notes(root, "owner/repo", "v-failed-one", commits[-1])

    def test_immutable_must_be_explicitly_enabled(self):
        with patch("release_publication_checks.api", return_value={"enabled": True}):
            check_immutable("owner/repo")
        with patch("release_publication_checks.api", return_value={"enabled": False}):
            with self.assertRaises(ValueError):
                check_immutable("owner/repo")

    def test_draft_requires_exact_three_assets_and_digests(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            assets = []
            for name in sorted(expected_assets("1.0.0")):
                data = name.encode("ascii")
                (root / name).write_bytes(data)
                assets.append({"name": name, "size": len(data), "digest": "sha256:" + hashlib.sha256(data).hexdigest()})
            release = {"tag_name": "v1.0.0", "draft": True, "prerelease": False, "assets": assets}
            with patch("release_publication_checks.api", return_value=[[release]]) as mocked_api:
                check_assets("owner/repo", "v1.0.0", root, published=False)
                mocked_api.assert_called_with("repos/owner/repo/releases?per_page=100", "--paginate", "--slurp")
                assets[0]["digest"] = "sha256:" + "0" * 64
                with self.assertRaises(ValueError):
                    check_assets("owner/repo", "v1.0.0", root, published=False)
            with patch("release_publication_checks.api", return_value=release) as mocked_api:
                with self.assertRaises(ValueError):
                    check_assets("owner/repo", "v1.0.0", root, published=True)
                mocked_api.assert_called_with("repos/owner/repo/releases/tags/v1.0.0")

    def test_draft_requires_one_matching_tag_across_pages(self):
        other = {"tag_name": "v0.9.0"}
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            assets = []
            for name in sorted(expected_assets("1.0.0")):
                data = name.encode("ascii")
                (root / name).write_bytes(data)
                assets.append({"name": name, "size": len(data)})
            target = {"tag_name": "v1.0.0", "draft": True, "prerelease": False, "assets": assets}
            with patch("release_publication_checks.api", return_value=[[other], [target]]) as mocked_api:
                check_assets("owner/repo", "v1.0.0", root, published=False)
                mocked_api.assert_called_with("repos/owner/repo/releases?per_page=100", "--paginate", "--slurp")
            with patch("release_publication_checks.api", return_value=[[target, target]]):
                with self.assertRaisesRegex(ValueError, "exactly one draft release"):
                    check_assets("owner/repo", "v1.0.0", root, published=False)
        with patch("release_publication_checks.api", return_value=[[other]]):
            with self.assertRaisesRegex(ValueError, "exactly one draft release"):
                check_assets("owner/repo", "v1.0.0", Path("missing"), published=False)


if __name__ == "__main__":
    unittest.main()
