"""Tests for success and failure release reports."""

import contextlib
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "release"))
from release_publication_checks import expected_assets
from release_verification_report import build_report, main


class ReleaseVerificationReportTests(unittest.TestCase):
    def test_failed_verification_still_writes_json_and_summary(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            report_path = root / "release-verification.json"
            summary_path = root / "summary.md"
            arguments = [
                "release_verification_report.py", "--version", "1.2.3", "--commit", "a" * 40,
                "--run-url", "https://example.invalid/run", "--expected-dir", str(root / "expected"),
                "--actual-dir", str(root / "actual"), "--output", str(report_path),
            ]
            for stage in ("validate", "macos-e2e", "windows-e2e", "attest", "publish", "post-publish-verify"):
                arguments.extend((f"--{stage}-status", "failure" if stage == "post-publish-verify" else "success"))
            with patch("sys.argv", arguments), patch.dict("os.environ", {"GITHUB_STEP_SUMMARY": str(summary_path)}), \
                 contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(main(), 1)
            report = json.loads(report_path.read_text(encoding="utf-8"))
            self.assertEqual(report["result"], "failure")
            self.assertEqual(report["stages"]["post_publish_verify"], "failure")
            self.assertIn("Release verification: failure", summary_path.read_text(encoding="utf-8"))

    def test_records_matching_assets_and_failures(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            expected = root / "expected"
            actual = root / "actual"
            expected.mkdir()
            actual.mkdir()
            for name in expected_assets("1.2.3"):
                (expected / name).write_bytes(name.encode())
                (actual / name).write_bytes(name.encode())
            stages = {name: "success" for name in (
                "validate", "macos_e2e", "windows_e2e", "attest", "publish", "post_publish_verify"
            )}
            report = build_report("1.2.3", "a" * 40, "https://example.invalid/run", stages, expected, actual)
            self.assertEqual(report["result"], "success")
            self.assertTrue(report["attestation_verified"])
            (actual / "SHA256SUMS.txt").write_bytes(b"changed")
            stages["post_publish_verify"] = "failure"
            report = build_report("1.2.3", "a" * 40, "https://example.invalid/run", stages, expected, actual)
            self.assertEqual(report["result"], "failure")
            self.assertFalse(report["attestation_verified"])
            self.assertTrue(report["failure_reasons"])


if __name__ == "__main__":
    unittest.main()
