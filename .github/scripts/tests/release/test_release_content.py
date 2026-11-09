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
from release_content import declarations, metadata, preflight_sources, resolve, valid_title
from release_publication_checks import assemble_notes, check_metadata_alignment, prepare_content, related_pulls, verify_content


class ReleaseContentTests(unittest.TestCase):
    def pull(self, number=3, title="feat: Multi-host packaging", body="A unified package for two hosts."):
        return {"number": number, "title": title, "body": body}

    def test_historical_metadata_requires_published_release(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            folder = root / ".github/release-metadata"
            folder.mkdir(parents=True)
            for name in ("v1.2.0.md", "v1.2.1.md", "v1.2.2.md", "v1.2.3.md"):
                (folder / name).write_text("metadata", encoding="utf-8")
            published = [{"tag_name": "v1.2.0"}, {"tag_name": "v1.2.2"}]
            with self.assertRaisesRegex(ValueError, "v1.2.1.md"):
                check_metadata_alignment(root, "v1.2.3", published)
            (folder / "v1.2.1.md").unlink()
            check_metadata_alignment(root, "v1.2.3", published)
            (folder / "v1.2.4.md").write_text("metadata", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "v1.2.4.md"):
                check_metadata_alignment(root, "v1.2.3", published)

    def test_preparation_rejects_skipped_metadata_before_writing_files(self):
        sha = "a" * 40
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            folder = root / ".github/release-metadata"
            folder.mkdir(parents=True)
            (folder / "v1.2.1.md").write_text("old metadata", encoding="utf-8")
            title, notes, manifest = (root / name for name in ("title.txt", "notes.md", "manifest.json"))
            with patch("release_publication_checks.checked_git", return_value=sha), \
                 patch("version_guard.checked_project_version", return_value="1.2.3"), \
                 patch("release_publication_checks.api", return_value=[[{"tag_name": "v1.2.0", "draft": False, "prerelease": False},
                                                                         {"tag_name": "v1.2.1", "draft": True, "prerelease": False}]]), \
                 self.assertRaisesRegex(ValueError, "v1.2.1.md"):
                prepare_content(root, "owner/repo", "v1.2.3", sha, title, notes, manifest)
            self.assertFalse(any(path.exists() for path in (title, notes, manifest)))

    def test_release_notes_preserve_declared_summary_across_versions(self):
        sha = "a" * 40
        scenarios = (
            ("v2.3.0", "v2.3.1", ("Change A",), ("Change A",), "Earlier published change"),
            ("v2.3.0", "v2.3.2", ("Change A", "Change B"), ("Change A", "Change B"), "Earlier published change"),
            ("v2.3.0", "v2.3.3", ("Change A", "Change B", "Change C"),
             ("Change A", "Change B", "Change C"), "Earlier published change"),
        )
        for baseline, tag, commits, highlights, previous_change in scenarios:
            with self.subTest(tag=tag, commits=commits), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                folder = root / ".github/release-metadata"
                folder.mkdir(parents=True)
                summary = "\n".join(f"- {item}" for item in highlights)
                (folder / f"{tag}.md").write_text(
                    f"---\ntitle: {tag} — Release changes\n---\n\n## Highlights\n\n{summary}\n",
                    encoding="utf-8")
                title, notes, manifest = (root / name for name in ("title.txt", "notes.md", "manifest.json"))
                history = f"Changes since {baseline}\n\n" + "\n".join(f"- {item}" for item in commits) + "\n"
                with patch("release_publication_checks.checked_git", return_value=sha), \
                     patch("version_guard.checked_project_version", return_value=tag.removeprefix("v")), \
                     patch("release_publication_checks.api", return_value=[[{"tag_name": baseline, "draft": False, "prerelease": False}]]), \
                     patch("release_publication_checks.release_notes", return_value=(baseline, baseline, list(commits), history)), \
                     patch("release_publication_checks.related_pulls", return_value=[]), \
                     patch("release_publication_checks.generate_release_content", return_value="## What's Changed\n\n**Full Changelog**: link"):
                    prepare_content(root, "owner/repo", tag, sha, title, notes, manifest)
                body = notes.read_text(encoding="utf-8")
                declared = body.split("## What's Changed", 1)[0]
                for item in highlights:
                    self.assertEqual(declared.count(item), 1)
                self.assertNotIn(previous_change, declared)

    def test_skipped_version_changes_missing_from_summary_are_not_automatically_rejected(self):
        sha = "a" * 40
        tag = "v2.3.3"
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            folder = root / ".github/release-metadata"
            folder.mkdir(parents=True)
            (folder / f"{tag}.md").write_text(
                "---\ntitle: v2.3.3 — Change C\n---\n\n## Highlights\n\n- Change C\n",
                encoding="utf-8")
            title, notes, manifest = (root / name for name in ("title.txt", "notes.md", "manifest.json"))
            history = "Changes since v2.3.0\n\n- Change A\n- Change B\n- Change C\n"
            with patch("release_publication_checks.checked_git", return_value=sha), \
                 patch("version_guard.checked_project_version", return_value="2.3.3"), \
                 patch("release_publication_checks.api", return_value=[[{"tag_name": "v2.3.0", "draft": False, "prerelease": False}]]), \
                 patch("release_publication_checks.release_notes", return_value=("v2.3.0", "v2.3.0", ["Change A", "Change B", "Change C"], history)), \
                 patch("release_publication_checks.related_pulls", return_value=[]), \
                 patch("release_publication_checks.generate_release_content", return_value="## What's Changed\n\n**Full Changelog**: link"):
                prepare_content(root, "owner/repo", tag, sha, title, notes, manifest)
            body = notes.read_text(encoding="utf-8")
            declared = body.split("## What's Changed", 1)[0]
            self.assertIn("Change C", declared)
            self.assertNotIn("Change A", declared)
            self.assertNotIn("Change B", declared)
            self.assertIn("Change A", body.split("## Commit History", 1)[1])
            self.assertIn("Change B", body.split("## Commit History", 1)[1])

    def test_metadata_requires_both_fields(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            folder = root / ".github/release-metadata"
            folder.mkdir(parents=True)
            path = folder / "v1.2.3.md"
            path.write_text("---\ntitle: v1.2.3 — Multi-host packaging\n---\n\n## Overview\n\nUnified packaging.\n", encoding="utf-8")
            fields, sources = resolve(root, "v1.2.3", [self.pull()])
            self.assertEqual(fields["title"], "v1.2.3 — Multi-host packaging")
            self.assertIn("Overview", fields["description"])
            self.assertIn("v1.2.3.md", sources["title"])
            for partial in ("---\ntitle: v1.2.3 — Multi-host packaging\n---\n", "---\n---\n\n## Overview\n\nUnified packaging.\n"):
                with self.subTest(partial=partial):
                    path.write_text(partial, encoding="utf-8")
                    with self.assertRaisesRegex(ValueError, "requires both Title and Description"):
                        resolve(root, "v1.2.3", [self.pull()])

    def test_pr_can_still_declare_one_field_without_metadata(self):
        pull = self.pull(body="## Release Title\n\nv1.2.3 — Multi-host packaging")
        fields, sources = resolve(Path("/missing"), "v1.2.3", [pull])
        self.assertEqual(fields["title"], "v1.2.3 — Multi-host packaging")
        self.assertEqual(fields["description"], "Multi-host packaging")
        self.assertEqual(sources["description"], "PR #3 fallback")

    def test_checked_in_metadata_covers_published_range(self):
        root = Path(__file__).resolve().parents[4]
        fields, sources = resolve(root, "v2.2.4", [])
        self.assertEqual(fields["title"], "v2.2.4 — Structured Release Notes and Reliable Publication")
        for detail in ("metadata", "permission", "pull request", "retries"):
            self.assertIn(detail, fields["description"].lower())
        self.assertEqual(Path(sources["title"]).parts, (".github", "release-metadata", "v2.2.4.md"))
        self.assertFalse((root / ".github/release-metadata/v2.2.1.md").exists())
        self.assertFalse((root / ".github/release-metadata/v2.2.3.md").exists())

    def test_metadata_and_pr_titles_must_match_current_tag(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            folder = root / ".github/release-metadata"
            folder.mkdir(parents=True)
            (folder / "v1.2.3.md").write_text(
                "---\ntitle: v1.2.4 — Multi-host packaging\n---\n\nUnified packaging.\n", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "must start"):
                resolve(root, "v1.2.3", [])
        with self.assertRaisesRegex(ValueError, "must start"):
            resolve(Path("/missing"), "v1.2.3", [self.pull(body="## Release Title\n\nv1.2.4 — Multi-host packaging")])

    def test_explicit_pr_declarations_and_conflicts(self):
        body = "## Release Title\n\nv1.2.3 — Multi-host packaging\n\n## Release Description\n\n## Overview\n\nUnified packaging."
        fields, sources = resolve(Path("/missing"), "v1.2.3", [self.pull(body=body), self.pull(4, body=body)])
        self.assertEqual(fields["title"], "v1.2.3 — Multi-host packaging")
        self.assertEqual(sources["title"], "PR #3")
        with self.assertRaisesRegex(ValueError, "conflicting"):
            resolve(Path("/missing"), "v1.2.3", [self.pull(body=body), self.pull(4, body=body.replace("Multi-host", "Other"))])
        with self.assertRaisesRegex(ValueError, "duplicate"):
            declarations("## Release Title\nv1.2.3 — One\n## Release Title\nv1.2.3 — Two", "v1.2.3")

    def test_ambiguous_and_invalid_sources_fail_closed(self):
        with self.assertRaisesRegex(ValueError, "found 0 PRs"):
            resolve(Path("/missing"), "v1.2.3", [])
        with self.assertRaisesRegex(ValueError, "found 2 PRs"):
            resolve(Path("/missing"), "v1.2.3", [self.pull(), self.pull(4)])
        for title in ("v1.2.3", "v1.2.3 — v1.2.3", "v1.2.3 Multi-host", "v1.2.4 — Multi-host", "Multi-host packaging", "v1.2.3 — Merge pull request #3", "v1.2.3 — Changes", "v1.2.3 — Fix", "  "):
            with self.subTest(title=title), self.assertRaises(ValueError):
                valid_title(title, "v1.2.3")
        self.assertEqual(valid_title("v1.2.3 — Multi-host packaging", "v1.2.3"), "v1.2.3 — Multi-host packaging")
        self.assertEqual(valid_title("v1.2.3 — Release Metadata Aligned with Published Versions", "v1.2.3"),
                         "v1.2.3 — Release Metadata Aligned with Published Versions")
        with self.assertRaisesRegex(ValueError, "summary prose"):
            declarations("## Release Description\n\n## What's Changed\n\n- Fix", "v1.2.3")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            folder = root / ".github/release-metadata"
            folder.mkdir(parents=True)
            (folder / "v1.2.3.md").write_text("---\ntitle: \n---\n", encoding="utf-8")
            with self.assertRaises(ValueError):
                metadata(root, "v1.2.3")

    def test_pr_preflight_uses_release_sources_across_the_published_range(self):
        previous = self.pull(3)
        current = dict(self.pull(4, title="fix: Release notes permission", body=""), base={"ref": "main"})
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with patch("release_publication_checks.checked_git", return_value="a" * 40), \
                 patch("release_publication_checks.api", return_value=[[{"tag_name": "v1.2.2", "draft": False, "prerelease": False}]]), \
                 patch("release_publication_checks.published_baseline", return_value=("v1.2.2", "baseline", "revision")) as baseline, \
                 patch("release_publication_checks.related_pulls", return_value=[previous]):
                with self.assertRaisesRegex(ValueError, "Release title needs metadata or an unambiguous merged PR; found 2 PRs"):
                    preflight_sources(root, "owner/repo", "v1.2.3", current, "refs/remotes/origin/main")
                declared = dict(current, body="## Release Title\n\nv1.2.3 — Reliable publication checks\n\n## Release Description\n\nValidate release sources in PR checks.")
                fields, sources = preflight_sources(root, "owner/repo", "v1.2.3", declared, "refs/remotes/origin/main")
                self.assertEqual(fields["title"], "v1.2.3 — Reliable publication checks")
                self.assertEqual(sources["title"], "PR #4")
                baseline.assert_called_with(root, "owner/repo", "a" * 40)
                folder = root / ".github/release-metadata"
                folder.mkdir(parents=True)
                (folder / "v1.2.3.md").write_text("---\ntitle: v1.2.3 — Reliable publication checks\n---\n\nValidate release sources in PR checks.\n", encoding="utf-8")
                fields, sources = preflight_sources(root, "owner/repo", "v1.2.3", current, "refs/remotes/origin/main")
                self.assertEqual(fields["title"], "v1.2.3 — Reliable publication checks")
                self.assertIn("v1.2.3.md", sources["title"])

    def test_pr_preflight_rejects_unpublished_historical_metadata(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            folder = root / ".github/release-metadata"
            folder.mkdir(parents=True)
            (folder / "v1.2.1.md").write_text("old metadata", encoding="utf-8")
            pull = dict(self.pull(), base={"ref": "main"})
            with patch("release_publication_checks.api", return_value=[[
                    {"tag_name": "v1.2.0", "draft": False, "prerelease": False},
                    {"tag_name": "v1.2.1", "draft": True, "prerelease": False}]]), \
                 self.assertRaisesRegex(ValueError, "v1.2.1.md"):
                preflight_sources(root, "owner/repo", "v1.2.3", pull, "refs/remotes/origin/main")

    def test_prepared_content_is_pinned_and_contains_sections(self):
        sha = "a" * 40
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            title, notes, manifest = (root / name for name in ("title.txt", "notes.md", "manifest.json"))
            with patch("release_publication_checks.checked_git", return_value=sha), \
                 patch("version_guard.checked_project_version", return_value="1.2.3"), \
                 patch("release_publication_checks.api", return_value=[[{"tag_name": "v1.2.2", "draft": False, "prerelease": False}]]), \
                 patch("release_publication_checks.release_notes", return_value=("v1.2.2", "v1.2.2 (sha)", ["one"], "Changes since v1.2.2\n\n- one\n")), \
                 patch("release_publication_checks.related_pulls", return_value=[self.pull()]), \
                 patch("release_publication_checks.generate_release_content", return_value="## What's Changed\n\n- Fix\n\n**Full Changelog**: link"):
                prepare_content(root, "owner/repo", "v1.2.3", sha, title, notes, manifest)
            self.assertEqual(title.read_text(encoding="utf-8"), "v1.2.3 — Multi-host packaging\n")
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
                      patch("release_publication_checks.release_notes", return_value=("v1.2.2", "baseline", ["one"], "Changes since baseline\n\n- one\n")),
                      patch("release_publication_checks.api", return_value=[[{"tag_name": "v1.2.2", "draft": False, "prerelease": False}]]))
            for content, pulls, error in (("---\ntitle: v1.2.3 — Broken\n---\n", [self.pull()], "requires both"),
                                          ("---\ntitle: v1.2.4 — Wrong version\n---\n\nSummary.\n", [self.pull()], "must start"),
                                          (None, [self.pull(), self.pull(4)], "found 2 PRs")):
                if content is None:
                    metadata_file.unlink(missing_ok=True)
                else:
                    metadata_file.write_text(content, encoding="utf-8")
                with common[0], common[1], common[2], common[3], \
                     patch("release_publication_checks.related_pulls", return_value=pulls), \
                     self.assertRaisesRegex(ValueError, error):
                    prepare_content(root, "owner/repo", "v1.2.3", sha, title, notes, manifest)
                self.assertFalse(any(path.exists() for path in (title, notes, manifest)))
            with common[0], common[1], common[3], \
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
        published = workflow.index("      - name: Verify published release content")
        self.assertGreater(published, workflow.index('gh release edit "$TAG" --repo "$REPOSITORY" --draft=false'))
        self.assertIn("assets-published --repository", workflow[published:workflow.index("  verify-published-release:")])


if __name__ == "__main__":
    unittest.main()
