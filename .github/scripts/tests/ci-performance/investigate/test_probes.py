"""Verify independent fixture probe samples."""

import importlib.util
from pathlib import Path
import tempfile
import unittest


MODULE_PATH = Path(__file__).resolve().parents[3] / "ci-performance/investigate/probes.py"
SPEC = importlib.util.spec_from_file_location("ci_perf_probes", MODULE_PATH)
PROBES = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PROBES)


class ProbeTests(unittest.TestCase):
    def test_fixture_probe_uses_independent_sample_paths(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            result = PROBES.probe_operation("sample", 2, lambda index: (root / str(index)).write_text("x"))
            self.assertEqual(len(result["samples_seconds"]), 2)
            self.assertEqual({path.name for path in root.iterdir()}, {"0", "1"})


if __name__ == "__main__":
    unittest.main()
