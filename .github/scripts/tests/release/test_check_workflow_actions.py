"""Tests for the workflow action allowlist."""

import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "release"))
from check_workflow_actions import APPROVED, check_workflow


class WorkflowActionTests(unittest.TestCase):
    def test_approved_actions(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "workflow.yml"
            path.write_text("jobs:\n  build:\n    steps:\n" +
                            "\n".join(f"      - uses: {action}" for action in sorted(APPROVED)),
                            encoding="utf-8")
            self.assertEqual(check_workflow(path), [])

    def test_rejects_source_version_and_nonliteral_uses(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "workflow.yml"
            path.write_text("\n".join((
                "jobs:",
                "  reusable:",
                "    uses: owner/workflow@v1",
                "  build:",
                "    steps:",
                "      - uses: actions/checkout@v4",
                "      - { uses: owner/unapproved-action@v1 }",
                "      - uses: ./local-action",
                "      - uses: docker://alpine:latest",
                "      - uses: ${{ matrix.action }}",
                "      - 'uses': actions/checkout@v4",
                "      - uses: 'actions/checkout@v5'",
            )), encoding="utf-8")
            errors = check_workflow(path)
            self.assertEqual(len(errors), 8)
            self.assertTrue(any("owner/unapproved-action@v1" in error for error in errors))
            self.assertTrue(any("actions/checkout@v5" in error for error in errors))

    def test_approved_flow_mapping(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "workflow.yml"
            path.write_text("jobs: { build: { steps: [ { uses: actions/checkout@v7 } ] } }\n",
                            encoding="utf-8")
            self.assertEqual(check_workflow(path), [])

    def test_non_action_uses_key_is_ignored(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "workflow.yml"
            path.write_text("jobs: { build: { env: { uses: owner/value@v1 }, steps: [] } }\n",
                            encoding="utf-8")
            self.assertEqual(check_workflow(path), [])

    def test_alias_and_merge_cannot_hide_action(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "workflow.yml"
            path.write_text("shared: &action { uses: owner/unapproved-action@v1 }\n"
                            "jobs: { build: { steps: [ { <<: *action } ] } }\n", encoding="utf-8")
            errors = check_workflow(path)
            self.assertEqual(len(errors), 1)
            self.assertIn("owner/unapproved-action@v1", errors[0])

    def test_non_scalar_uses_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "workflow.yml"
            path.write_text("jobs: { build: { steps: [ { uses: [actions/checkout@v7] } ] } }\n",
                            encoding="utf-8")
            self.assertEqual(len(check_workflow(path)), 1)


if __name__ == "__main__":
    unittest.main()
