"""Render one best-effort workflow summary from actual CI results."""
import argparse
from datetime import datetime, timezone
from functools import lru_cache
import json
import math
import os
from pathlib import Path
import re
import subprocess
import tempfile

PLATFORMS = ("macos-arm64", "windows-x64-msvc")
HOSTS = ("codex", "claude")
STATUS = {"success": "Passed", "passed": "Passed", "failure": "Failed", "failed": "Failed",
          "skipped": "Skipped", "cancelled": "Cancelled", "timed_out": "Timed out"}
RUST_STEPS = (
    ("Formatting", "Check Rust formatting", None),
    ("All targets", "Check all Rust targets", None),
    ("Clippy", "Check Rust lints", None),
    ("Compile tests", "Compile Rust tests", "compile"),
    ("Workspace tests", "Run Rust unit and integration tests", "test"),
    ("Doc tests", "Run Rust doc tests", "doc"),
    ("Enforce test results", "Enforce Rust test results", None),
    ("Release binaries", "Build Work and Skill release binaries", None),
    ("Binary CLI smoke", "Smoke test Rust binaries", None),
    ("Binary Host simulation", "Simulate Host behaviour with the release binaries", None),
)
PACKAGE_STEPS = (
    ("Native regression", "Verify {os} installer prerequisites"),
    ("Native release build", "Build native release binary"),
)
PACKAGE_PHASES = (("Package build", "package-build"), ("Package integrity", "package-integrity"),
                  ("Downloader E2E", "downloader"))


def installed_host_label(host):
    return f"Installed {host.capitalize()} Host simulation"


RUST_ROWS = tuple(label for label, _, _ in RUST_STEPS)
PACKAGE_ROWS = (*[label for label, _ in PACKAGE_STEPS], *[label for label, _ in PACKAGE_PHASES],
                "Installer E2E", *map(installed_host_label, HOSTS), "Checksum")
WORKFLOW_JOBS = {
    "branch": ("preflight", "rust-ci", "package-ci", "rust-validation-evidence", "confirm-branch-validation"),
    "pr": ("validate-release-preflight", "rust-ci", "package-ci", "rust-validation-evidence", "confirm-release-validation"),
    "publish": ("rust-ci", "prepare-release-content", "create-release-tag", "validate-release-publication",
                "package-ci", "rust-validation-evidence", "verify-and-attest-release-assets",
                "prepare-and-publish-release", "verify-published-release"),
}
LABELS = {"preflight": "Preflight", "validate-release-preflight": "Release preflight",
          "rust-ci": "Rust validation", "package-ci": "Package validation",
          "rust-validation-evidence": "Strict evidence gate",
          "confirm-branch-validation": "Confirm branch validation",
          "confirm-release-validation": "Confirm release validation",
          "prepare-release-content": "Release content", "create-release-tag": "Create tag",
          "validate-release-publication": "Publication validation",
          "verify-and-attest-release-assets": "Verify and attest assets",
          "prepare-and-publish-release": "Publication", "verify-published-release": "Post-publish verification"}


def status(value):
    return STATUS.get(value, "Unavailable")


def safe(value):
    if value is None:
        return "Unavailable"
    # Treat evidence and ref names as plain text, never Markdown or HTML.
    return str(value).replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;").replace("|", "&#124;").replace("\x60", "&#96;").replace("[", "&#91;").replace("]", "&#93;").replace("!", "&#33;").replace("*", "&#42;").replace("_", "&#95;").replace("\\", "&#92;").replace("\r", " ").replace("\n", " ")[:400]


def number(value):
    return value if isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value) and value >= 0 else None


def timestamp(value):
    try:
        return datetime.fromisoformat(value.replace("Z", "+00:00"))
    except (AttributeError, TypeError, ValueError):
        return None


def duration(item):
    start = timestamp(item.get("started_at"))
    end = timestamp(item.get("completed_at"))
    return number((end - start).total_seconds()) if start and end else None


def result(value="Unavailable", seconds=None, approximate=False):
    return {"status": value, "seconds": number(seconds), **({"approximate": True} if approximate else {})}


def elapsed_label(value):
    if number(value) is None:
        return "Unavailable"
    hours, remainder = divmod(round(value), 3600)
    minutes, seconds = divmod(remainder, 60)
    if hours:
        return f"{hours}h {minutes}m {seconds}s"
    return f"{minutes}m {seconds}s" if minutes else f"{seconds}s"


def duration_bar(value, maximum):
    seconds = number(value.get("seconds"))
    if seconds is None:
        return "Unavailable"
    length = round(seconds / maximum * 20) if maximum else 0
    label = (f"~{seconds:.0f} s" if value.get("approximate") else
             "<0.1 s" if 0 < seconds < 0.05 else f"{seconds:.1f} s")
    return ("█" * length + " " if length else "") + label


def combine(items):
    values = [item["status"] for item in items]
    for candidate in ("Failed", "Timed out", "Cancelled", "Unavailable", "Skipped"):
        if candidate in values:
            return result(candidate, sum(item["seconds"] for item in items) if all(item["seconds"] is not None for item in items) else None)
    return result("Passed", sum(item["seconds"] for item in items) if all(item["seconds"] is not None for item in items) else None)


def find_job(jobs, name):
    matches = [job for job in jobs if job.get("name") == name or job.get("name", "").endswith(" / " + name)]
    return matches[0] if len(matches) == 1 else {}


def collect_stage_results(kind, jobs, terminal_needs):
    """Read actual job conclusions without requiring transitive needs edges."""
    names = {
        "preflight": ("validate branch preflight",),
        "validate-release-preflight": ("validate release preflight",),
        "rust-ci": ("validate and build rust (macOS)", "validate and build rust (Windows)"),
        "package-ci": ("build macOS package", "build Windows package",
                       "verify macOS checksums", "verify Windows checksums"),
        "rust-validation-evidence": ("rust validation evidence",),
        "confirm-branch-validation": ("confirm branch validation",),
        "confirm-release-validation": ("confirm release validation",),
        "prepare-release-content": ("prepare release content before tag",),
        "create-release-tag": ("create release tag",),
        "validate-release-publication": ("validate release publication",),
        "verify-and-attest-release-assets": ("verify and attest release assets",),
        "prepare-and-publish-release": ("prepare and publish release",),
        "verify-published-release": ("verify published release",),
    }
    required = WORKFLOW_JOBS[kind]
    results = {}
    for key in required:
        values = [find_job(jobs, name).get("conclusion") for name in names[key]]
        conclusion = next((value for value in ("failure", "timed_out", "cancelled")
                           if value in values), None)
        if conclusion is None and all(value in ("success", "skipped") for value in values):
            conclusion = "skipped" if "skipped" in values else "success"
        results[key] = {"result": conclusion}
    # The final direct dependency is authoritative even if the Jobs API is delayed.
    terminal = required[-1]
    if terminal in terminal_needs:
        results[terminal] = terminal_needs[terminal]
    return results


def step_result(job, name):
    matches = [step for step in job.get("steps", []) if step.get("name") == name]
    if len(matches) == 1:
        step = matches[0]
        return result(status(step.get("conclusion")), duration(step), approximate=True)
    # Only explicit skipped/cancelled job state establishes nonexecution.
    return result(status(job.get("conclusion")) if job.get("conclusion") in ("skipped", "cancelled", "timed_out") else "Unavailable")


def identity(data, commit, run_id, attempt, platform=None):
    return (isinstance(data, dict) and data.get("commit") == commit
            and str(data.get("run_id")) == str(run_id) and str(data.get("run_attempt")) == str(attempt)
            and (platform is None or data.get("platform") == platform))


def gh_json(*arguments):
    completed = subprocess.run(["gh", *arguments], capture_output=True, text=True, check=True, timeout=45)
    return json.loads(completed.stdout)


def collect_jobs(repository, run_id, attempt):
    endpoint = f"repos/{repository}/actions/runs/{run_id}"
    try:
        pages = gh_json("api", "--paginate", "--slurp", endpoint + "/jobs?filter=latest&per_page=100")
        jobs = [job for page in pages for job in page["jobs"]]
    except (OSError, ValueError, KeyError, subprocess.SubprocessError):
        return []
    # API job records may omit run_attempt; resolve the actual producer by job ID.
    unresolved = {job["id"] for job in jobs if "id" in job and not job.get("run_attempt")}
    for n in range(int(attempt), 0, -1):
        if not unresolved:
            break
        try:
            pages = gh_json("api", "--paginate", "--slurp", endpoint + f"/attempts/{n}/jobs?per_page=100")
            ids = {job["id"] for page in pages for job in page["jobs"]}
            for job in jobs:
                if job.get("id") in unresolved & ids:
                    job["run_attempt"] = n
            unresolved -= ids
        except (OSError, ValueError, KeyError, subprocess.SubprocessError):
            continue
    return jobs


@lru_cache(maxsize=16)
def read_artifact_files(run_id, name):
    if not re.fullmatch(r"[a-zA-Z0-9_.-]+", name):
        return {}
    try:
        with tempfile.TemporaryDirectory() as directory:
            subprocess.run(["gh", "run", "download", str(run_id), "--name", name, "--dir", directory],
                           capture_output=True, check=True, timeout=60)
            files, duplicates = {}, set()
            for path in Path(directory).rglob("*.json"):
                if path.name in files:
                    duplicates.add(path.name)
                    continue
                try:
                    files[path.name] = json.loads(path.read_text(encoding="utf-8"))
                except (OSError, ValueError):
                    files[path.name] = None
            return {name: data for name, data in files.items() if name not in duplicates}
    except (OSError, ValueError, subprocess.SubprocessError):
        return {}


def read_artifact(run_id, name, filename):
    return read_artifact_files(run_id, name).get(filename)


def collect_platform(jobs, platform, commit, run_id):
    os_name = "macOS" if platform.startswith("macos") else "Windows"
    rust_job = find_job(jobs, f"validate and build rust ({os_name})")
    package_job = find_job(jobs, f"build {os_name} package")
    checksum_job = find_job(jobs, f"verify {os_name} checksums")
    rust_attempt = rust_job.get("run_attempt")
    package_attempt = package_job.get("run_attempt")
    rust = read_artifact(run_id, f"rust-test-results-{platform}-attempt-{rust_attempt}", "test-results.json") if rust_attempt else None
    if not identity(rust, commit, run_id, rust_attempt, platform):
        rust = {}
    rows = {}
    for label, step, phase in RUST_STEPS:
        value = step_result(rust_job, step)
        if phase:
            evidence = rust.get("phases", {}).get(phase, {})
            if evidence.get("status") in ("passed", "failed"):
                value = result(status(evidence["status"]), evidence.get("duration_seconds"))
            elif value["status"] == "Passed":
                # continue-on-error can turn actual failures into a successful conclusion.
                value = result("Unavailable", value["seconds"], approximate=value.get("approximate", False))
        rows[label] = value
    for label, step in PACKAGE_STEPS:
        rows[label] = step_result(package_job, step.format(os=os_name))
    combined_step = step_result(package_job, f"Package and verify {os_name} ZIP")
    for label, phase in PACKAGE_PHASES:
        values = []
        for host in HOSTS:
            record = read_artifact(run_id, f"package-validation-{platform}-attempt-{package_attempt}", f"{phase}-{host}.json") if package_attempt else None
            if identity(record, commit, run_id, package_attempt) and record.get("phase") == f"{phase}-{host}" and isinstance(record.get("exit_code"), int):
                values.append(result("Passed" if record["exit_code"] == 0 else "Failed", record.get("duration_seconds")))
            else:
                values.append(result(combined_step["status"] if combined_step["status"] in ("Skipped", "Cancelled", "Timed out") else "Unavailable"))
        rows[label] = combine(values)
    rows["Installer E2E"] = step_result(package_job, f"Verify {os_name} release installation")
    for host in HOSTS:
        record = read_artifact(run_id, f"installed-host-simulation-{platform}-attempt-{package_attempt}", f"{host}-{platform}.json") if package_attempt else None
        valid = identity(record, commit, run_id, package_attempt, platform) and record.get("host") == host
        if valid and record.get("status") == "passed" and isinstance(record.get("simulation"), dict) and all(record["simulation"].get(key) for key in ("work_contracts", "skill_contracts", "on_demand_sources", "discussion_versions", "trace_anchors", "independent_skills")):
            rows[installed_host_label(host)] = result("Passed", record.get("duration_seconds"))
        elif valid and record.get("status") in ("failed", "timed_out"):
            rows[installed_host_label(host)] = result(status(record["status"]), record.get("duration_seconds"))
        else:
            installation = rows["Installer E2E"]["status"]
            rows[installed_host_label(host)] = result(installation if installation in ("Skipped", "Cancelled", "Timed out") else "Unavailable")
    rows["Checksum"] = step_result(checksum_job, f"Verify {os_name} package checksums")
    return {"rows": rows, "rust": rust}


def table(lines, header, values):
    lines.extend(["", "| " + " | ".join(header) + " |", "| " + " | ".join("---" for _ in header) + " |"])
    lines.extend("| " + " | ".join(safe(value) for value in row) + " |" for row in values)


def stats(rust, doc=False):
    counts = rust.get("doc_tests" if doc else "test_counts")
    if not isinstance(counts, dict):
        return [None] * 4
    values = [counts.get(key) for key in ("passed", "failed", "ignored" if doc else "skipped")]
    return [sum(values) if all(number(value) is not None for value in values) else None, *values]


def render(kind, meta, needs, platforms=None, jobs=(), release_content=None, release_report=None):
    required = WORKFLOW_JOBS.get(kind, ("credentials",))
    statuses = [status(needs.get(key, {}).get("result")) for key in required]
    overall = combine([result(value) for value in statuses])["status"]
    lines = [f"# {safe({'branch': 'Branch validation', 'pr': 'PR release validation', 'credentials': 'PR release credentials validation', 'publish': 'Publish release'}[kind])}",
             "", f"**Validation result: {overall}**", "",
             f"Ref: {safe(meta.get('ref'))} · Commit: {safe(meta.get('commit'))}",
             f"Run: {safe(meta.get('run_id'))} · Attempt: {safe(meta.get('attempt'))}",
             f"Elapsed at summary: {elapsed_label(meta.get('elapsed'))}",
             "", "## Required validation"]
    table(lines, ("Validation", "Result"), [(LABELS.get(key, "Release credentials"), value) for key, value in zip(required, statuses)])
    if kind == "pr":
        preflight = find_job(jobs, "validate release preflight")
        table(lines, ("PR checks", "Result"), [(label, step_result(preflight, step)["status"]) for label, step in (
            ("Version guard", "Validate pull request version"), ("Unused release version", "Check proposed release version is unused"),
            ("Release content declarations", "Validate release content declarations"))])
    if platforms is not None and kind != "credentials":
        names = (*RUST_ROWS, *PACKAGE_ROWS)
        lines.extend(["", "## Rust validation"])
        table(lines, ("Validation", "macOS", "Windows"), [(name, *(platforms.get(p, {}).get("rows", {}).get(name, result())["status"] for p in PLATFORMS)) for name in RUST_ROWS])
        for doc, title in ((False, "Workspace test statistics"), (True, "Doc test statistics")):
            lines.extend(["", f"### {title}"])
            values = [stats(platforms.get(p, {}).get("rust", {}), doc) for p in PLATFORMS]
            table(lines, ("Result", "macOS", "Windows"), [(name, values[0][i], values[1][i]) for i, name in enumerate(("Total", "Passed", "Failed", "Ignored" if doc else "Skipped"))])
        lines.extend(["", "## Package validation"])
        table(lines, ("Validation", "macOS", "Windows"), [(name, *(platforms.get(p, {}).get("rows", {}).get(name, result())["status"] for p in PLATFORMS)) for name in PACKAGE_ROWS])
        lines.extend(["", "## Runner environment"])
        fields = (("OS", "runner_os"), ("Architecture", "runner_arch"), ("Image", "runner_image"), ("Rust", "rustc"), ("Cargo", "cargo"), ("Target", "target"))
        table(lines, ("Field", "macOS", "Windows"), [(label, *(platforms.get(p, {}).get("rust", {}).get(key) for p in PLATFORMS)) for label, key in fields])
        seconds = [platforms.get(p, {}).get("rows", {}).get(name, result())["seconds"] for p in PLATFORMS for name in names]
        maximum = max((value for value in seconds if value is not None), default=0)
        lines.extend(["", "## Cross-platform duration", "",
                      "Bars share one scale (20 blocks at the maximum); short phases may have no visible bar. "
                      "~ indicates second-resolution GitHub timestamps, including ~0 s for a phase completed within one timestamp interval. "
                      "Installed Host simulations are part of Installer E2E; durations must not be added together as workflow elapsed time."])
        table(lines, ("Phase", "macOS", "Windows"), [(name, *(duration_bar(platforms.get(p, {}).get("rows", {}).get(name, result()), maximum) for p in PLATFORMS)) for name in names])
    if kind == "publish":
        lines.extend(["", "## Release evidence"])
        if release_content:
            table(lines, ("Field", "Value"), [("Baseline", release_content.get("baseline_label")), ("Target", release_content.get("tag")), ("Sources", release_content.get("sources"))])
        else:
            lines.append("Release content evidence unavailable.")
        if release_report:
            table(lines, ("Field", "Value"), [("Verification", status(release_report.get("result"))), ("Attestation verified", release_report.get("attestation_verified")), ("Assets matching", sum(item.get("matches") is True for item in release_report.get("assets", []))), ("Failure records", len(release_report.get("failure_reasons", [])))])
        else:
            lines.append("Post-publish verification evidence unavailable.")
    failures = [job for job in jobs if job.get("conclusion") in ("failure", "cancelled", "timed_out")]
    if failures or overall != "Passed":
        lines.extend(["", "## Failed or incomplete validation"])
        for job in failures:
            lines.append(f"- {safe(job.get('name'))}: {status(job.get('conclusion'))}")
            for step in job.get("steps", []):
                if step.get("conclusion") in ("failure", "cancelled", "timed_out"):
                    lines.append(f"  - {safe(step.get('name'))}: {status(step.get('conclusion'))}")
            if str(job.get("id", "")).isdigit() and re.fullmatch(r"[\w.-]+/[\w.-]+", str(meta.get("repository", ""))):
                lines.append(f"  - [Job log](https://github.com/{meta['repository']}/actions/runs/{meta['run_id']}/job/{job['id']})")
        for platform, data in (platforms or {}).items():
            for name, value in data.get("rows", {}).items():
                if value["status"] == "Failed":
                    lines.append(f"- {safe(platform)} — {safe(name)}: Failed")
        for platform, data in (platforms or {}).items():
            for phase, evidence in data.get("rust", {}).get("phases", {}).items():
                code = evidence.get("exit_code")
                if phase in ("compile", "test", "doc") and isinstance(code, int) and code != 0:
                    lines.append(f"- {safe(platform)} — Rust {phase}: exit code {code}")
        if not failures:
            lines.append("See required validation results and job logs for details.")
    return "\n".join(lines) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("kind", choices=("branch", "pr", "publish", "credentials"))
    args = parser.parse_args()
    repository = os.environ["GITHUB_REPOSITORY"]
    run_id = os.environ["GITHUB_RUN_ID"]
    attempt = os.environ["GITHUB_RUN_ATTEMPT"]
    commit = os.environ["GITHUB_SHA"]
    meta = {"repository": repository, "run_id": run_id, "attempt": attempt, "commit": commit, "ref": os.environ.get("GITHUB_REF")}
    jobs, platforms, release_content, release_report = [], None, None, None
    if args.kind == "credentials":
        needs = {"credentials": {"result": os.environ.get("CREDENTIALS_RESULT")}}
        try:
            meta["elapsed"] = max(0, datetime.now(timezone.utc).timestamp() - float(os.environ["CREDENTIALS_STARTED"]))
        except (KeyError, ValueError):
            pass
    else:
        terminal_needs = json.loads(os.environ.get("NEEDS_JSON", "{}"))
        jobs = collect_jobs(repository, run_id, attempt)
        needs = collect_stage_results(args.kind, jobs, terminal_needs)
        try:
            run = gh_json("api", f"repos/{repository}/actions/runs/{run_id}/attempts/{attempt}")
            start = timestamp(run.get("run_started_at"))
            if start:
                meta["elapsed"] = max(0, (datetime.now(timezone.utc) - start).total_seconds())
        except (OSError, ValueError, subprocess.SubprocessError):
            pass
        platforms = {platform: collect_platform(jobs, platform, commit, run_id) for platform in PLATFORMS}
        if args.kind == "publish":
            producer = find_job(jobs, "prepare release content before tag").get("run_attempt")
            if producer:
                content = read_artifact(run_id, f"validated-release-content-attempt-{producer}", "release-content.json")
                if isinstance(content, dict) and content.get("sha") == commit:
                    release_content = content
            producer = find_job(jobs, "verify published release").get("run_attempt")
            key = needs["verify-published-release"].get("outputs", {}).get("verification-artifact-key")
            keys = (key,) if key else tuple(dict.fromkeys(
                value for value in ((release_content or {}).get("tag"), commit) if value))
            if producer:
                for key in keys:
                    report = read_artifact(run_id, f"release-verification-{key}-attempt-{producer}", "release-verification.json")
                    if isinstance(report, dict) and report.get("commit") == commit:
                        release_report = report
                        break
    summary = render(args.kind, meta, needs, platforms, jobs, release_content, release_report)
    with Path(os.environ["GITHUB_STEP_SUMMARY"]).open("a", encoding="utf-8") as stream:
        stream.write(summary)


if __name__ == "__main__":
    main()
