import json
import io
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "ci"))
import rust_test_results


class RustTestResultsTests(unittest.TestCase):
    def test_environment_records_same_fields_for_both_platforms(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(rust_test_results.subprocess, "run", side_effect=[SimpleNamespace(stdout="host: aarch64-apple-darwin\n")] + [SimpleNamespace(stdout="abc\n"), SimpleNamespace(stdout="rustc 1.85\n"), SimpleNamespace(stdout="cargo 1.85\n")] * 2):
            for platform, target in (("macos-arm64", "native"), ("windows-x64-msvc", "x86_64-pc-windows-msvc")):
                self.assertEqual(rust_test_results.environment_command(SimpleNamespace(directory=directory, platform=platform, target=target)), 0)
                data = json.loads((Path(directory) / "environment.json").read_text(encoding="utf-8"))
                self.assertEqual(data["checkout_commit"], "abc")
                self.assertEqual(data["rustc"], "rustc 1.85")
                self.assertEqual(data["cargo"], "cargo 1.85")
                self.assertEqual(data["platform"], platform)
                self.assertEqual(data["target"], "aarch64-apple-darwin" if target == "native" else target)

    def test_report_preserves_resolved_native_target(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            args = SimpleNamespace(directory=root, junit=root / "missing.xml", output=root / "test-results.json",
                                   platform="macos-arm64", target="native", min_doc_tests=1)
            self.assertEqual(rust_test_results.report_command(args), 0)
            self.assertIsNone(json.loads(args.output.read_text(encoding="utf-8"))["target"])
            (root / "environment.json").write_text(json.dumps({"target": "aarch64-apple-darwin"}), encoding="utf-8")
            self.assertEqual(rust_test_results.report_command(args), 0)
            self.assertEqual(json.loads(args.output.read_text(encoding="utf-8"))["target"], "aarch64-apple-darwin")

    def test_measure_displays_unicode_when_console_starts_as_cp1252(self):
        with tempfile.TemporaryDirectory() as directory:
            console_bytes = io.BytesIO()
            console = io.TextIOWrapper(console_bytes, encoding="cp1252")
            args = SimpleNamespace(directory=directory, phase="test", command=[sys.executable, "-X", "utf8", "-c", "print('SLOW [>120.000s] (───────) 測試通過')"])
            with patch.dict("os.environ", {"PYTHONIOENCODING": "utf-8"}), patch.object(sys, "stdout", console):
                self.assertEqual(rust_test_results.measure(args), 0)
            self.assertIn("SLOW [>120.000s] (───────) 測試通過", (Path(directory) / "test.log").read_text(encoding="utf-8"))
            self.assertIn("SLOW [>120.000s] (───────) 測試通過".encode("utf-8"), console_bytes.getvalue())
            self.assertEqual(json.loads((Path(directory) / "test.json").read_text(encoding="utf-8"))["exit_code"], 0)

    def test_complete_report_requires_nextest_and_doc_results(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for phase in rust_test_results.PHASES:
                (root / f"{phase}.json").write_text(json.dumps({"phase": phase, "duration_seconds": 2.5, "exit_code": 0}), encoding="utf-8")
            (root / "doc.log").write_text("test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n", encoding="utf-8")
            (root / "junit.xml").write_text('<testsuites><testsuite name="binary"><testcase classname="suite" name="case" time="1.25" /></testsuite></testsuites>', encoding="utf-8")
            report = rust_test_results.build_report(root, root / "junit.xml", {"platform": "macos", "commit": "abc", "toolchain": "1.85.0"})
            self.assertEqual(report["status"], "passed")
            self.assertEqual(report["doc_tests"]["passed"], 1)
            self.assertEqual(report["test_count"], 1)
            self.assertEqual(report["test_counts"], {"passed": 1, "failed": 0, "skipped": 0})
            self.assertEqual(report["phases"]["compile"]["duration_seconds"], 2.5)

    def test_missing_junit_is_not_zero_tests(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            report = rust_test_results.build_report(root, root / "missing.xml", {})
            self.assertIsNone(report["test_count"])
            self.assertIsNone(report["test_counts"])
            self.assertNotEqual(report["status"], "passed")

    def test_successful_doc_command_with_no_test_binaries_reports_zero_tests(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for phase in rust_test_results.PHASES:
                (root / f"{phase}.json").write_text(json.dumps({"phase": phase, "duration_seconds": 2.5, "exit_code": 0}), encoding="utf-8")
            (root / "doc.log").write_text("    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.06s\n", encoding="utf-8")
            (root / "junit.xml").write_text('<testsuites><testsuite name="binary"><testcase name="case" time="1" /></testsuite></testsuites>', encoding="utf-8")
            report = rust_test_results.build_report(root, root / "junit.xml", {})
            self.assertEqual(report["status"], "passed")
            self.assertEqual(report["doc_tests"], {"passed": 0, "failed": 0, "ignored": 0})
            required = rust_test_results.build_report(root, root / "junit.xml", {}, min_doc_tests=1)
            self.assertEqual(required["status"], "incomplete")
            self.assertIn("Expected at least 1 passing doc tests, found 0", required["diagnostics"])
            self.assertEqual(rust_test_results.enforce_command(SimpleNamespace(directory=root, junit=root / "junit.xml", min_doc_tests=1)), 1)
            (root / "doc.log").write_text("    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.06s\nrunning 1 test\n", encoding="utf-8")
            self.assertIn("Doc test results missing", rust_test_results.build_report(root, root / "junit.xml", {})["diagnostics"])

    def test_report_preserves_failure_and_missing_doc_diagnostics(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for phase, code in (("compile", 0), ("test", 1), ("doc", 0)):
                (root / f"{phase}.json").write_text(json.dumps({"phase": phase, "duration_seconds": 2.5, "exit_code": code}), encoding="utf-8")
            (root / "junit.xml").write_text('<testsuites><testsuite name="binary"><testcase classname="suite" name="case" time="1.25"><failure /></testcase></testsuite></testsuites>', encoding="utf-8")
            report = rust_test_results.build_report(root, root / "junit.xml", {"platform": "macos", "commit": "abc", "toolchain": "1.85.0"})
            self.assertEqual(report["status"], "failed")
            self.assertEqual(report["tests"][0]["outcome"], "failed")
            self.assertIn("Doc test results missing", report["diagnostics"])

    def test_invalid_junit_duration_cannot_be_reported_as_success(self):
        with tempfile.TemporaryDirectory() as directory:
            junit = Path(directory) / "junit.xml"
            junit.write_text('<testsuites><testsuite name="binary"><testcase name="case" time="NaN" /></testsuite></testsuites>', encoding="utf-8")
            tests, error = rust_test_results.parse_junit(junit)
            self.assertEqual(tests, [])
            self.assertIn("invalid", error)

    def test_enforcement_uses_test_evidence_not_uploaded_report(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for phase, code in (("compile", 0), ("test", 1), ("doc", 0)):
                (root / f"{phase}.json").write_text(json.dumps({"phase": phase, "duration_seconds": 1, "exit_code": code}), encoding="utf-8")
            (root / "doc.log").write_text("test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n", encoding="utf-8")
            (root / "junit.xml").write_text('<testsuites><testsuite name="binary"><testcase name="case" time="1" /></testsuite></testsuites>', encoding="utf-8")
            (root / "test-results.json").write_text('{"status":"passed"}', encoding="utf-8")
            self.assertEqual(rust_test_results.enforce_command(SimpleNamespace(directory=root, junit=root / "junit.xml", min_doc_tests=0)), 1)

    def test_enforcement_logs_failed_test_and_captured_stderr(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for phase, code in (("compile", 0), ("test", 100), ("doc", 0)):
                (root / f"{phase}.json").write_text(json.dumps({"phase": phase, "duration_seconds": 1, "exit_code": code}), encoding="utf-8")
            (root / "doc.log").write_text("test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n", encoding="utf-8")
            (root / "junit.xml").write_text(
                '<testsuites><testsuite name="work-infrastructure"><testcase classname="process::host::tests" name="termination" time="1">'
                '<failure message="test failure">assertion failed: cleanup_error.is_none()<system-err>leader_wait_result=258\nassertion failed</system-err></failure>'
                '</testcase><testcase name="passed" time="0.1" /></testsuite></testsuites>',
                encoding="utf-8",
            )
            stderr = io.StringIO()
            with patch.object(sys, "stderr", stderr):
                self.assertEqual(rust_test_results.enforce_command(SimpleNamespace(directory=root, junit=root / "junit.xml", min_doc_tests=1)), 1)
            output = stderr.getvalue()
            self.assertIn("Failed test: work-infrastructure::process::host::tests::termination", output)
            self.assertIn("Failure: test failure", output)
            self.assertIn("Failure output: assertion failed: cleanup_error.is_none()", output)
            self.assertIn("stderr: leader_wait_result=258", output)
            self.assertNotIn("Failed test: work-infrastructure::::passed", output)


if __name__ == "__main__":
    unittest.main()
