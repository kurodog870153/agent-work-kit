"""Verify that investigation logs retain comparable test evidence."""

import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest


MODULE_PATH = Path(__file__).resolve().parents[2] / "ci-performance/investigate.py"
SPEC = importlib.util.spec_from_file_location("ci_perf_investigate", MODULE_PATH)
INVESTIGATE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(INVESTIGATE)


class InvestigationLogTests(unittest.TestCase):
    def test_cargo_and_nextest_counts_and_internal_steps(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            cargo = INVESTIGATE.timed_command(
                "cargo", [sys.executable, "-c", "print('AWK_TEST_TIMING git_init 2000\\ntest result: ok. 2 passed; 0 failed;')"],
                output, output,
            )
            nextest = INVESTIGATE.timed_command(
                "nextest", [sys.executable, "-c", "print('PASS [  0.002s] test_one\\nPASS [  0.003s] test_two\\nSummary [  0.010s] 2 tests run: 2 passed')"],
                output, output,
            )
            self.assertEqual(cargo["test_count"], 2)
            self.assertEqual(cargo["internal_timings"]["git_init"]["sum_seconds"], 0.002)
            self.assertEqual(nextest["test_count"], 2)
            self.assertEqual(nextest["individual_tests"]["sum_seconds"], 0.005)
            self.assertIn("AWK_TEST_TIMING", (output / "cargo.log").read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()
