import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "ci"))
from package_validation_results import run


class PackageValidationResultsTests(unittest.TestCase):
    def test_records_actual_success_and_failure_once(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for code in (0, 7):
                with patch.dict("os.environ", {"GITHUB_SHA": "abc", "GITHUB_RUN_ID": "1", "GITHUB_RUN_ATTEMPT": "2"}):
                    self.assertEqual(run(root, "package", [sys.executable, "-c", f"raise SystemExit({code})"]), code)
                result = json.loads((root / "package.json").read_text(encoding="utf-8"))
                self.assertEqual(result["exit_code"], code)
                self.assertEqual(result["commit"], "abc")
                self.assertEqual(result["run_attempt"], "2")
                self.assertGreaterEqual(result["duration_seconds"], 0)

    def test_workflow_cli_preserves_child_exit_code_and_evidence(self):
        script = Path(__file__).resolve().parents[2] / "ci/package_validation_results.py"
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for code in (0, 7):
                with self.subTest(code=code):
                    completed = subprocess.run([sys.executable, "-B", str(script), "run",
                                                "--directory", directory, "--phase", f"package-{code}", "--",
                                                sys.executable, "-c", f"raise SystemExit({code})"],
                                               capture_output=True, text=True)
                    self.assertEqual(completed.returncode, code, completed.stderr)
                    record = json.loads((root / f"package-{code}.json").read_text(encoding="utf-8"))
                    self.assertEqual(record["exit_code"], code)
