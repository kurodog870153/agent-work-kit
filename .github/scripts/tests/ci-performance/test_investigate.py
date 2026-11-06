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
                "cargo", [sys.executable, "-c", "print('test result: ok. 1 passed; 0 failed;\\nAWK_TEST_TIMING git_init 2000\\ntest result: ok. 2 passed; 0 failed;')"],
                output, output, "cargo",
            )
            nextest = INVESTIGATE.timed_command(
                "nextest", [sys.executable, "-c", "print('test result: ok. 1 passed; 0 failed;\\nPASS [  0.002s] test_one\\nPASS [  0.003s] test_two\\nSummary [  0.010s] 2 tests run: 2 passed')"],
                output, output, "nextest",
            )
            self.assertEqual(cargo["test_count"], 2)
            self.assertEqual(cargo["internal_timings"]["git_init"]["sum_seconds"], 0.002)
            self.assertEqual(nextest["test_count"], 2)
            self.assertEqual(nextest["individual_tests"]["count"], 2)
            self.assertEqual(nextest["individual_tests"]["sum_seconds"], 0.005)
            self.assertIn("AWK_TEST_TIMING", (output / "cargo.log").read_text(encoding="utf-8"))

    def test_nextest_requires_its_own_summary(self):
        self.assertIsNone(
            INVESTIGATE.test_count_from_log("test result: ok. 1 passed; 0 failed;\n", "nextest")
        )

    def test_nextest_single_test_summary(self):
        log = "PASS [  0.002s] work-infrastructure::lib one_test\nSummary [  0.010s] 1 test run: 1 passed\n"
        self.assertEqual(INVESTIGATE.test_count_from_log(log, "nextest"), 1)


if __name__ == "__main__":
    unittest.main()
