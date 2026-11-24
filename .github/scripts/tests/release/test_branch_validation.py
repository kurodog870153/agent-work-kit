"""Regression checks for manual validation and nested GitHub instructions."""
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[4]
WORKFLOWS = ROOT / ".github/workflows"
INSTRUCTIONS = ROOT / "ai-maintainer-instructions"


def parse_yaml(text):
    # Use the same Ruby YAML parser as the workflow allowlist when available.
    if shutil.which("ruby"):
        result = subprocess.run(
            ["ruby", "-e", 'require "yaml"; require "json"; puts JSON.generate(YAML.safe_load(STDIN.read, aliases: true))'],
            input=text, text=True, encoding="utf-8", capture_output=True, check=True)
        return json.loads(result.stdout)
    import yaml
    return yaml.safe_load(text)


def workflow(name):
    return parse_yaml((WORKFLOWS / name).read_text(encoding="utf-8"))


def triggers(data):
    # YAML 1.1 loaders may interpret the unquoted "on" key as true.
    return data.get("on", data.get(True, data.get("true")))


class BranchValidationTests(unittest.TestCase):
    def setUp(self):
        self.branch = workflow("branch-validation.yml")
        self.pr = workflow("pr-release-validation.yml")

    def test_manual_only_and_concurrency(self):
        self.assertEqual("branch validation", self.branch["name"])
        self.assertEqual("branch validation: ${{ github.ref_name }}", self.branch["run-name"])
        self.assertEqual({"workflow_dispatch": None}, triggers(self.branch))
        self.assertEqual({"pull_request": None}, triggers(self.pr))
        self.assertEqual(
            {"group": "branch-validation-${{ github.ref }}", "cancel-in-progress": True},
            self.branch["concurrency"])
        self.assertEqual({"contents": "read", "actions": "read"}, self.branch["permissions"])
        self.assertFalse((WORKFLOWS / "issue-134-branch-validation.yml").exists())

    def test_pr_and_release_concurrency(self):
        credentials = workflow("pr-release-credentials-validation.yml")
        release = workflow("publish-release.yml")
        self.assertEqual(
            {"group": "pr-release-validation-${{ github.event.pull_request.number }}",
             "cancel-in-progress": True}, self.pr["concurrency"])
        self.assertEqual(
            {"group": "pr-release-credentials-validation-${{ github.event.pull_request.number }}",
             "cancel-in-progress": True}, credentials["concurrency"])
        self.assertEqual(
            {"group": "agent-work-kit-release-${{ github.ref }}",
             "cancel-in-progress": False}, release["concurrency"])
        groups = [data["concurrency"]["group"]
                  for data in (self.branch, self.pr, credentials, release)]
        self.assertEqual(4, len(set(groups)))
        self.assertEqual(
            {"pull_request_target": {"types": ["opened", "synchronize", "reopened",
                                               "ready_for_review"]}}, triggers(credentials))
        self.assertEqual({"push": {"branches": ["main"]}}, triggers(release))
        self.assertEqual({}, credentials["permissions"])

    def test_report_display_names(self):
        for data in (self.branch, self.pr, workflow("publish-release.yml")):
            self.assertEqual("rust validation report",
                             data["jobs"]["rust-validation-report"]["name"])
        action = ROOT / ".github/actions/rust-validation-report/action.yml"
        self.assertEqual("rust validation report",
                         parse_yaml(action.read_text(encoding="utf-8"))["name"])

    def test_shared_checks_and_job_dependencies(self):
        jobs = self.branch["jobs"]
        pr_jobs = self.pr["jobs"]
        preflight = "./.github/actions/validation-preflight"
        self.assertEqual(preflight, jobs["preflight"]["steps"][-1]["uses"])
        self.assertTrue(any(step.get("uses") == preflight
                            for step in pr_jobs["validate-release-preflight"]["steps"]))
        for name in ("rust-ci", "package-ci"):
            self.assertEqual(pr_jobs[name]["uses"], jobs[name]["uses"])
            self.assertEqual("preflight", jobs[name]["needs"])
        self.assertEqual("validate-release-preflight", pr_jobs["rust-ci"]["needs"])
        self.assertEqual("rust-ci", pr_jobs["package-ci"]["needs"])
        self.assertEqual({"rust-ci", "package-ci"}, set(jobs["rust-validation-report"]["needs"]))
        self.assertEqual(pr_jobs["rust-validation-report"]["steps"],
                         jobs["rust-validation-report"]["steps"])
        confirm = jobs["confirm-branch-validation"]
        self.assertEqual("always()", confirm["if"])
        self.assertEqual("rust-validation-report", confirm["needs"])
        self.assertEqual(pr_jobs["confirm-release-validation"]["steps"], confirm["steps"])
        # The default success condition propagates upstream failure or skip to the report.
        for name in ("rust-ci", "package-ci", "rust-validation-report"):
            self.assertEqual("success()", jobs[name].get("if", "success()"))
        for name in ("preflight", "rust-ci", "package-ci", "rust-validation-report"):
            self.assertFalse(jobs[name].get("continue-on-error", False))
        shared = ROOT / ".github/actions/validation-preflight/action.yml"
        steps = parse_yaml(shared.read_text(encoding="utf-8"))["runs"]["steps"]
        commands = "\n".join(step.get("run", "") for step in steps)
        self.assertIn("check_workflow_actions.py", commands)
        self.assertIn("unittest discover", commands)
        self.assertIn("YAML.parse_file", commands)
        branch_text = (WORKFLOWS / "branch-validation.yml").read_text(encoding="utf-8")
        for forbidden in ("version_guard.py", "release_version_availability.py",
                          "release_content.py", "publish-release.yml", "secrets."):
            self.assertNotIn(forbidden, branch_text + commands)

    @unittest.skipUnless(shutil.which("bash"), "bash required for Workflow result gate")
    def test_gate_rejects_incomplete_report(self):
        step = self.branch["jobs"]["confirm-branch-validation"]["steps"][0]
        env = {**os.environ, **dict.fromkeys(step["env"], "success")}
        def run(values):
            return subprocess.run(["bash", "-c", step["run"]], env=values, capture_output=True).returncode
        self.assertEqual(0, run(env))
        for name in step["env"]:
            for status in ("failure", "cancelled", "skipped", ""):
                with self.subTest(job=name, status=status):
                    self.assertNotEqual(0, run({**env, name: status}))


class GitHubInstructionsTests(unittest.TestCase):
    def test_nested_discovery_and_frontmatter(self):
        self.assertFalse((INSTRUCTIONS / "issues").exists())
        github = INSTRUCTIONS / "github"
        self.assertEqual({"actions", "issues"}, {path.name for path in github.iterdir() if path.is_dir()})
        readme = (INSTRUCTIONS / "README.md").read_text(encoding="utf-8")
        self.assertIn("[github/](github/)", readme)
        self.assertNotIn("[issues/](issues/)", readme)
        candidates = {}
        # Test the documented recursive discovery; do not add a runtime loader.
        for path in github.rglob("*.md"):
            source = path.read_text(encoding="utf-8")
            self.assertTrue(source.startswith("---\n"), path)
            _, frontmatter, body = source.split("---", 2)
            metadata = parse_yaml(frontmatter)
            self.assertEqual({"name", "description", "metadata"}, set(metadata))
            self.assertTrue(metadata["name"].strip())
            self.assertTrue(metadata["description"].strip())
            self.assertEqual({"tags"}, set(metadata["metadata"]))
            tags = metadata["metadata"]["tags"]
            self.assertTrue(tags)
            self.assertEqual(len(tags), len(set(tags)))
            for tag in tags:
                self.assertRegex(tag, r"^[a-z0-9]+(?:-[a-z0-9]+)*$")
            self.assertEqual(1, len(re.findall(r"^# .+$", body, re.MULTILINE)))
            candidates[path.relative_to(github).as_posix()] = metadata
        self.assertEqual({"actions/validation.md", "issues/github-issue-planning.md"}, set(candidates))
        # Representative applicability cases: CI log/dispatch versus issue task planning.
        self.assertIn("Workflow", candidates["actions/validation.md"]["description"])
        self.assertIn("GitHub Issue", candidates["issues/github-issue-planning.md"]["description"])
        self.assertIn("branch-validation", candidates["actions/validation.md"]["metadata"]["tags"])
        self.assertIn("task-planning", candidates["issues/github-issue-planning.md"]["metadata"]["tags"])


if __name__ == "__main__":
    unittest.main()
