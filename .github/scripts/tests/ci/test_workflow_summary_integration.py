"""Exercise the shared summary entrypoint without contacting GitHub or publishing."""
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "ci"))
import workflow_summary as summary


class WorkflowSummaryIntegrationTests(unittest.TestCase):
    def render_main(self, kind, needs, jobs=(), attempts=None, records=None, credentials="success"):
        calls = []
        attempts = attempts or {}
        records = records or {}

        def api(*args):
            endpoint = args[-1]
            if "/jobs?filter=latest" in endpoint:
                return [{"jobs": [dict(job) for job in jobs]}]
            if "/jobs?per_page" in endpoint:
                attempt = int(endpoint.split("/attempts/")[1].split("/")[0])
                return [{"jobs": [{"id": job_id} for job_id, n in attempts.items() if n == attempt]}]
            if endpoint.endswith("/attempts/3"):
                return {"run_started_at": "2026-11-25T00:00:00Z"}
            raise AssertionError(endpoint)

        def read(run, name, filename):
            calls.append((run, name, filename))
            return records.get((name, filename))

        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "summary.md"
            env = {"GITHUB_REPOSITORY": "owner/repo", "GITHUB_RUN_ID": "123",
                   "GITHUB_RUN_ATTEMPT": "3", "GITHUB_SHA": "abc", "GITHUB_REF": "refs/heads/main",
                   "GITHUB_STEP_SUMMARY": str(path), "NEEDS_JSON": json.dumps(needs),
                   "CREDENTIALS_RESULT": credentials, "RELEASE_ADMIN_TOKEN": "secret-fixture"}
            with patch.dict(os.environ, env, clear=True), patch.object(sys, "argv", ["summary", kind]), \
                    patch.object(summary, "gh_json", side_effect=api) as gh, \
                    patch.object(summary, "read_artifact", side_effect=read):
                summary.main()
                if kind == "credentials":
                    gh.assert_not_called()
                    self.assertFalse(calls)
            return path.read_text(encoding="utf-8"), calls

    def test_pr_actual_step_results_and_scope(self):
        needs = {"confirm-release-validation": {"result": "failure"}}
        jobs = [{"id": 1, "name": "validate release preflight", "conclusion": "failure", "steps": [
            {"name": "Validate pull request version", "conclusion": "success"},
            {"name": "Check proposed release version is unused", "conclusion": "failure"},
            {"name": "Validate release content declarations", "conclusion": "skipped"}]}]
        text, _ = self.render_main("pr", needs, jobs, {1: 3})
        for row in ("| Version guard | Passed |", "| Unused release version | Failed |",
                    "| Release content declarations | Skipped |"):
            self.assertIn(row, text)
        self.assertIn("Validation result: Failed", text)
        self.assertIn("/actions/runs/123/job/1", text)
        self.assertNotIn("## Release evidence", text)

    def test_credentials_only_safe_status_without_api_or_artifacts(self):
        for raw, expected in (("success", "Passed"), ("failure", "Failed"), ("cancelled", "Cancelled")):
            with self.subTest(raw=raw):
                text, _ = self.render_main("credentials", {}, credentials=raw)
                self.assertIn(f"| Release credentials | {expected} |", text)
                self.assertNotIn("secret-fixture", text)
                self.assertNotIn("## Rust validation", text)
                self.assertNotIn("## Release evidence", text)

    def test_publish_mixed_producer_attempts_and_missing_evidence(self):
        needs = {"verify-published-release": {"result": "success",
                                             "outputs": {"verification-artifact-key": "v1.2.3"}}}
        jobs = [{"id": 1, "name": "prepare release content before tag", "conclusion": "success"},
                {"id": 2, "name": "verify published release", "conclusion": "success"},
                {"id": 3, "name": "rust validation / validate and build rust (macOS)"},
                {"id": 4, "name": "build and validate packages / build macOS package"}]
        attempts = {1: 1, 2: 3, 3: 1, 4: 2}
        records = {
            ("validated-release-content-attempt-1", "release-content.json"): {
                "sha": "abc", "tag": "v1.2.3", "baseline_label": "v1.2.2", "sources": ["metadata"]},
            ("release-verification-v1.2.3-attempt-3", "release-verification.json"): {
                "commit": "abc", "result": "success", "attestation_verified": True,
                "assets": [{"matches": True}], "failure_reasons": []},
            ("rust-test-results-macos-arm64-attempt-1", "test-results.json"): {
                "commit": "abc", "run_id": "123", "run_attempt": "1", "platform": "macos-arm64",
                "test_counts": {"passed": 143, "failed": 0, "skipped": 0}},
            ("package-validation-macos-arm64-attempt-2", "package-build-codex.json"): {
                "commit": "abc", "run_id": "123", "run_attempt": "2", "phase": "package-build-codex", "exit_code": 0},
        }
        text, calls = self.render_main("publish", needs, jobs, attempts, records)
        for row in ("| Baseline | v1.2.2 |", "| Target | v1.2.3 |", "| Verification | Passed |",
                    "| Attestation verified | True |", "| Assets matching | 1 |", "| Failure records | 0 |",
                    "| Passed | 143 | Unavailable |", "| Package build | Unavailable | Unavailable |"):
            self.assertIn(row, text)
        self.assertIn(("123", "validated-release-content-attempt-1", "release-content.json"), calls)
        self.assertIn(("123", "release-verification-v1.2.3-attempt-3", "release-verification.json"), calls)
        self.assertIn(("123", "rust-test-results-macos-arm64-attempt-1", "test-results.json"), calls)
        self.assertIn(("123", "package-validation-macos-arm64-attempt-2", "package-build-codex.json"), calls)
        self.assertIn(("123", "installed-host-simulation-macos-arm64-attempt-2", "codex-macos-arm64.json"), calls)
        self.assertNotIn("## PR checks", text)
        records[("validated-release-content-attempt-1", "release-content.json")]["sha"] = "wrong"
        del records[("release-verification-v1.2.3-attempt-3", "release-verification.json")]
        needs["verify-published-release"]["result"] = "failure"
        text, _ = self.render_main("publish", needs, jobs, attempts, records)
        self.assertIn("Release content evidence unavailable.", text)
        self.assertIn("Post-publish verification evidence unavailable.", text)
        self.assertIn("Validation result: Failed", text)

    def test_branch_terminal_only_and_all_required_jobs(self):
        names = ("validate branch preflight", "rust validation / validate and build rust (macOS)",
                 "rust validation / validate and build rust (Windows)",
                 "build and validate packages / build macOS package",
                 "build and validate packages / build Windows package",
                 "build and validate packages / verify macOS checksums",
                 "build and validate packages / verify Windows checksums", "rust validation evidence")
        jobs = [{"id": index, "name": name, "conclusion": "success"}
                for index, name in enumerate(names, 1)]
        attempts = {job["id"]: 1 for job in jobs}
        needs = {"confirm-branch-validation": {"result": "success"}}
        text, _ = self.render_main("branch", needs, jobs, attempts)
        self.assertIn("Validation result: Passed", text)
        self.assertIn("| Rust validation | Passed |", text)
        self.assertIn("| Package validation | Passed |", text)
        jobs[2]["conclusion"] = "failure"
        needs["confirm-branch-validation"]["result"] = "failure"
        text, _ = self.render_main("branch", needs, jobs, attempts)
        self.assertIn("| Rust validation | Failed |", text)
        self.assertIn("Validation result: Failed", text)
        jobs[2]["conclusion"] = "success"
        text, _ = self.render_main("branch", needs, jobs[:-1], attempts)
        self.assertIn("| Strict evidence gate | Unavailable |", text)

    def test_stage_results_preserve_non_success_and_missing_api(self):
        for raw in ("failure", "cancelled", "timed_out", "skipped"):
            with self.subTest(raw=raw):
                jobs = [{"name": "validate and build rust (macOS)", "conclusion": raw},
                        {"name": "validate and build rust (Windows)", "conclusion": "success"}]
                needs = {"confirm-release-validation": {"result": "failure"}}
                result = summary.collect_stage_results("pr", jobs, needs)
                self.assertEqual(result["rust-ci"]["result"], raw)
                self.assertIsNone(result["package-ci"]["result"])
                self.assertEqual(result["confirm-release-validation"]["result"], "failure")
        text, _ = self.render_main("pr", {"confirm-release-validation": {"result": "success"}})
        self.assertIn("Validation result: Unavailable", text)
        self.assertIn("| Confirm release validation | Passed |", text)

    def test_artifact_reader_downloads_once_and_rejects_duplicate_and_invalid_files(self):
        def download(command, **kwargs):
            root = Path(command[command.index("--dir") + 1])
            (root / "a").mkdir()
            (root / "b").mkdir()
            (root / "a/duplicate.json").write_text("{}", encoding="utf-8")
            (root / "b/duplicate.json").write_text("{}", encoding="utf-8")
            (root / "good.json").write_text('{"ok": true}', encoding="utf-8")
            (root / "bad.json").write_text("not json", encoding="utf-8")
        summary.read_artifact_files.cache_clear()
        self.addCleanup(summary.read_artifact_files.cache_clear)
        with patch.object(summary.subprocess, "run", side_effect=download) as run:
            self.assertEqual(summary.read_artifact("123", "evidence-attempt-1", "good.json"), {"ok": True})
            self.assertIsNone(summary.read_artifact("123", "evidence-attempt-1", "duplicate.json"))
            self.assertIsNone(summary.read_artifact("123", "evidence-attempt-1", "bad.json"))
            run.assert_called_once()
