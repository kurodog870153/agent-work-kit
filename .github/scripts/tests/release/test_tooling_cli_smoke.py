"""Checks for installed Tooling CLI public smoke behavior."""

import json
import runpy
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[2] / "release/tooling_cli_smoke.py"


class ToolingCliSmokeTests(unittest.TestCase):
    def test_import_does_not_run_smoke(self):
        namespace = runpy.run_path(str(SCRIPT), run_name="tooling_cli_smoke_import_test")
        self.assertTrue(callable(namespace["run_tooling_public_smoke"]))

    def test_tooling_public_smoke_uses_installed_skill_and_embedded_examples(self):
        repository = Path(__file__).resolve().parents[4]
        binary = repository / "rust/target/debug" / ("tooling.exe" if sys.platform == "win32" else "tooling")
        if not binary.is_file():
            self.skipTest("tooling debug binary has not been built")
        with tempfile.TemporaryDirectory(dir=repository) as directory:
            skill = Path(directory) / "tooling"
            shutil.copytree(repository / "skills/tooling", skill)
            (skill / "scripts").mkdir()
            shutil.copy2(binary, skill / "scripts" / binary.name)
            result = subprocess.run([sys.executable, "-B", str(SCRIPT), "--skill-root", str(skill)],
                                    capture_output=True, text=True, timeout=120)
            self.assertEqual(result.returncode, 0, result.stderr)
            evidence = json.loads(result.stdout)
            self.assertGreater(evidence["first_sources"], 0)
            self.assertGreater(evidence["changed_sources"], 0)
            self.assertEqual(evidence["target_boundary"], "target_valid")
            self.assertTrue(evidence["incompatible_policy_blocked"])


if __name__ == "__main__":
    unittest.main()
