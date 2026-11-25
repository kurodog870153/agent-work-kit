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
            self.assertEqual("rust validation evidence",
                             data["jobs"]["rust-validation-evidence"]["name"])
        action = ROOT / ".github/actions/rust-validation-evidence/action.yml"
        self.assertEqual("rust validation evidence",
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
        self.assertEqual({"rust-ci", "package-ci"}, set(jobs["rust-validation-evidence"]["needs"]))
        self.assertEqual("package-ci", pr_jobs["rust-validation-evidence"]["needs"])
        for platform in ("macos", "windows"):
            key = f"{platform}-rust-attempt"
            self.assertEqual(pr_jobs["package-ci"]["with"][key],
                             f"${{{{ needs.rust-ci.outputs.{key} }}}}")
            self.assertEqual(pr_jobs["rust-validation-evidence"]["steps"][-1]["with"][key],
                             f"${{{{ needs.package-ci.outputs.{key} }}}}")
        confirm = jobs["confirm-branch-validation"]
        self.assertEqual("always()", confirm["if"])
        self.assertEqual("rust-validation-evidence", confirm["needs"])
        self.assertEqual(pr_jobs["confirm-release-validation"]["steps"], confirm["steps"])
        # The default success condition propagates upstream failure or skip to the report.
        for name in ("rust-ci", "package-ci", "rust-validation-evidence"):
            self.assertEqual("success()", jobs[name].get("if", "success()"))
        for name in ("preflight", "rust-ci", "package-ci", "rust-validation-evidence"):
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

    def test_direct_dependency_topologies_and_forwarded_attempts(self):
        release = workflow("publish-release.yml")
        expected = {
            "branch": {"preflight": [], "rust-ci": ["preflight"], "package-ci": ["preflight"],
                       "rust-validation-evidence": ["rust-ci", "package-ci"],
                       "confirm-branch-validation": ["rust-validation-evidence"],
                       "generate-workflow-summary": ["confirm-branch-validation"]},
            "pr": dict(zip(("validate-release-preflight", "rust-ci", "package-ci", "rust-validation-evidence",
                            "confirm-release-validation", "generate-workflow-summary"),
                           ([], ["validate-release-preflight"], ["rust-ci"], ["package-ci"],
                            ["rust-validation-evidence"], ["confirm-release-validation"]))),
        }
        chain = ("rust-ci", "prepare-release-content", "create-release-tag", "validate-release-publication",
                 "package-ci", "rust-validation-evidence", "verify-and-attest-release-assets",
                 "prepare-and-publish-release", "verify-published-release", "generate-workflow-summary")
        expected["publish"] = {name: [chain[i - 1]] if i else [] for i, name in enumerate(chain)}
        for kind, data in (("branch", self.branch), ("pr", self.pr), ("publish", release)):
            jobs = data["jobs"]
            self.assertEqual(set(jobs), set(expected[kind]))
            for name, job in jobs.items():
                with self.subTest(kind=kind, job=name):
                    needs = job.get("needs", [])
                    needs = [needs] if isinstance(needs, str) else needs
                    self.assertEqual(needs, expected[kind][name])
                    references = re.findall(r"needs\.([a-z0-9-]+)\.", json.dumps(job))
                    self.assertLessEqual(set(references), set(needs))
            final = jobs["generate-workflow-summary"]
            self.assertEqual(final["if"], "always()")
            self.assertTrue(final["continue-on-error"])
            self.assertEqual(final["permissions"], {"contents": "read", "actions": "read"})
        package = workflow("build-and-validate-packages.yml")
        call = triggers(package)["workflow_call"]
        for platform in ("macos", "windows"):
            key = f"{platform}-rust-attempt"
            for producer, consumer in zip(chain, chain[1:4]):
                self.assertEqual(release["jobs"][consumer]["outputs"][key],
                                 f"${{{{ needs.{producer}.outputs.{key} }}}}")
            self.assertEqual(release["jobs"]["package-ci"]["with"][key],
                             f"${{{{ needs.validate-release-publication.outputs.{key} }}}}")
            self.assertEqual(release["jobs"]["rust-validation-evidence"]["steps"][-1]["with"][key],
                             f"${{{{ needs.package-ci.outputs.{key} }}}}")
            self.assertEqual(call["inputs"][key]["type"], "string")
            self.assertEqual(call["outputs"][key]["value"],
                             f"${{{{ jobs.verify-windows-release-package-checksums.outputs.{key} }}}}")
            self.assertEqual(package["jobs"]["verify-windows-release-package-checksums"]["outputs"][key],
                             f"${{{{ inputs.{key} }}}}")
        verification = release["jobs"]["verify-published-release"]
        self.assertEqual(verification["outputs"]["verification-artifact-key"],
                         "${{ needs.prepare-and-publish-release.outputs.tag || github.sha }}")

    def test_installed_host_artifact_upload_uses_valid_job_status_context(self):
        jobs = workflow("build-and-validate-packages.yml")["jobs"]
        for platform in ("macos", "windows"):
            with self.subTest(platform=platform):
                steps = jobs[f"build-and-verify-{platform}-package"]["steps"]
                uploads = [step for step in steps if step.get("with", {}).get("path")
                           == "dist/installed-host-simulation/*.json"]
                self.assertEqual(len(uploads), 1)
                upload = uploads[0]
                self.assertEqual(upload["if"], "always()")
                self.assertEqual(upload["with"]["if-no-files-found"],
                                 "${{ job.status == 'success' && 'error' || 'warn' }}")
                self.assertFalse(upload.get("continue-on-error", False))

    def test_package_artifacts_use_producer_attempt_for_all_workflows(self):
        jobs = workflow("build-and-validate-packages.yml")["jobs"]
        for os_name, platform in (("macos", "macos-arm64"), ("windows", "windows-x64-msvc")):
            producer = f"build-and-verify-{os_name}-package"
            consumer = f"verify-{os_name}-release-package-checksums"
            names = [step.get("with", {}).get("name", "") for step in jobs[producer]["steps"]
                     if step.get("uses", "").startswith("actions/upload-artifact@")]
            verify = next(step["with"] for step in jobs[consumer]["steps"]
                          if step.get("uses") == "./.github/actions/verify-release-package-checksums")
            for prefix, key in (("", "package-artifact"), ("checksums-", "checksum-artifact")):
                for release in ("release", "pr-release"):
                    pattern = f"format('{prefix}{release}-{platform}-attempt-{{0}}', "
                    self.assertTrue(any(pattern + "github.run_attempt)" in name for name in names))
                    self.assertIn(pattern + f"needs.{producer}.outputs.artifact-attempt)", verify[key])
            self.assertEqual(f"${{{{ needs.{producer}.outputs.artifact-attempt }}}}",
                             jobs[consumer]["outputs"]["artifact-attempt"])

    def test_credentials_summary_trusted_source_and_token_isolation(self):
        data = workflow("pr-release-credentials-validation.yml")
        self.assertEqual({}, data["permissions"])
        job = data["jobs"]["validate-release-credentials"]
        self.assertNotIn("permissions", job)
        check, render = job["steps"]
        self.assertEqual({"CREDENTIALS_RESULT": "${{ steps.credentials.outcome }}",
                          "WORKFLOW_SHA": "${{ github.workflow_sha }}"}, render["env"])
        self.assertEqual("always()", render["if"])
        self.assertTrue(render["continue-on-error"])
        self.assertIn("secrets.RELEASE_ADMIN_READ_TOKEN", check["env"]["RELEASE_ADMIN_TOKEN"])
        self.assertIn("raw.githubusercontent.com/$GITHUB_REPOSITORY/$WORKFLOW_SHA/", render["run"])
        self.assertNotIn("pull_request.head", render["run"])
        self.assertNotIn("RELEASE_ADMIN", render["run"])
        self.assertTrue(all("uses" not in step for step in job["steps"]))

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
