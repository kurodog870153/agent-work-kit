"""Unit tests for the main release CI gate."""

import unittest
from unittest.mock import patch

from main_release import successful_runs


class MainReleaseTests(unittest.TestCase):
    def test_requires_both_workflows_for_exact_push_commit(self):
        sha = "a" * 40
        good = {"head_sha": sha, "event": "push", "run_number": 1, "run_attempt": 1, "status": "completed", "conclusion": "success"}
        with patch("main_release.gh_api", side_effect=[{"workflow_runs": [good]}, {"workflow_runs": [good]}]):
            self.assertTrue(successful_runs("owner/repo", sha))
        with patch("main_release.gh_api", side_effect=[{"workflow_runs": [good]}, {"workflow_runs": []}]):
            self.assertFalse(successful_runs("owner/repo", sha))

    def test_latest_failed_attempt_blocks_tag(self):
        sha = "b" * 40
        good = {"head_sha": sha, "event": "push", "run_number": 1, "run_attempt": 1, "status": "completed", "conclusion": "success"}
        failed = dict(good, run_attempt=2, conclusion="failure")
        with patch("main_release.gh_api", return_value={"workflow_runs": [good, failed]}):
            self.assertFalse(successful_runs("owner/repo", sha))


if __name__ == "__main__":
    unittest.main()
