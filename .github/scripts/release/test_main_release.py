"""Unit tests for the main release CI gate."""

import unittest
from unittest.mock import patch

from main_release import main, successful_runs, wait_for_successful_runs


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

    def test_duplicate_notification_skips_tag_at_same_commit(self):
        sha = "c" * 40
        tag_ref = {"object": {"type": "commit", "sha": sha}}
        with patch.dict("main_release.os.environ", {"GITHUB_REPOSITORY": "owner/repo", "WORKFLOW_HEAD_SHA": sha}), \
             patch("main_release.main_head", return_value=sha), \
             patch("main_release.wait_for_successful_runs", return_value=True), \
             patch("main_release.checked_version", return_value="1.0.1"), \
             patch("main_release.tag_exists", return_value=True), \
             patch("main_release.gh_api", return_value=tag_ref), \
             patch("main_release.subprocess.run") as run:
            run.return_value.stdout = sha
            self.assertEqual(main(), 0)
            run.assert_called_once()

    def test_waits_for_notifying_workflow_to_finish(self):
        with patch("main_release.successful_runs", side_effect=[False, True]) as check, \
             patch("main_release.time.sleep") as sleep:
            self.assertTrue(wait_for_successful_runs("owner/repo", "d" * 40))
            self.assertEqual(check.call_count, 2)
            sleep.assert_called_once_with(5)


if __name__ == "__main__":
    unittest.main()
