"""Simulate Host behaviour against real Work and Skill CLI binaries.

A Host (Codex or Claude Code) is simulated by a script that only uses the public CLI surface:
every CLI call is a fresh process, so state that survives between calls proves cross-session
recovery. All requests are generated here; no fixture files are read.
"""

import argparse
import hashlib
import os
import stat
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

TIMEOUT = 60


class Cli:
    """One CLI binary; every call is a new process and returns (exit_code, parsed JSON)."""

    def __init__(self, binary, cwd=None):
        self.binary = Path(binary).resolve()
        self.cwd = cwd

    def raw(self, *args, stdin=None):
        return subprocess.run([str(self.binary), *map(str, args)], capture_output=True, timeout=TIMEOUT, cwd=self.cwd,
                              input=stdin.encode("utf-8") if stdin is not None else None, check=False)

    def call(self, *args, stdin=None):
        result = self.raw(*args, stdin=stdin)
        try:
            return result.returncode, json.loads(result.stdout.decode("utf-8"))
        except json.JSONDecodeError:
            raise AssertionError(f"{self.binary.name} {args!r} returned non-JSON: "
                                 f"{result.stdout!r} {result.stderr!r}") from None

    def ok(self, *args, stdin=None):
        code, body = self.call(*args, stdin=stdin)
        if code != 0 or body.get("status") != "ok":
            raise AssertionError(f"{self.binary.name} {args!r} failed: {code} {body!r}")
        return body["data"]

    def reject(self, reason, *args, stdin=None):
        code, body = self.call(*args, stdin=stdin)
        got = body.get("error", {}).get("reason")
        if code == 0 or got != reason:
            raise AssertionError(f"{self.binary.name} {args!r} expected {reason}, got {code} {body!r}")


def request(**fields):
    return json.dumps(fields, separators=(",", ":"))


def check(condition, message):
    if not condition:
        raise AssertionError(message)


def cli_contract_smoke(work, skill, evidence):
    """Public contract queries of both CLIs; Help, List, Describe, Scaffold and Example agree."""
    for cli, prefix in ((work, "work"), (skill, "skill")):
        check(cli.raw("--help").returncode == 0, f"{prefix} --help failed")
        listing = cli.ok("list")
        check(all(command.startswith(prefix) for command in listing["commands"]), f"{prefix} command index")
        constructible = [c["contract_id"] for c in listing["contracts"] if c["caller_constructible"]]
        check(constructible, f"{prefix} exposes no request contract")
        for contract in constructible:
            check(cli.ok("describe", "--contract", contract)["contract_id"] == contract, contract)
            check(cli.ok("scaffold", "--contract", contract), contract)
        for command in listing["commands"]:
            if command != prefix:
                cli.ok("describe", "--command", command)
        evidence[f"{prefix}_contracts"] = len(listing["contracts"])
    # Examples of Work request contracts are caller-usable requests.
    for contract in ("work-source-create-request", "work-discussion-create-request", "work-task-create-request"):
        check(work.ok("example", "--contract", contract, "--scenario", "default")["valid"], contract)
    work.reject("unknown_scenario", "example", "--contract", "work-source-create-request", "--scenario", "none")
    # Tooling and Contract Skill are not CLIs any more.
    for retired in ("tooling", "contract", "migration", "recover"):
        check(retired not in " ".join(work.ok("list")["commands"]), f"Work CLI still exposes {retired}")
    check(work.raw("migration").returncode != 0, "Work CLI accepted a migration command")


def on_demand_loading(skill, skills_dir, scratch, evidence):
    """Skills load only the requested branch, and each Skill resolves Shared on its own."""
    root = skill.ok("discover", "--install", skills_dir, "--skill", "work")
    check({child["node"] for child in root["children"]} == {"execute", "task"}, "Work root children")
    nodes = {"work": ["task"]}
    wanted = scratch / "resolve-task.json"
    wanted.write_text(request(schema="skill-resolve-request",
                              selections=[{"skill": "work", "nodes": nodes["work"]}]), encoding="utf-8")
    sources = [item["source"] for item in skill.ok("resolve", "--install", skills_dir, "--request", wanted)["sources"]]
    check("shared:bootstrap.md" in sources and "skill(work):instructions/task/instructions.md" in sources, sources)
    check(not any("/execute/" in source for source in sources), "an unrequested branch was loaded")
    evidence["on_demand_sources"] = len(sources)

    switched = scratch / "resolve-execute.json"
    switched.write_text(request(schema="skill-resolve-request",
                                selections=[{"skill": "work", "nodes": ["execute"]}]), encoding="utf-8")
    skill.ok("switch", "--install", skills_dir, "--active", wanted, "--next", switched)

    skill.reject("ancestor_missing", "resolve", "--install", skills_dir, "--request", "-",
                 stdin=request(schema="skill-resolve-request", selections=[{"skill": "work", "nodes": ["missing"]}]))

    # Each Skill is independently activatable with only itself and the shared Shared tree installed.
    for name in ("work", "tooling"):
        alone = scratch / f"only-{name}"
        alone.mkdir()
        for part in (name, "shared"):
            shutil.copytree(skills_dir / part, alone / part)
        skill.ok("validate", "--install", alone)
        skill.ok("discover", "--install", alone, "--skill", name)
        other = "tooling" if name == "work" else "work"
        skill.reject("unknown_root", "discover", "--install", alone, "--skill", other)
    evidence["independent_skills"] = ["work", "tooling"]


def requirement_clarification(work, root, evidence):
    """One question at a time, answers bound to the asked question, resumed by a new process."""
    work.ok("source", "create", "--root", root, "--request", "-",
            stdin=request(schema="work-source-create-request", id="src-1", title="Issue",
                          locator="https://example.test/1", content={"kind":"text", "text":"  raw\r\nbody "},
                          origin={"source": "external"}))
    work.ok("discussion", "create", "--root", root, "--request", "-",
            stdin=request(schema="work-discussion-create-request", id="disc-1",
                          original_request="Add export", source_ids=["src-1"]))
    ask = request(schema="work-discussion-round-request", id="disc-1", expected_version=1,
                  ask={"id": "q1", "text": "Format?", "options": [{"id": "1", "text": "CSV"}, {"id": "2", "text": "PDF"}],
                       "recommendation": "1", "reason": "simple"})
    work.ok("discussion", "round", "--root", root, "--request", "-", stdin=ask)

    # A later Host session recovers the open question and its numbered options from the record alone.
    progress = work.ok("discussion", "progress", "--root", root, "--id", "disc-1")
    question = progress["outstanding_question"]
    check(question["id"] == "q1" and [o["text"] for o in question["options"]] == ["CSV", "PDF"], progress)
    # A second question cannot be asked over the open one, and an answer must name a real option.
    work.reject("version_conflict", "discussion", "round", "--root", root, "--request", "-", stdin=ask)
    work.reject("rule_violation", "discussion", "round", "--root", root, "--request", "-",
                stdin=request(schema="work-discussion-round-request", id="disc-1", expected_version=2,
                              answer={"question_id": "q1", "option_id": "9"}))
    work.ok("discussion", "round", "--root", root, "--request", "-",
            stdin=request(schema="work-discussion-round-request", id="disc-1", expected_version=2,
                          answer={"question_id": "q1", "option_id": "2"},
                          decisions=[{"id": "dec1", "text": "Use PDF", "question_id": "q1"}]))
    done = work.ok("discussion", "progress", "--root", root, "--id", "disc-1")
    check(not done.get("outstanding_question"), done)
    history = work.ok("discussion", "history", "--root", root, "--id", "disc-1")
    check(len(history["versions"]) == 3 if "versions" in history else json.dumps(history).count('"version"') == 3, history)
    check("Use PDF" in json.dumps(work.ok("discussion", "show", "--root", root, "--id", "disc-1")), "decision lost")
    check("Use PDF" not in json.dumps(work.ok("discussion", "show", "--root", root, "--id", "disc-1", "--version", "2")),
          "history was rewritten")
    evidence["discussion_versions"] = 3


def task_and_acceptance(work, root, evidence):
    """TASK, Execution/Attempt, two-level Acceptance, Revise and trace."""
    send = lambda *command, body: work.ok(*command, "--root", root, "--request", "-", stdin=body)  # noqa: E731
    task = request(schema="work-task-create-request", id="task-1", title="Export", goal="Users can export",
                   requirements=["Export as PDF"], discussion_ids=["disc-1"], source_ids=["src-1"],
                   criteria=[{"id": "c1", "text": "PDF opens"}, {"id": "c2", "text": "Docs updated", "required": False}])
    def reviewed_task(operation, draft):
        proposal = {"operation": operation, "request": json.loads(draft)}
        preview = send("task", "preview", body=request(schema="work-task-preview-request", draft=proposal))
        path = Path(root) / "preview" / "task" / (proposal["request"]["id"] + ".json")
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(request(**preview), encoding="utf-8")
        preview = send("task", "preview", body=request(schema="work-task-preview-request", draft=proposal))
        path.write_text(request(**preview), encoding="utf-8")
        reviewed = json.loads(path.read_text(encoding="utf-8"))
        check(list(path.parent.iterdir()) == [path], "duplicate Preview file")
        apply = dict(schema="work-task-apply-request", draft=proposal,
                     approved_candidate=reviewed["candidate"], reviewed_dependencies=reviewed["reviewed_dependencies"])
        work.reject("approval_mismatch", "task", "apply", "--root", root, "--request", "-",
                    stdin=request(**{**apply, "approved_candidate": "not approved"}))
        check(path.exists(), "failed Apply lost Preview")
        send("task", "apply", body=request(**apply))
        path.unlink()
        check(not path.exists(), "successful Apply retained Preview")

    reviewed_task("create", task)
    log = Path(root) / "log" / "task-1"
    log.mkdir(parents=True)
    (log / "build.log").write_text("initial build failed", encoding="utf-8")
    (log / "retry").mkdir()
    (log / "retry" / "test.log").write_text("export test passed", encoding="utf-8")
    send("execution", "record", body=request(
        schema="work-execution-record-request", id="exec-1", task_id="task-1",
        attempt={"id": "at-1", "result": "failed", "changes": ["added button"], "anomalies": ["timeout"]}))
    send("execution", "record", body=request(
        schema="work-execution-record-request", id="exec-1", task_id="task-1", expected_version=1,
        attempt={"id": "at-2", "result": "succeeded", "changes": ["fixed timeout"], "verification": ["export test"]}))
    complete = lambda version: request(schema="work-task-complete-request", id="task-1", expected_version=version)  # noqa: E731

    def accept(version, expected, verdict):
        fields = dict(schema="work-task-acceptance-record-request", id="acc-1", task_id="task-1", task_version=version,
                      conclusions=[{"criterion_id": "c1", "verdict": verdict, "evidence": "checked",
                                    "origin": {"source": "host"}}])
        if expected:
            fields["expected_version"] = expected
        return request(**fields)

    # Execution never completes anything; required Acceptance gates completion.
    work.reject("rule_violation", "task", "complete", "--root", root, "--request", "-", stdin=complete(1))
    send("acceptance", "task-record", body=accept(1, None, "failed"))
    work.reject("rule_violation", "task", "complete", "--root", root, "--request", "-", stdin=complete(1))
    send("acceptance", "task-record", body=accept(1, 1, "passed"))
    send("task", "complete", body=complete(1))

    # Requirement Acceptance is a separate record that completion did not create.
    work.reject("not_found", "acceptance", "requirement-show", "--root", root, "--id", "req-1")
    requirement = lambda expected, version: request(  # noqa: E731
        **{"schema": "work-requirement-acceptance-record-request", "id": "req-1", "discussion_id": "disc-1",
           **({"expected_version": expected} if expected else {}), "verdict": "passed", "evidence": "user confirmed",
           "origin": {"source": "user"}, "task_refs": [{"kind": "task", "id": "task-1", "version": version}]})
    send("acceptance", "requirement-record", body=requirement(None, 2))

    # Revise: affected Acceptance must be confirmed again and history is kept.
    reviewed_task("revise", request(
        schema="work-task-revise-request", id="task-1", expected_version=2, reason="also CSV",
        requirements=["Export as PDF and CSV"], reconfirm_requirement=True,
        criteria=[{"id": "c1", "text": "PDF and CSV open"}, {"id": "c2", "text": "Docs updated", "required": False}]))
    work.reject("rule_violation", "task", "complete", "--root", root, "--request", "-", stdin=complete(3))
    check(work.ok("acceptance", "requirement-show", "--root", root, "--id", "req-1")["needs_reconfirm"] is True
          if "needs_reconfirm" in work.ok("acceptance", "requirement-show", "--root", root, "--id", "req-1")
          else "needs_reconfirm\": true" in json.dumps(work.ok("acceptance", "requirement-show", "--root", root, "--id", "req-1")),
          "Requirement Acceptance was not reopened")
    check("completed" in json.dumps(work.ok("task", "show", "--root", root, "--id", "task-1", "--version", "2")),
          "revision lost completed history")
    send("acceptance", "task-record", body=accept(3, 2, "passed"))
    send("task", "complete", body=complete(3))
    send("acceptance", "requirement-record", body=requirement(1, 4))

    for kind, record in (("source", "src-1"), ("discussion", "disc-1"), ("task", "task-1"), ("execution", "exec-1"),
                         ("task_acceptance", "acc-1"), ("requirement_acceptance", "req-1")):
        traced = json.dumps(work.ok("trace", "--root", root, "--kind", kind, "--id", record))
        for expected in ("src-1", "disc-1", "task-1", "exec-1", "acc-1", "req-1"):
            check(expected in traced, f"trace from {kind} lacks {expected}")
    status = {item["id"] for item in work.ok("status", "--root", root)["records"]}
    check({"src-1", "disc-1", "task-1", "exec-1", "acc-1", "req-1"} <= status, status)
    check(not (Path(root) / "txn").exists(), "unexpected transaction directory")
    check(not (Path(root) / "write.tmp").exists(), "temporary file survived a successful write")
    check("build.log" not in json.dumps(work.ok("status", "--root", root)), "Log leaked into formal status")
    shutil.rmtree(log)
    check("export test" in json.dumps(work.ok("execution", "show", "--root", root, "--id", "exec-1")),
          "formal verification depended on disposable Log")
    evidence["trace_anchors"] = 6
    evidence["host_task_preview_and_log"] = True


def correction_preview(work, root, evidence):
    def send(*command, body):
        return work.ok(*command, "--root", root, "--request", "-", stdin=body)
    draft = dict(schema="work-correction-draft-request", id="cor-1",
                 target={"kind": "source", "id": "src-1", "version": 1},
                 statement="first candidate", reason="typo", origin={"source": "user"})
    path = Path(root) / "preview" / "correction" / "cor-1.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    first = send("correction", "preview", body=request(**draft))
    path.write_text(request(**first), encoding="utf-8")
    draft["statement"] = "reviewed candidate"
    preview = send("correction", "preview", body=request(**draft))
    path.write_text(request(**preview), encoding="utf-8")
    check(list(path.parent.iterdir()) == [path], "duplicate Correction Preview")
    check("cor-1" not in json.dumps(work.ok("status", "--root", root)), "unapproved Correction became formal")
    reviewed = json.loads(path.read_text(encoding="utf-8"))
    apply = dict(schema="work-correction-apply-request", draft=draft,
                 approved_candidate=reviewed["candidate"],
                 reviewed_target_version=reviewed["reviewed_target_version"])
    work.reject("approval_mismatch", "correction", "apply", "--root", root, "--request", "-",
                stdin=request(**{**apply, "approved_candidate": first["candidate"]}))
    check(path.exists(), "failed Correction Apply lost Preview")
    send("correction", "apply", body=request(**apply))
    path.unlink()
    check("cor-1" in json.dumps(work.ok("status", "--root", root)), "approved Correction not published")
    evidence["host_correction_preview"] = True


def physical(path, directory=False):
    """Check physical files/directories without following a reparse point."""
    try:
        info = path.lstat()
    except OSError:
        return False
    return (not stat.S_ISLNK(info.st_mode)
            and not (getattr(info, "st_file_attributes", 0) & 0x400)
            and (stat.S_ISDIR(info.st_mode) if directory else stat.S_ISREG(info.st_mode)))


def read_installed_entry(discovery, name):
    """Read actual entry paths; no alternate CLI or install root is supplied."""
    discovery = Path(discovery)
    directory = discovery / name
    entry = directory / "SKILL.md"
    check(physical(directory, directory=True) and physical(entry), f"invalid physical entry: {entry}")
    check({p.name for p in directory.iterdir()} == {"SKILL.md"}, f"unexpected entry contents: {directory}")
    text = entry.read_text(encoding="utf-8")
    lines = text.splitlines()
    check(lines and lines[0] == "---" and f"name: {name}" in lines[1:4], f"invalid entry Frontmatter: {entry}")
    labels = {"Shared Bootstrap": "bootstrap", "Skill Root": "root", "Skill CLI": "skill",
              "Skill CLI --install": "install", "Installation State": "state"}
    if name == "work":
        labels["Work CLI"] = "work"
    paths = {}
    for line in lines:
        if not line.startswith("- "):
            continue
        label, separator, encoded = line[2:].partition(": ")
        if label not in labels:
            check(label != "Work CLI", "Tooling entry must not depend on Work CLI")
            continue
        key = labels[label]
        check(separator and key not in paths, f"invalid or duplicate entry path: {label}")
        value = json.loads(encoded)
        check(isinstance(value, str), f"entry path is not a string: {label}")
        path = Path(value)
        check(path.is_absolute(), f"entry path is not absolute: {label}")
        paths[key] = path
    check(set(paths) == set(labels.values()), f"missing entry paths: {entry}")
    check(physical(paths["state"]), "missing or unsafe State")
    state = json.loads(paths["state"].read_text(encoding="utf-8"))
    base = Path(state["install_dir"])
    expected_host = {".agents": "codex", ".claude": "claude"}.get(discovery.parent.name)
    check(expected_host is not None and state["host"] == expected_host and state["status"] == "installed",
          "invalid installation State or Host")
    suffix = ".exe" if sys.platform == "win32" else ""
    expected = {"bootstrap": base / "skills/shared/bootstrap.md",
                "root": base / "skills" / name / "instructions/instructions.md",
                "skill": base / "bin" / f"skill{suffix}",
                "install": base / "skills", "state": base / "install-state.json"}
    if name == "work":
        expected["work"] = base / "bin" / f"work{suffix}"
    check(paths == expected and physical(base, directory=True), "entry paths do not belong to the same installation")
    check(not os.path.lexists(base / ".agent-work-kit-txn"), "installation has a pending transaction")
    for key, path in paths.items():
        check(physical(path, directory=key == "install"), f"missing or unsafe {key}: {path}")
        for parent in path.parents:
            if parent == base:
                break
            check(physical(parent, directory=True), f"unsafe entry path component: {parent}")
    digest = hashlib.sha256(entry.read_bytes()).hexdigest()
    check(state["files"].count(f"{digest}  host-skills/{name}/SKILL.md") == 1, "entry not in State Manifest")
    check(paths["bootstrap"].read_text(encoding="utf-8").strip(), "empty Bootstrap")
    check(paths["root"].read_text(encoding="utf-8").strip(), "empty Skill Root")
    return paths


def run(work_binary=None, skill_binary=None, skills_dir=None, host_skills_dir=None):
    evidence = {}
    with tempfile.TemporaryDirectory(prefix="host-simulation-") as directory:
        scratch = Path(directory)
        if host_skills_dir is not None:
            check(work_binary is None and skill_binary is None and skills_dir is None,
                  "installed Host simulation must use only entry paths")
            work_paths = read_installed_entry(host_skills_dir, "work")
            tooling_paths = read_installed_entry(host_skills_dir, "tooling")
            check(work_paths["state"] == tooling_paths["state"], "mixed installation entries")
            work_binary, skill_binary, skills_dir = work_paths["work"], work_paths["skill"], work_paths["install"]
            tooling = Cli(tooling_paths["skill"], scratch)
            tooling.ok("discover", "--install", tooling_paths["install"], "--skill", "tooling")
            tooling.ok("resolve", "--install", tooling_paths["install"], "--request", "-",
                       stdin=request(schema="skill-resolve-request", selections=[{"skill": "tooling", "nodes": [""]}]))
            evidence["physical_host_entries"] = ["work", "tooling"]
            evidence["entry_paths_from_non_install_cwd"] = True
        else:
            check(all(value is not None for value in (work_binary, skill_binary, skills_dir)),
                  "source simulation requires explicit binaries and source root")
        work, skill = Cli(work_binary, scratch), Cli(skill_binary, scratch)
        skills_dir = Path(skills_dir).resolve()
        cli_contract_smoke(work, skill, evidence)
        skill.ok("validate", "--install", skills_dir)
        on_demand_loading(skill, skills_dir, scratch, evidence)
        root = str(scratch / ".work")
        requirement_clarification(work, root, evidence)
        task_and_acceptance(work, root, evidence)
        correction_preview(work, root, evidence)
    return evidence


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work", type=Path)
    parser.add_argument("--skill", type=Path)
    parser.add_argument("--install", type=Path, help="managed root holding work, tooling and shared")
    parser.add_argument("--host-skills-dir", type=Path, help="fixed Host discovery directory")
    parser.add_argument("--evidence", type=Path)
    args = parser.parse_args()
    evidence = run(args.work, args.skill, args.install, args.host_skills_dir)
    text = json.dumps(evidence, ensure_ascii=False, indent=2, sort_keys=True)
    if args.evidence:
        args.evidence.parent.mkdir(parents=True, exist_ok=True)
        args.evidence.write_text(text + "\n", encoding="utf-8")
    print(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
