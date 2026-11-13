"""Check that Rust workspace evidence and installed CLI smoke stay distinct."""

import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "release"))
from rust_workspace_report import PLATFORMS, HOSTS, compose_report


class RustWorkspaceReportTests(unittest.TestCase):
    def fixture(self, root):
        rust = {}
        smoke = {}
        for platform in PLATFORMS:
            rust[platform] = root / platform / "rust"
            smoke[platform] = root / platform / "cli-smoke"
            (rust[platform] / ".ci-perf").mkdir(parents=True)
            smoke[platform].mkdir()
            result = {"commit": "abc123", "status": "passed", "target": platform,
                      "test_count": 2, "test_ids_sha256": "digest",
                      "tests": [{"id": "skill-model::test_one", "outcome": "passed"},
                                {"id": "work-cli::test_two", "outcome": "passed"}],
                      "doc_tests": {"passed": 1, "failed": 0},
                      "phases": {phase: {"status": "passed", "command": ["cargo", "--workspace"]}
                                 for phase in ("compile", "test", "doc")}}
            (rust[platform] / ".ci-perf/performance.json").write_text(json.dumps(result), encoding="utf-8")
            for host in HOSTS:
                content = ("# Installed Rust CLI smoke evidence\n\n"
                           "$ work --project-root <root> task validate\n"
                           "## Tooling CLI smoke\n\n```json\n" +
                           json.dumps({"first_sources": 4, "changed_sources": 5,
                                       "target_boundary": "target_valid",
                                       "incompatible_policy_blocked": True}) + "\n```\n")
                (smoke[platform] / f"{host}-{platform}.md").write_text(content, encoding="utf-8")
        return rust, smoke

    def test_report_covers_both_platforms_workspace_suites_and_installed_clis(self):
        with tempfile.TemporaryDirectory() as directory:
            rust, smoke = self.fixture(Path(directory))
            report = compose_report(rust, smoke, "abc123")
            for platform in PLATFORMS:
                self.assertIn(platform, report)
                for host in HOSTS:
                    self.assertIn(f"{host}-{platform}.md", report)
            self.assertIn("skill-model", report)
            self.assertIn("work-cli", report)
            self.assertIn("Rust workspace validation", report)

    def test_missing_or_mismatched_evidence_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            rust, smoke = self.fixture(Path(directory))
            with self.assertRaisesRegex(ValueError, "incomplete or failed"):
                compose_report(rust, smoke, "different-commit")
            (smoke["windows-x64-msvc"] / "claude-windows-x64-msvc.md").unlink()
            with self.assertRaisesRegex(ValueError, "incomplete"):
                compose_report(rust, smoke, "abc123")


if __name__ == "__main__":
    unittest.main()
