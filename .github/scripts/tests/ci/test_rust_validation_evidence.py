"""Check that Rust workspace evidence and installed Host simulation stay distinct."""

import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "ci"))
from rust_validation_evidence import PLATFORMS, HOSTS, validate_evidence


class RustValidationEvidenceTests(unittest.TestCase):
    def fixture(self, root):
        rust = {}
        hosts = {}
        for platform in PLATFORMS:
            rust[platform] = root / platform / "rust"
            hosts[platform] = root / platform / "installed-host"
            (rust[platform] / ".ci-results").mkdir(parents=True)
            hosts[platform].mkdir()
            result = {"commit": "abc123", "platform": platform, "run_id": "123", "run_attempt": "1", "status": "passed", "target": platform,
                      "test_count": 2, "test_ids_sha256": "digest",
                      "tests": [{"id": "skill-model::test_one", "outcome": "passed"},
                                {"id": "work-cli::test_two", "outcome": "passed"}],
                      "doc_tests": {"passed": 1, "failed": 0},
                      "phases": {phase: {"status": "passed", "command": ["cargo", "--workspace"]}
                                 for phase in ("compile", "test", "doc")}}
            (rust[platform] / ".ci-results/test-results.json").write_text(json.dumps(result), encoding="utf-8")
            for host in HOSTS:
                content = {"host": host, "platform": platform, "commit": "abc123", "run_id": "123",
                           "run_attempt": "1", "status": "passed", "version": "1.0.0", "zip": "package.zip", "zip_sha256": "digest",
                           "simulation": {"work_contracts": 25, "skill_contracts": 5, "on_demand_sources": 3,
                                          "discussion_versions": 3, "trace_anchors": 6,
                                          "independent_skills": ["work", "tooling"]}}
                (hosts[platform] / f"{host}-{platform}.json").write_text(json.dumps(content), encoding="utf-8")
        return rust, hosts

    def test_valid_evidence_for_both_platforms_and_hosts(self):
        with tempfile.TemporaryDirectory() as directory:
            rust, hosts = self.fixture(Path(directory))
            validate_evidence(rust, hosts, "abc123", "123", dict.fromkeys(PLATFORMS, "1"), dict.fromkeys(PLATFORMS, "1"))

    def test_missing_or_mismatched_evidence_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            rust, hosts = self.fixture(Path(directory))
            attempts = dict.fromkeys(PLATFORMS, "1")
            with self.assertRaisesRegex(ValueError, "incomplete or failed"):
                validate_evidence(rust, hosts, "different-commit", "123", attempts, attempts)
            with self.assertRaisesRegex(ValueError, "incomplete or failed"):
                validate_evidence(rust, hosts, "abc123", "different-run", attempts, attempts)
            with self.assertRaisesRegex(ValueError, "incomplete or failed"):
                validate_evidence(rust, hosts, "abc123", "123", dict.fromkeys(PLATFORMS, "2"), attempts)
            failed_path = hosts["windows-x64-msvc"] / "claude-windows-x64-msvc.json"
            failed = json.loads(failed_path.read_text(encoding="utf-8"))
            failed["status"] = "failed"
            failed_path.write_text(json.dumps(failed), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "invalid"):
                validate_evidence(rust, hosts, "abc123", "123", attempts, attempts)
            failed_path.unlink()
            with self.assertRaisesRegex(ValueError, "incomplete"):
                validate_evidence(rust, hosts, "abc123", "123", attempts, attempts)


if __name__ == "__main__":
    unittest.main()
