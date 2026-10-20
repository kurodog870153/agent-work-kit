"""Unit tests for the main release CI gate."""

import subprocess
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "release"))
from main_release_coordinator import main, successful_runs, wait_for_successful_runs


class MainReleaseTests(unittest.TestCase):
    def test_requires_both_workflows_for_exact_push_commit(self):
        sha = "a" * 40
        good = {"head_sha": sha, "event": "push", "run_number": 1, "run_attempt": 1, "status": "completed", "conclusion": "success"}
        with patch("main_release_coordinator.gh_api", side_effect=[{"workflow_runs": [good]}, {"workflow_runs": [good]}]):
            self.assertTrue(successful_runs("owner/repo", sha))
        with patch("main_release_coordinator.gh_api", side_effect=[{"workflow_runs": [good]}, {"workflow_runs": []}]):
            self.assertFalse(successful_runs("owner/repo", sha))

    def test_latest_failed_attempt_blocks_tag(self):
        sha = "b" * 40
        good = {"head_sha": sha, "event": "push", "run_number": 1, "run_attempt": 1, "status": "completed", "conclusion": "success"}
        failed = dict(good, run_attempt=2, conclusion="failure")
        with patch("main_release_coordinator.gh_api", return_value={"workflow_runs": [good, failed]}):
            self.assertFalse(successful_runs("owner/repo", sha))

    def test_duplicate_notification_skips_tag_at_same_commit(self):
        sha = "c" * 40
        tag_ref = {"object": {"type": "commit", "sha": sha}}
        with patch.dict("main_release_coordinator.os.environ", {"GITHUB_REPOSITORY": "owner/repo", "WORKFLOW_HEAD_SHA": sha}), \
             patch("main_release_coordinator.main_head", return_value=sha), \
             patch("main_release_coordinator.wait_for_successful_runs", return_value=True), \
             patch("main_release_coordinator.checked_version", return_value="1.0.1"), \
             patch("main_release_coordinator.tag_exists", return_value=True), \
             patch("main_release_coordinator.gh_api", return_value=tag_ref), \
             patch("main_release_coordinator.subprocess.run") as run:
            run.return_value.stdout = sha
            self.assertEqual(main(), 0)
            run.assert_called_once()

    def test_waits_for_notifying_workflow_to_finish(self):
        with patch("main_release_coordinator.successful_runs", side_effect=[False, True]) as check, \
             patch("main_release_coordinator.time.sleep") as sleep:
            self.assertTrue(wait_for_successful_runs("owner/repo", "d" * 40))
            self.assertEqual(check.call_count, 2)
            sleep.assert_called_once_with(5)

    def test_dispatch_failure_preserves_new_tag_and_does_not_retry(self):
        sha = "e" * 40
        checkout = subprocess.CompletedProcess(["git", "rev-parse", "HEAD"], 0, stdout=sha)
        failed_dispatch = subprocess.CalledProcessError(1, ["gh", "workflow", "run"])
        with patch.dict("main_release_coordinator.os.environ", {"GITHUB_REPOSITORY": "owner/repo", "WORKFLOW_HEAD_SHA": sha}), \
             patch("main_release_coordinator.main_head", return_value=sha), \
             patch("main_release_coordinator.wait_for_successful_runs", return_value=True), \
             patch("main_release_coordinator.successful_runs", return_value=True), \
             patch("main_release_coordinator.checked_version", return_value="1.0.4"), \
             patch("main_release_coordinator.tag_exists", return_value=False), \
             patch("main_release_coordinator.release_exists", return_value=False), \
             patch("main_release_coordinator.gh_api") as api, \
             patch("main_release_coordinator.subprocess.run", side_effect=[checkout, failed_dispatch]):
            with self.assertRaises(subprocess.CalledProcessError):
                main()
            api.assert_called_once_with(
                "repos/owner/repo/git/refs", method="POST",
                fields=("ref=refs/tags/v1.0.4", f"sha={sha}"),
            )

    def test_tag_creation_failure_never_dispatches_publish(self):
        sha = "f" * 40
        checkout = subprocess.CompletedProcess(["git", "rev-parse", "HEAD"], 0, stdout=sha)
        failed_tag = subprocess.CalledProcessError(1, ["gh", "api", "--method", "POST"])
        with patch.dict("main_release_coordinator.os.environ", {"GITHUB_REPOSITORY": "owner/repo", "WORKFLOW_HEAD_SHA": sha}), \
             patch("main_release_coordinator.main_head", return_value=sha), \
             patch("main_release_coordinator.wait_for_successful_runs", return_value=True), \
             patch("main_release_coordinator.successful_runs", return_value=True), \
             patch("main_release_coordinator.checked_version", return_value="1.0.4"), \
             patch("main_release_coordinator.tag_exists", return_value=False), \
             patch("main_release_coordinator.release_exists", return_value=False), \
             patch("main_release_coordinator.gh_api", side_effect=failed_tag) as api, \
             patch("main_release_coordinator.subprocess.run", return_value=checkout) as run:
            with self.assertRaises(subprocess.CalledProcessError):
                main()
            api.assert_called_once()
            run.assert_called_once()


if __name__ == "__main__":
    unittest.main()
