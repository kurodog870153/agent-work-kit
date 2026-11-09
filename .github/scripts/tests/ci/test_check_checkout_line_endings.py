"""Tests for the shared checkout line ending check."""

import subprocess
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "ci"))
from check_checkout_line_endings import CRLF_PATHS, LF_PATHS, check_line_endings


class CheckoutLineEndingTests(unittest.TestCase):
    @patch("check_checkout_line_endings.subprocess.run")
    def test_checks_every_path_with_its_expected_line_ending(self, run):
        def result(command, **_kwargs):
            path = command[-1]
            ending = "crlf" if path in CRLF_PATHS else "lf"
            return subprocess.CompletedProcess(command, 0, f"i/lf    w/{ending}    attr/text\t{path}\n", "")

        run.side_effect = result
        check_line_endings()
        self.assertEqual([call.args[0][-1] for call in run.call_args_list], list(LF_PATHS + CRLF_PATHS))

    @patch("check_checkout_line_endings.subprocess.run")
    def test_rejects_missing_or_wrong_line_endings(self, run):
        for output in ("", "i/lf    w/crlf    attr/text\tREADME.md\n"):
            with self.subTest(output=output):
                run.return_value = subprocess.CompletedProcess([], 0, output, "")
                with self.assertRaisesRegex(ValueError, "README.md"):
                    check_line_endings()


if __name__ == "__main__":
    unittest.main()
