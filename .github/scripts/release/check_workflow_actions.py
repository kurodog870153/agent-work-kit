"""Reject workflow uses references outside the approved source and version list."""

import argparse
import json
import shutil
import subprocess
import sys
from pathlib import Path


APPROVED = {
    "actions/checkout@v7",
    "actions/upload-artifact@v7",
    "actions/download-artifact@v8",
    "actions/attest@v4",
    "taiki-e/install-action@v2",
    "ruby/setup-ruby@v1",
    "./.github/workflows/rust-validation.yml",
    "./.github/actions/verify-release-package-checksums",
    "./.github/actions/select-release-rust-toolchain",
    "./.github/actions/verify-windows-batch-line-endings",
    "./.github/actions/verify-windows-release-install",
}
RUBY_REFERENCES = r"""
require "json"
require "yaml"
workflow = YAML.safe_load(File.read(ARGV[0]), aliases: true)
references = []
jobs = workflow.is_a?(Hash) ? workflow["jobs"] : nil
if jobs.is_a?(Hash)
  jobs.each do |job_name, job|
    next unless job.is_a?(Hash)
    references << ["jobs.#{job_name}.uses", job["uses"]] if job.key?("uses")
    if job["steps"].is_a?(Array)
      references.concat(job["steps"].each_with_index.filter_map do |step, index|
        next unless step.is_a?(Hash) && step.key?("uses")
        ["jobs.#{job_name}.steps[#{index}].uses", step["uses"]]
      end)
    end
  end
end
runs = workflow.is_a?(Hash) ? workflow["runs"] : nil
if runs.is_a?(Hash) && runs["using"] == "composite" && runs["steps"].is_a?(Array)
  references.concat(runs["steps"].each_with_index.filter_map do |step, index|
    next unless step.is_a?(Hash) && step.key?("uses")
    ["runs.steps[#{index}].uses", step["uses"]]
  end)
end
puts JSON.generate(references)
"""


def workflow_references(path):
    if shutil.which("ruby"):
        result = subprocess.run(
            ["ruby", "-e", RUBY_REFERENCES, str(path)], capture_output=True, text=True, check=True
        )
        return json.loads(result.stdout)

    # PyYAML is available on some development machines; CI uses its existing Ruby parser.
    import yaml

    references = []
    try:
        workflow = yaml.safe_load(path.read_text(encoding="utf-8"))
    except yaml.YAMLError as error:
        raise ValueError(str(error)) from error
    jobs = workflow.get("jobs") if isinstance(workflow, dict) else None
    if isinstance(jobs, dict):
        for job_name, job in jobs.items():
            if not isinstance(job, dict):
                continue
            if "uses" in job:
                references.append((f"jobs.{job_name}.uses", job["uses"]))
            if isinstance(job.get("steps"), list):
                for index, step in enumerate(job["steps"]):
                    if isinstance(step, dict) and "uses" in step:
                        references.append((f"jobs.{job_name}.steps[{index}].uses", step["uses"]))
    runs = workflow.get("runs") if isinstance(workflow, dict) else None
    if isinstance(runs, dict) and runs.get("using") == "composite" and isinstance(runs.get("steps"), list):
        for index, step in enumerate(runs["steps"]):
            if isinstance(step, dict) and "uses" in step:
                references.append((f"runs.steps[{index}].uses", step["uses"]))
    return references


def check_workflow(path):
    errors = []
    try:
        references = workflow_references(path)
    except subprocess.CalledProcessError as error:
        return [f"{path}: invalid workflow YAML: {(error.stderr or str(error)).strip()}"]
    except (OSError, ValueError) as error:
        return [f"{path}: invalid workflow YAML: {error}"]
    for location, reference in references:
        if not isinstance(reference, str) or reference not in APPROVED:
            errors.append(f"{path}:{location}: unapproved uses: {reference}")
    return errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", nargs="?", type=Path, default=Path(".github/workflows"))
    args = parser.parse_args()
    paths = sorted((*args.directory.glob("*.yml"), *args.directory.glob("*.yaml")))
    if args.directory == Path(".github/workflows"):
        paths.extend(sorted(Path(".github/actions").glob("*/action.y*ml")))
    if not paths:
        parser.error("no workflow files found")
    errors = [error for path in paths for error in check_workflow(path)]
    for error in errors:
        print(error, file=sys.stderr)
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
