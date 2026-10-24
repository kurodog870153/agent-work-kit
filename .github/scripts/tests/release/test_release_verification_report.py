"""Tests for success and failure release reports."""

import contextlib
import hashlib
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path
from urllib.parse import urlsplit
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "release"))
from release_publication_checks import expected_assets
from release_verification_report import build_report, main, required_checks
from release_post_publish_checks import main as post_publish_main, verify


class ReleaseVerificationReportTests(unittest.TestCase):
    def test_post_publish_exit_status_and_json(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            output = root / "release-checks.json"
            arguments = [
                "release_post_publish_checks.py", "--version", "1.2.3", "--repository", "owner/repo",
                "--tag", "v1.2.3", "--commit", "a" * 40, "--manifest-sha256", "b" * 64,
                "--expected-dir", str(root / "expected"), "--actual-dir", str(root / "actual"),
                "--output", str(output),
            ]
            for status, exit_code in (("success", 0), ("failure", 1)):
                checks = [{"check": "release", "asset": None, "status": status}]
                with self.subTest(status=status), patch("sys.argv", arguments), \
                     patch("release_post_publish_checks.verify", return_value=checks), \
                     contextlib.redirect_stdout(io.StringIO()):
                    self.assertEqual(post_publish_main(), exit_code)
                self.assertEqual(json.loads(output.read_text(encoding="utf-8")), checks)

    def test_published_checks_cover_all_assets_and_missing_file(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            expected = root / "expected"
            actual = root / "actual"
            expected.mkdir()
            actual.mkdir()
            for name in expected_assets("1.2.3"):
                (expected / name).write_bytes(name.encode())
                (actual / name).write_bytes(name.encode())

            commands = []

            def runner(command, **_kwargs):
                commands.append(command)
                return type("Result", (), {"returncode": 0, "stderr": "", "stdout": ""})()

            def fetcher(request, timeout):
                self.assertEqual(timeout, 30)
                return io.BytesIO((expected / Path(urlsplit(request.full_url).path).name).read_bytes())

            manifest = hashlib.sha256((expected / "SHA256SUMS.txt").read_bytes()).hexdigest()
            checks = verify("1.2.3", "owner/repo", "v1.2.3", "a" * 40,
                            manifest, expected, actual, runner, fetcher)
            self.assertEqual(required_checks("1.2.3"),
                             {(item["check"], item["asset"]) for item in checks})
            self.assertTrue(all(item["status"] == "success" for item in checks))
            attestations = [command for command in commands if command[:3] == ["gh", "attestation", "verify"]]
            self.assertEqual(len(attestations), 5)
            self.assertTrue(all(command[command.index("--source-ref") + 1] == "refs/heads/main"
                                for command in attestations))
            (actual / "install-work-from-github-release.command").unlink()
            checks = verify("1.2.3", "owner/repo", "v1.2.3", "a" * 40,
                            manifest, expected, actual, runner, fetcher)
            self.assertTrue(any(item["check"] == "asset_digest" and
                                item["asset"] == "install-work-from-github-release.command" and
                                item["status"] == "failure" for item in checks))

    def test_attestation_failure_records_asset_and_error(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            expected = root / "expected"
            actual = root / "actual"
            expected.mkdir()
            actual.mkdir()
            for name in expected_assets("1.2.3"):
                (expected / name).write_bytes(name.encode())
                (actual / name).write_bytes(name.encode())

            def runner(command, **_kwargs):
                failed = command[:3] == ["gh", "attestation", "verify"]
                return type("Result", (), {"returncode": 1 if failed else 0,
                                             "stderr": "signature rejected" if failed else "", "stdout": ""})()

            def fetcher(request, timeout):
                self.assertEqual(timeout, 30)
                return io.BytesIO((expected / Path(urlsplit(request.full_url).path).name).read_bytes())

            checks = verify("1.2.3", "owner/repo", "v1.2.3", "a" * 40,
                            "bad-digest", expected, actual, runner, fetcher)
            failed = [item for item in checks if item["status"] == "failure"]
            self.assertTrue(any(item["check"] == "attestation" and item["asset"]
                                and item["error"] == "signature rejected" for item in failed))
            self.assertTrue(any(item["check"] == "signed_manifest_digest" and
                                item["expected_sha256"] == "bad-digest" for item in failed))
            stages = {name: "success" for name in (
                "validate", "macos_e2e", "windows_e2e", "attest", "publish", "post_publish_verify"
            )}
            report = build_report("1.2.3", "a" * 40, "https://example.invalid/run",
                                  stages, expected, actual, checks)
            self.assertEqual(report["result"], "failure")
            self.assertIn("signature rejected", "\n".join(report["failure_reasons"]))
            self.assertFalse(report["attestation_verified"])

    def test_downloader_urls_record_fixed_and_latest_failures(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            expected = root / "expected"
            actual = root / "actual"
            expected.mkdir()
            actual.mkdir()
            for name in expected_assets("1.2.3"):
                (expected / name).write_bytes(name.encode())
                (actual / name).write_bytes(name.encode())

            def runner(command, **_kwargs):
                return type("Result", (), {"returncode": 0, "stderr": "", "stdout": ""})()

            def fetcher(request, timeout):
                if "/latest/" in request.full_url:
                    raise OSError("latest URL unavailable")
                return io.BytesIO(b"wrong bytes")

            checks = verify("1.2.3", "owner/repo", "v1.2.3", "a" * 40,
                            hashlib.sha256((expected / "SHA256SUMS.txt").read_bytes()).hexdigest(),
                            expected, actual, runner, fetcher)
            url_checks = [item for item in checks if item["check"].startswith("download_url_")]
            self.assertEqual(len(url_checks), 4)
            self.assertTrue(all(item["status"] == "failure" for item in url_checks))
            self.assertTrue(any("latest URL unavailable" in item["error"] for item in url_checks))
            self.assertTrue(any("SHA-256 differs" in item["error"] for item in url_checks))

    def test_failed_verification_still_writes_json_and_summary(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            report_path = root / "release-verification.json"
            checks_path = root / "release-checks.json"
            checks_path.write_text(json.dumps([{
                "check": "attestation", "asset": "work-v1.2.3-macos-arm64.zip",
                "status": "failure", "error": "signature rejected",
            }]), encoding="utf-8")
            summary_path = root / "summary.md"
            arguments = [
                "release_verification_report.py", "--version", "1.2.3", "--commit", "a" * 40,
                "--run-url", "https://example.invalid/run", "--expected-dir", str(root / "expected"),
                "--actual-dir", str(root / "actual"), "--checks", str(checks_path),
                "--output", str(report_path),
            ]
            for stage in ("validate", "macos-e2e", "windows-e2e", "attest", "publish", "post-publish-verify"):
                arguments.extend((f"--{stage}-status", "failure" if stage == "post-publish-verify" else "success"))
            with patch("sys.argv", arguments), patch.dict("os.environ", {"GITHUB_STEP_SUMMARY": str(summary_path)}), \
                 contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(main(), 1)
            report = json.loads(report_path.read_text(encoding="utf-8"))
            self.assertEqual(report["result"], "failure")
            self.assertEqual(report["stages"]["post_publish_verify"], "failure")
            summary = summary_path.read_text(encoding="utf-8")
            self.assertIn("Release verification: failure", summary)
            self.assertIn("signature rejected", summary)
            self.assertEqual(report["checks"][0]["asset"], "work-v1.2.3-macos-arm64.zip")

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
            checks = [{"check": check, "asset": asset, "status": "success"}
                      for check, asset in required_checks("1.2.3")]
            report = build_report("1.2.3", "a" * 40, "https://example.invalid/run", stages, expected, actual, checks)
            self.assertEqual(report["result"], "success")
            self.assertTrue(report["attestation_verified"])
            missing = [item for item in checks if item["check"] != "download_url_latest"]
            report = build_report("1.2.3", "a" * 40, "https://example.invalid/run", stages, expected, actual, missing)
            self.assertEqual(report["result"], "failure")
            self.assertTrue(any("download_url_latest" in reason for reason in report["failure_reasons"]))
            missing = [item for item in checks if item["check"] != "attestation"]
            report = build_report("1.2.3", "a" * 40, "https://example.invalid/run", stages, expected, actual, missing)
            self.assertEqual(report["result"], "failure")
            self.assertFalse(report["attestation_verified"])
            (actual / "SHA256SUMS.txt").write_bytes(b"changed")
            stages["post_publish_verify"] = "failure"
            report = build_report("1.2.3", "a" * 40, "https://example.invalid/run", stages, expected, actual, checks)
            self.assertEqual(report["result"], "failure")
            self.assertFalse(report["attestation_verified"])
            self.assertTrue(report["failure_reasons"])
            changed = next(item for item in report["assets"] if item["name"] == "SHA256SUMS.txt")
            self.assertNotEqual(changed["expected_sha256"], changed["actual_sha256"])
            self.assertIn("expected=", "\n".join(report["failure_reasons"]))


if __name__ == "__main__":
    unittest.main()
