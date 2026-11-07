"""Verify Cargo timing evidence from the reusable compile phase."""

import contextlib
import importlib.util
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch


MODULE_PATH = Path(__file__).resolve().parents[3] / "ci-performance/investigate/compile.py"
SPEC = importlib.util.spec_from_file_location("ci_perf_compile", MODULE_PATH)
COMPILE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(COMPILE)


class CompileInvestigationTests(unittest.TestCase):
    def test_preserves_existing_cargo_timing(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "target/cargo-timings").mkdir(parents=True)
            (root / "target/cargo-timings/cargo-timing.html").write_text("report", encoding="utf-8")
            (root / "evidence").mkdir()
            (root / "evidence/compile.json").write_text(
                json.dumps({"phase": "compile", "command": ["cargo", "test", "--timings"],
                            "duration_seconds": 12.3, "exit_code": 0}), encoding="utf-8"
            )
            with contextlib.chdir(root), patch.object(COMPILE.subprocess, "run", return_value=SimpleNamespace(stdout="rustc test")):
                self.assertEqual(COMPILE.run(SimpleNamespace(directory="evidence", target="")), 0)
            self.assertEqual((root / "evidence/compile-timings.html").read_text(encoding="utf-8"), "report")
            result = json.loads((root / "evidence/compile-investigation.json").read_text(encoding="utf-8"))
            self.assertEqual(result["compile_seconds"], 12.3)
            self.assertTrue(result["cargo_timings_available"])


if __name__ == "__main__":
    unittest.main()
