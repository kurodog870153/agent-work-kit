"""Tests for release publication gates."""

import hashlib
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "release"))
from release_publication_checks import check_assets, check_first_publish_attempt, check_identity, check_immutable, expected_assets, generate_release_content, main, published_baseline, release_notes


class ReleaseGateTests(unittest.TestCase):
    def test_github_generated_title_uses_published_baseline(self):
        with patch("release_publication_checks.api", return_value={"name": "Changes", "body": "## What's Changed\n\n- Fix"}) as mocked_api:
            self.assertEqual(generate_release_content("owner/repo", "v1.2.3", "v1.2.2", "a" * 40),
                             "## What's Changed\n\n- Fix")
            mocked_api.assert_called_once_with(
                "repos/owner/repo/releases/generate-notes", "--method", "POST",
                "-f", "tag_name=v1.2.3", "-f", f"target_commitish={'a' * 40}",
                "-f", "previous_tag_name=v1.2.2",
            )
        for name in (None, " ", "two\nlines"):
            with self.subTest(name=name), patch("release_publication_checks.api", return_value={"name": name, "body": "Description"}):
                self.assertEqual(generate_release_content("owner/repo", "v1.2.3", None, "a" * 40), "Description")
        with patch("release_publication_checks.api", return_value={"name": "Changes", "body": " "}):
            with self.assertRaisesRegex(ValueError, "release description"):
                generate_release_content("owner/repo", "v1.2.3", None, "a" * 40)

    def test_notes_action_writes_prepared_content_separately(self):
        with tempfile.TemporaryDirectory() as directory:
            notes_file = Path(directory) / "notes.md"
            title_file = Path(directory) / "title.txt"
            manifest_file = Path(directory) / "manifest.json"
            argv = ["release_publication_checks.py", "notes", "--repository", "owner/repo", "--tag", "v1.2.3",
                    "--expected-sha", "a" * 40, "--notes-file", str(notes_file), "--title-file", str(title_file),
                    "--manifest-file", str(manifest_file)]
            with patch.object(sys, "argv", argv), patch.dict(os.environ, {"GITHUB_STEP_SUMMARY": ""}), \
                 patch("release_publication_checks.prepare_content", return_value={"baseline_label": "v1.2.2", "sources": {"title": "test"}}) as prepared:
                self.assertEqual(main(), 0)
            prepared.assert_called_once()

    def test_published_identity_does_not_depend_on_newer_main(self):
        sha = "a" * 40
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "rust").mkdir()
            (root / "rust/Cargo.toml").write_text("[workspace]\n", encoding="utf-8")
            (root / "VERSION").write_text("1.0.4\n", encoding="utf-8")
            def api(path, *_):
                if "/git/ref/tags/" in path:
                    return {"object": {"sha": sha}}
                return {"commit": {"sha": "b" * 40}}
            with patch("version_guard.validate_lock", return_value="1.0.0") as validate_lock, \
                 patch("release_publication_checks.subprocess.run", return_value=subprocess.CompletedProcess([], 0, stdout=sha)), \
                 patch("release_publication_checks.api", side_effect=api):
                self.assertEqual(check_identity(root, "owner/repo", "v1.0.4", sha, require_latest_main=False), "1.0.4")
                validate_lock.assert_called()
                with self.assertRaisesRegex(ValueError, "tag v1.0.0 does not match project version 1.0.4"):
                    check_identity(root, "owner/repo", "v1.0.0", sha, require_latest_main=False)
                with self.assertRaisesRegex(ValueError, "latest main"):
                    check_identity(root, "owner/repo", "v1.0.4", sha)

    def test_unpublished_rerun_and_draft_are_allowed(self):
        sha = "a" * 40
        run = {"id": 42, "head_sha": sha}
        with patch("release_publication_checks.api", side_effect=lambda path, *_: [{"workflow_runs": [run]}] if "/runs?" in path else [[]]) as api, \
             patch("release_publication_checks.subprocess.run", return_value=subprocess.CompletedProcess([], 1, stderr="HTTP 404")):
            check_first_publish_attempt("owner/repo", "v1.2.3", sha, "42", "1")
        self.assertIn("event=push", api.call_args_list[0].args[0])
        with patch("release_publication_checks.api", side_effect=lambda path, *_: [{"workflow_runs": [run]}] if "/runs?" in path else [[]]), \
             patch("release_publication_checks.subprocess.run", return_value=subprocess.CompletedProcess([], 1, stderr="HTTP 404")):
            check_first_publish_attempt("owner/repo", "v1.2.3", sha, "42", "2")
        with patch("release_publication_checks.api", side_effect=lambda path, *_: [{"workflow_runs": [run]}] if "/runs?" in path else [[{"tag_name": "v1.2.3", "draft": True}]]), \
             patch("release_publication_checks.subprocess.run", return_value=subprocess.CompletedProcess([], 0, stdout=json.dumps({"object": {"sha": sha}}))):
            check_first_publish_attempt("owner/repo", "v1.2.3", sha, "42", "2")
        with patch("release_publication_checks.api", side_effect=lambda path, *_: [{"workflow_runs": [run]}] if "/runs?" in path else [[]]), \
             patch("release_publication_checks.subprocess.run", return_value=subprocess.CompletedProcess([], 0, stdout=json.dumps({"object": {"sha": sha}}))):
            check_first_publish_attempt("owner/repo", "v1.2.3", sha, "42", "1", tag_created=True)
            with self.assertRaisesRegex(ValueError, "already exists"):
                check_first_publish_attempt("owner/repo", "v1.2.3", sha, "42", "1")
        with patch("release_publication_checks.api", return_value=[{"workflow_runs": [run, dict(run, id=41)]}]):
            with self.assertRaisesRegex(ValueError, "another run"):
                check_first_publish_attempt("owner/repo", "v1.2.3", sha, "42", "1")
        for draft in (True, False):
            with self.subTest(draft=draft), patch("release_publication_checks.api", side_effect=[
                [{"workflow_runs": [run]}], [[{"tag_name": "v1.2.3", "draft": draft}]],
            ]), patch("release_publication_checks.subprocess.run", return_value=subprocess.CompletedProcess([], 1, stderr="HTTP 404")):
                with self.assertRaisesRegex(ValueError, "draft from another run|already published"):
                    check_first_publish_attempt("owner/repo", "v1.2.3", sha, "42", "1")

    def test_rerun_rejects_published_release_and_mismatched_tag(self):
        sha = "a" * 40
        run = {"id": 42, "head_sha": sha}
        for release, ref_sha, error in (({"tag_name": "v1.2.3", "draft": False}, sha, "already published"),
                                        ({"tag_name": "v1.2.3", "draft": True}, "b" * 40, "different commit")):
            def api(path, *_):
                return [{"workflow_runs": [run]}] if "/runs?" in path else [[release]]
            with self.subTest(error=error), patch("release_publication_checks.api", side_effect=api), \
                 patch("release_publication_checks.subprocess.run", return_value=subprocess.CompletedProcess([], 0, stdout=json.dumps({"object": {"sha": ref_sha}}))):
                with self.assertRaisesRegex(ValueError, error):
                    check_first_publish_attempt("owner/repo", "v1.2.3", sha, "42", "2")

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
                    with self.assertRaisesRegex(ValueError, "another run"):
                        check_first_publish_attempt("owner/repo", "v1.2.3", sha, "42", "1")

    def test_failed_version_is_not_reused_as_next_version(self):
        old_sha, new_sha = "a" * 40, "b" * 40
        old_run = {"id": 41, "head_sha": old_sha, "conclusion": "failure"}
        new_run = {"id": 42, "head_sha": new_sha, "status": "in_progress"}
        def api(path, *_):
            if "/runs?" in path:
                return [{"workflow_runs": [old_run if "head_sha=" + old_sha in path else new_run]}]
            return [[]]
        with patch("release_publication_checks.api", side_effect=api), \
             patch("release_publication_checks.subprocess.run", return_value=subprocess.CompletedProcess([], 1, stderr="HTTP 404")):
            with self.assertRaisesRegex(ValueError, "another run"):
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
                self.assertEqual(release_notes(root, "owner/repo", "v-failed-one", commits[-1])[0], "v-published")
                self.assertEqual(published_baseline(root, "owner/repo", commits[1])[0], "v-published")

    def test_immutable_must_be_explicitly_enabled(self):
        with patch("release_publication_checks.api", return_value={"enabled": True}):
            check_immutable("owner/repo")
        with patch("release_publication_checks.api", return_value={"enabled": False}):
            with self.assertRaises(ValueError):
                check_immutable("owner/repo")

    def test_draft_requires_exact_seven_assets_and_digests(self):
        self.assertEqual(len(expected_assets("1.0.0")), 7)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            assets = []
            for name in sorted(expected_assets("1.0.0")):
                data = name.encode("ascii")
                (root / name).write_bytes(data)
                assets.append({"name": name, "size": len(data), "digest": "sha256:" + hashlib.sha256(data).hexdigest()})
            release = {"tag_name": "v1.0.0", "name": "v1.0.0: Changes", "body": "Generated description\n\ncustom notes\n", "draft": True, "prerelease": False, "assets": assets}
            with patch("release_publication_checks.api", return_value=[[release]]) as mocked_api:
                check_assets("owner/repo", "v1.0.0", root, published=False, expected_title="v1.0.0: Changes",
                             expected_notes="Generated description\n\ncustom notes\n")
                mocked_api.assert_called_with("repos/owner/repo/releases?per_page=100", "--paginate", "--slurp")
                with self.assertRaisesRegex(ValueError, "release title"):
                    check_assets("owner/repo", "v1.0.0", root, published=False, expected_title="wrong title")
                with self.assertRaisesRegex(ValueError, "release notes"):
                    check_assets("owner/repo", "v1.0.0", root, published=False, expected_notes="wrong notes")
                with self.assertRaisesRegex(ValueError, "release notes"):
                    check_assets("owner/repo", "v1.0.0", root, published=False,
                                 expected_notes="Generated description\n\ncustom notes")
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
