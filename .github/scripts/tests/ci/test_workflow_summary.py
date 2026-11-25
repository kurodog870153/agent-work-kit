import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "ci"))
from workflow_summary import (WORKFLOW_JOBS, PLATFORMS, collect_platform, combine, duration_bar, elapsed_label, identity, render, result, status, step_result)


class WorkflowSummaryTests(unittest.TestCase):
    def setUp(self):
        self.meta = {"repository": "owner/repo", "run_id": "123", "attempt": "2", "commit": "abc", "ref": "main", "elapsed": 19}
        self.platforms = {p: {"rows": {"Formatting": result("Passed", 10), "Workspace tests": result("Failed", 20),
                                       "Installed Codex Host simulation": result("Passed"), "Installed Claude Host simulation": result("Unavailable")},
                              "rust": {"test_counts": {"passed": 2, "failed": 1, "skipped": 0},
                                       "doc_tests": {"passed": 1, "failed": 0, "ignored": 0}}} for p in PLATFORMS}

    def test_all_workflow_scopes(self):
        for kind in WORKFLOW_JOBS:
            needs = {key: {"result": "success"} for key in WORKFLOW_JOBS[kind]}
            text = render(kind, self.meta, needs, self.platforms)
            self.assertEqual(text.count("# " + {"branch": "Branch validation", "pr": "PR release validation", "publish": "Publish release"}[kind]), 1)
            self.assertIn("macOS | Windows", text)
            self.assertIn("Workspace test statistics", text)
            self.assertIn("Installed Codex Host simulation", text)
            self.assertIn("█", text)
            self.assertNotIn("mermaid", text)
            self.assertEqual("PR checks" in text, kind == "pr")
            self.assertEqual("Release evidence" in text, kind == "publish")
        credentials = render("credentials", self.meta, {"credentials": {"result": "failure"}})
        self.assertIn("Validation result: Failed", credentials)
        self.assertNotIn("macOS", credentials)
        self.assertNotIn("Release evidence", credentials)

    def test_states_and_missing_values(self):
        for raw, expected in (("success", "Passed"), ("failure", "Failed"), ("skipped", "Skipped"), ("cancelled", "Cancelled"), ("timed_out", "Timed out"), (None, "Unavailable")):
            self.assertEqual(status(raw), expected)
        self.assertEqual(combine([result("Passed", 2), result("Unavailable")])["status"], "Unavailable")
        self.assertEqual(combine([result("Failed", 2), result("Unavailable")])["status"], "Failed")
        text = render("branch", self.meta, {"preflight": {"result": "failure"}}, self.platforms)
        self.assertIn("Validation result: Failed", text)
        self.assertIn("Unavailable", text)
        self.assertIn("Elapsed at summary: 19", text)

    def test_actual_attempt_and_identity(self):
        good = {"commit": "abc", "run_id": "123", "run_attempt": "1", "platform": PLATFORMS[0]}
        self.assertTrue(identity(good, "abc", "123", 1, PLATFORMS[0]))
        self.assertFalse(identity(good, "abc", "123", 2, PLATFORMS[0]))
        self.assertFalse(identity(good, "other", "123", 1))
        jobs = [{"name": "rust validation / validate and build rust (macOS)", "run_attempt": 1,
                 "steps": [{"name": "Run Rust unit and integration tests", "conclusion": "success"}]}]
        with patch("workflow_summary.read_artifact", return_value=None) as read:
            data = collect_platform(jobs, PLATFORMS[0], "abc", "123")
            self.assertEqual(data["rows"]["Workspace tests"]["status"], "Unavailable")
            read.assert_called_once_with("123", "rust-test-results-macos-arm64-attempt-1", "test-results.json")

    def test_installed_host_failure_uses_actual_attempt_evidence(self):
        platform = PLATFORMS[0]
        jobs = [{"name": "build macOS package", "run_attempt": 1,
                 "steps": [{"name": "Verify macOS release installation", "conclusion": "failure"}]}]
        record = {"host": "codex", "platform": platform, "commit": "abc", "run_id": "123",
                  "run_attempt": "1", "status": "failed", "duration_seconds": 7}
        def read(run, name, filename):
            if name == f"installed-host-simulation-{platform}-attempt-1" and filename.startswith("codex"):
                return record
            return None
        with patch("workflow_summary.read_artifact", side_effect=read):
            data = collect_platform(jobs, platform, "abc", "123")
        self.assertEqual(data["rows"]["Installed Codex Host simulation"], result("Failed", 7))
        self.assertEqual(data["rows"]["Installed Claude Host simulation"]["status"], "Unavailable")

    def test_section_and_package_order(self):
        text = render("branch", {**self.meta, "elapsed": 740}, {key: {"result": "success"} for key in WORKFLOW_JOBS["branch"]}, self.platforms)
        self.assertIn("Elapsed at summary: 12m 20s", text)
        self.assertLess(text.index("## Rust validation"), text.index("### Workspace test statistics"))
        self.assertLess(text.index("### Doc test statistics"), text.index("## Package validation"))
        for earlier, later in (("Package build", "Package integrity"), ("Package integrity", "Downloader E2E"),
                               ("Downloader E2E", "Installer E2E"), ("Installer E2E", "Installed Codex Host simulation"),
                               ("Installed Codex Host simulation", "Installed Claude Host simulation")):
            self.assertLess(text.index(f"| {earlier} |"), text.index(f"| {later} |"))
        self.assertIn("Installed Host simulations are part of Installer E2E", text)

    def test_duration_precision_and_proportional_bars(self):
        self.assertEqual(elapsed_label(3723), "1h 2m 3s")
        self.assertEqual(elapsed_label(None), "Unavailable")
        self.assertEqual(duration_bar(result("Passed", 0.2), 204), "0.2 s")
        self.assertEqual(duration_bar(result("Passed", 0.01), 204), "<0.1 s")
        self.assertEqual(duration_bar(result("Passed", 0), 204), "0.0 s")
        self.assertEqual(duration_bar(result("Passed", 204), 204), "█" * 20 + " 204.0 s")
        self.assertEqual(duration_bar(result(), 204), "Unavailable")
        job = {"steps": [{"name": "fmt", "conclusion": "success", "started_at": "2026-11-25T00:00:00Z",
                           "completed_at": "2026-11-25T00:00:00Z"}]}
        self.assertEqual(duration_bar(step_result(job, "fmt"), 204), "~0 s")

    def test_untrusted_text_is_escaped(self):
        meta = {**self.meta, "ref": "<img src=x>\n| injected |"}
        text = render("credentials", meta, {"credentials": {"result": "success"}})
        self.assertNotIn("<img", text)
        self.assertNotIn("| injected |", text)
        self.assertIn("&#124; injected", text)
