"""Release content source and pre-tag gate tests."""

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "release"))
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "version-guard"))
from release_content import declarations, metadata, resolve, valid_title
from release_publication_checks import assemble_notes, prepare_content, related_pulls, verify_content


class ReleaseContentTests(unittest.TestCase):
    def pull(self, number=3, title="feat: Multi-host packaging", body="A unified package for two hosts."):
        return {"number": number, "title": title, "body": body}

    def test_metadata_requires_both_fields(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            folder = root / ".github/release-metadata"
            folder.mkdir(parents=True)
            path = folder / "v1.2.3.md"
            path.write_text("---\ntitle: Multi-host packaging\n---\n\n## Overview\n\nUnified packaging.\n", encoding="utf-8")
            fields, sources = resolve(root, "v1.2.3", [self.pull()])
            self.assertEqual(fields["title"], "Multi-host packaging")
            self.assertIn("Overview", fields["description"])
            self.assertIn("v1.2.3.md", sources["title"])
            for partial in ("---\ntitle: Multi-host packaging\n---\n", "---\n---\n\n## Overview\n\nUnified packaging.\n"):
                with self.subTest(partial=partial):
                    path.write_text(partial, encoding="utf-8")
                    with self.assertRaisesRegex(ValueError, "requires both Title and Description"):
                        resolve(root, "v1.2.3", [self.pull()])

    def test_pr_can_still_declare_one_field_without_metadata(self):
        pull = self.pull(body="## Release Title\n\nMulti-host packaging")
        fields, sources = resolve(Path("/missing"), "v1.2.3", [pull])
        self.assertEqual(fields["title"], "Multi-host packaging")
        self.assertEqual(fields["description"], "Multi-host packaging")
        self.assertEqual(sources["description"], "PR #3 fallback")

    def test_explicit_pr_declarations_and_conflicts(self):
        body = "## Release Title\n\nMulti-host packaging\n\n## Release Description\n\n## Overview\n\nUnified packaging."
        fields, sources = resolve(Path("/missing"), "v1.2.3", [self.pull(body=body), self.pull(4, body=body)])
        self.assertEqual(fields["title"], "Multi-host packaging")
        self.assertEqual(sources["title"], "PR #3")
        with self.assertRaisesRegex(ValueError, "conflicting"):
            resolve(Path("/missing"), "v1.2.3", [self.pull(body=body), self.pull(4, body=body.replace("Multi-host", "Other"))])
        with self.assertRaisesRegex(ValueError, "duplicate"):
            declarations("## Release Title\nOne\n## Release Title\nTwo")

    def test_ambiguous_and_invalid_sources_fail_closed(self):
        with self.assertRaisesRegex(ValueError, "found 0 PRs"):
            resolve(Path("/missing"), "v1.2.3", [])
        with self.assertRaisesRegex(ValueError, "found 2 PRs"):
            resolve(Path("/missing"), "v1.2.3", [self.pull(), self.pull(4)])
        for title in ("v1.2.3", "v1.2.3: v1.2.3", "v1.2.3 Multi-host", "Merge pull request #3", "Changes", "  "):
            with self.subTest(title=title), self.assertRaises(ValueError):
                valid_title(title)
        with self.assertRaisesRegex(ValueError, "summary prose"):
            declarations("## Release Description\n\n## What's Changed\n\n- Fix")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            folder = root / ".github/release-metadata"
            folder.mkdir(parents=True)
            (folder / "v1.2.3.md").write_text("---\ntitle: \n---\n", encoding="utf-8")
            with self.assertRaises(ValueError):
                metadata(root, "v1.2.3")

    def test_prepared_content_is_pinned_and_contains_sections(self):
        sha = "a" * 40
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            title, notes, manifest = (root / name for name in ("title.txt", "notes.md", "manifest.json"))
            with patch("release_publication_checks.checked_git", return_value=sha), \
                 patch("version_guard.checked_project_version", return_value="1.2.3"), \
                 patch("release_publication_checks.release_notes", return_value=("v1.2.2", "v1.2.2 (sha)", ["one"], "Changes since v1.2.2\n\n- one\n")), \
                 patch("release_publication_checks.related_pulls", return_value=[self.pull()]), \
                 patch("release_publication_checks.generate_release_content", return_value="## What's Changed\n\n- Fix\n\n**Full Changelog**: link"):
                prepare_content(root, "owner/repo", "v1.2.3", sha, title, notes, manifest)
            self.assertEqual(title.read_text(), "Multi-host packaging\n")
            self.assertLess(notes.read_text().index("## Overview"), notes.read_text().index("## What's Changed"))
            self.assertLess(notes.read_text().index("## What's Changed"), notes.read_text().index("## Commit History"))
            self.assertIn("Full Changelog", notes.read_text())
            with patch("release_publication_checks.checked_git", return_value=sha), \
                 patch("release_publication_checks.api", return_value=[[{"tag_name": "v1.2.2", "draft": False, "prerelease": False, "published_at": "2026-01-01"}]]):
                self.assertEqual(verify_content(root, "owner/repo", "v1.2.3", sha, title, notes, manifest)["sha"], sha)
                with patch("release_publication_checks.api", return_value=[[{"tag_name": "v1.2.4", "draft": False, "prerelease": False, "published_at": "2026-02-01"}]]):
                    with self.assertRaisesRegex(ValueError, "baseline changed"):
                        verify_content(root, "owner/repo", "v1.2.3", sha, title, notes, manifest)
                notes.write_text("tampered", encoding="utf-8")
                with self.assertRaisesRegex(ValueError, "changed"):
                    verify_content(root, "owner/repo", "v1.2.3", sha, title, notes, manifest)
                notes.write_text("restored", encoding="utf-8")

    def test_only_merged_main_pulls_in_commit_range_are_related(self):
        sha = "a" * 40
        main = dict(self.pull(), merged_at="2026-01-01", base={"ref": "main"}, merge_commit_sha=sha)
        other = dict(self.pull(4), merged_at="2026-01-01", base={"ref": "feature"}, merge_commit_sha=sha)
        unmerged = dict(self.pull(5), merged_at=None, base={"ref": "main"}, merge_commit_sha=sha)
        unrelated = dict(self.pull(6), merged_at="2026-01-01", base={"ref": "main"}, merge_commit_sha="b" * 40)
        with patch("release_publication_checks.checked_git", return_value=sha), \
             patch("release_publication_checks.api", return_value=[[main, other, unmerged, unrelated, main]]) as api:
            self.assertEqual(related_pulls(Path("."), "owner/repo", "v1.2.3", sha, "v1.2.2"), [main])
            self.assertIn(sha, api.call_args.args[0])

    def test_merge_strategies_require_result_commit_in_range(self):
        commits = ["a" * 40, "b" * 40, "c" * 40]
        pulls = [dict(self.pull(number), merged_at="2026-01-01", base={"ref": "main"}, merge_commit_sha=sha)
                 for number, sha in enumerate(commits, 1)]
        with patch("release_publication_checks.checked_git", return_value="\n".join(commits)), \
             patch("release_publication_checks.api", return_value=[pulls]):
            self.assertEqual(related_pulls(Path("."), "owner/repo", "v1.2.3", commits[-1], "v1.2.2"), pulls)

    def test_notes_require_unique_generated_sections_and_history(self):
        summary = "## Overview\n\nA summary."
        generated = "## What's Changed\n\n- Fix\n\n**Full Changelog**: link"
        history = "Changes since v1.2.2\n\n- abc Fix"
        body = assemble_notes(summary, generated, history)
        self.assertEqual(body.count("## What's Changed"), 1)
        self.assertEqual(body.count("**Full Changelog**:"), 1)
        self.assertEqual(body.count("## Commit History"), 1)
        for changed in (generated + "\n## What's Changed\n\n- Repeat", generated + "\n**Full Changelog**: other", "## What's Changed\n\n- Fix"):
            with self.subTest(changed=changed), self.assertRaises(ValueError):
                assemble_notes(summary, changed, history)
        with self.assertRaisesRegex(ValueError, "empty"):
            assemble_notes(summary, generated, "Changes since baseline")
        with self.assertRaisesRegex(ValueError, "duplicate list content"):
            assemble_notes(summary + "\n\n- Fix", generated, history)

    def test_pre_tag_failures_leave_no_release_content(self):
        sha = "a" * 40
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            title, notes, manifest = (root / name for name in ("title.txt", "notes.md", "manifest.json"))
            folder = root / ".github/release-metadata"
            folder.mkdir(parents=True)
            metadata_file = folder / "v1.2.3.md"
            common = (patch("release_publication_checks.checked_git", return_value=sha),
                      patch("version_guard.checked_project_version", return_value="1.2.3"),
                      patch("release_publication_checks.release_notes", return_value=("v1.2.2", "baseline", ["one"], "Changes since baseline\n\n- one\n")))
            for content, pulls, error in (("---\ntitle: Broken\n---\n", [self.pull()], "requires both"),
                                          (None, [self.pull(), self.pull(4)], "found 2 PRs")):
                if content is None:
                    metadata_file.unlink(missing_ok=True)
                else:
                    metadata_file.write_text(content, encoding="utf-8")
                with common[0], common[1], common[2], \
                     patch("release_publication_checks.related_pulls", return_value=pulls), \
                     self.assertRaisesRegex(ValueError, error):
                    prepare_content(root, "owner/repo", "v1.2.3", sha, title, notes, manifest)
                self.assertFalse(any(path.exists() for path in (title, notes, manifest)))
            with common[0], common[1], \
                 patch("release_publication_checks.release_notes", side_effect=subprocess.CalledProcessError(1, "gh api")), \
                 self.assertRaises(subprocess.CalledProcessError):
                prepare_content(root, "owner/repo", "v1.2.3", sha, title, notes, manifest)
            self.assertFalse(any(path.exists() for path in (title, notes, manifest)))

    def test_workflow_prepares_and_verifies_before_tag_creation(self):
        workflow = (Path(__file__).resolve().parents[3] / "workflows/publish-release.yml").read_text(encoding="utf-8")
        prepare = workflow.index("  prepare-release-content:")
        create = workflow.index("  create-release-tag:")
        self.assertLess(prepare, create)
        self.assertIn("needs: prepare-release-content", workflow[create:workflow.index("  validate-release-publication:")])
        self.assertLess(workflow.index("Verify prepared content identity", create), workflow.index("Create version tag", create))


if __name__ == "__main__":
    unittest.main()
