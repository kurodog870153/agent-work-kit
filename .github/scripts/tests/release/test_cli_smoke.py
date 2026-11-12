"""Checks for the installed Work CLI smoke script and its evidence."""

import hashlib
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


SCRIPT = Path(__file__).resolve().parents[2] / "release/cli_smoke.py"
sys.path.insert(0, str(SCRIPT.parent))
from release_install_e2e import run_installed_cli_smoke


class CliSmokeTests(unittest.TestCase):
    def test_installed_cli_smoke_uses_installed_binary_and_records_zip_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / "skills/work/scripts/work.exe"
            binary.parent.mkdir(parents=True)
            binary.write_bytes(b"installed work")
            zip_path = root / "agent-work-kit-v2.2.0-codex-windows-x64-msvc.zip"
            zip_path.write_bytes(b"release zip bytes")
            evidence = root / "evidence"

            def complete_smoke(command, **kwargs):
                self.assertEqual(command[-1], str(SCRIPT))
                self.assertEqual(kwargs["env"]["WORK_BINARY"], str(binary))
                report = Path(kwargs["env"]["SCENARIO_REPORT"])
                self.assertEqual(report, evidence / "codex-windows-x64-msvc.md")
                report.write_text("$ work task validate\n# exit 0\n", encoding="utf-8")
                return subprocess.CompletedProcess(command, 0, "", "")

            with patch("release_install_e2e.subprocess.run", side_effect=complete_smoke) as run:
                run_installed_cli_smoke(binary, evidence, zip_path, "2.2.0", "codex", "windows-x64-msvc")
            self.assertEqual(run.call_count, 1)
            report = (evidence / "codex-windows-x64-msvc.md").read_text(encoding="utf-8")
            self.assertIn(zip_path.name, report)
            self.assertIn(hashlib.sha256(zip_path.read_bytes()).hexdigest(), report)
            self.assertIn("Release version: `2.2.0`", report)
            self.assertIn("$ work task validate\n# exit 0", report)

    def test_failed_public_contract_query_stops_without_success_report(self):
        with tempfile.TemporaryDirectory() as directory:
            report = Path(directory) / "cli-smoke.md"
            environment = os.environ.copy()
            # Python accepts the process launch but rejects Work CLI arguments.
            environment["WORK_BINARY"] = sys.executable
            environment["SCENARIO_REPORT"] = str(report)
            result = subprocess.run([sys.executable, "-B", str(SCRIPT)],
                                    capture_output=True, text=True, env=environment)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("AssertionError", result.stderr)
            self.assertFalse(report.exists())


if __name__ == "__main__":
    unittest.main()
