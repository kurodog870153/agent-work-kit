"""Exercise public Work contracts and TASK publication with an installed CLI."""

import io
import json
import os
import shutil
import subprocess
import tempfile


def run_work_public_smoke():
    REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
    ROOT = tempfile.mkdtemp(prefix="task-publication-acceptance-")
    WORK = os.environ.get("WORK_BINARY")
    INSTALLED_BINARY = WORK is not None
    if WORK is None:
        installed = os.path.join(ROOT, "skills", "work")
        shutil.copytree(os.path.join(REPO, "skills", "work"), installed)
        binary_name = "work.exe" if os.name == "nt" else "work"
        source_binary = os.path.join(REPO, "rust", "target", "debug", binary_name)
        WORK = os.path.join(installed, "scripts", binary_name)
        os.makedirs(os.path.dirname(WORK), exist_ok=True)
        shutil.copy2(source_binary, WORK)
    WORK = os.path.abspath(WORK)

    # The authorization root is the already-resolved project root the CLI sees.
    CANONICAL = os.path.realpath(ROOT)
    if os.name == "nt":
        CANONICAL = chr(92) + chr(92) + "?" + chr(92) + CANONICAL

    lines = []


    def say(text=""):
        lines.append(text)


    def cli(*args, user_config=True):
        arguments = [WORK, "--project-root", ROOT, "--verbose", *args]
        if user_config:
            arguments.extend(["--user-config-root", ROOT])
        process = subprocess.run(arguments, capture_output=True)
        try:
            payload = json.loads(process.stdout.decode("utf-8"))
        except json.JSONDecodeError:
            payload = {"raw": process.stdout.decode("utf-8", "replace"),
                       "stderr": process.stderr.decode("utf-8", "replace")}
        return process.returncode, payload


    def contract(contract_id):
        code, payload = cli("contract", "describe", contract_id, user_config=False)
        assert code == 0, payload
        return payload["data"]


    def branch_example(name):
        request = contract("work-discussion-request")
        command = next(field for field in request["fields"] if field["name"] == "command")
        return command["constraints"]["schema"]["$defs"][name]["examples"][0]


    def selection(name, command, request):
        path = os.path.join(ROOT, name + ".json")
        io.open(path, "w", encoding="utf-8", newline="\n").write(json.dumps(request) + "\n")
        code, result = cli(*command, "--input-file", path, user_config=False)
        assert code == 0, result
        say(f"$ work --project-root <root> {' '.join(command)} --input-file {name}.json")
        say(f"# exit {code}")
        say(json.dumps(result, ensure_ascii=False))
        return result["data"]


    def write_request(name, command):
        path = os.path.join(ROOT, name)
        io.open(path, "w", encoding="utf-8", newline="\n").write(
            json.dumps({"schema": "work-discussion-request",
                        "requirement_id": "example",
                        "command": command},
                       ensure_ascii=False, indent=2) + "\n")
        return path


    def summarize(payload):
        data = payload.get("data", {})
        if isinstance(data, dict) and isinstance(data.get("data"), dict):
            data = data["data"]
        keep = {}
        for key in ("saved", "operation_revision", "revision", "content_sha256",
                    "source_id", "schema", "status", "reason_code", "changed",
                    "approval_sha256", "task_count", "confirmed", "remaining"):
            if key in data:
                keep[key] = data[key]
        if isinstance(data, dict) and isinstance(data.get("view"), dict):
            view = data["view"]
            keep["view.revision"] = view.get("revision")
            keep["view.content_sha256"] = view.get("content_sha256")
        return keep


    say("# Issue 120 acceptance scenarios")
    say()
    say("Executed with the installed `work` binary against a fresh empty project."
        if INSTALLED_BINARY else
        "Executed with a temporary Work Skill and the locally built `work` binary against a fresh empty project.")
    say("Every request below was built from `contract describe` output and the")
    say("published examples; no Rust private structure was read.")
    say()
    say("## Public contract checks")
    say()
    say("```console")
    for command in (("contract", "list"),
                    ("contract", "describe", "work-discussion-request"),
                    ("contract", "scaffold", "work-source-capture-request")):
        code, result = cli(*command, user_config=False)
        assert code == 0, result
        say("$ work --project-root <root> " + " ".join(command))
        say(f"# exit {code}")
        say(json.dumps(result, ensure_ascii=False))
    say("```")
    say()

    # ---- Scenario 1: a brand new requirement -------------------------------
    say("## Scenario 1: a brand new requirement")
    say()
    say("### 1.1 Capture the original requirement as an immutable Source")
    say()
    capture_example = contract("work-source-capture-request")["example"]
    metadata = dict(capture_example)
    metadata["requirement_id"] = "example"
    metadata["source"] = {"kind": "file", "path": "request.txt", "media_type": "text/plain"}
    metadata["content_path"] = "source.txt"
    metadata_path = os.path.join(ROOT, "capture.json")
    payload_path = os.path.join(ROOT, "request.txt")
    io.open(payload_path, "w", encoding="utf-8", newline="\n").write(
        "Build the export feature.\n")
    io.open(metadata_path, "w", encoding="utf-8", newline="\n").write(
        json.dumps(metadata, ensure_ascii=False, indent=2) + "\n")
    say("Source of the request shape: `contract describe work-source-capture-request` → `example`.")
    say()
    say("```console")
    say("$ work --project-root <root> source capture --input-file capture.json --payload-file request.txt")
    code, payload = cli("source", "capture", "--input-file", metadata_path,
                        "--payload-file", payload_path, user_config=False)
    say(f"# exit {code}")
    say(json.dumps({"status": payload.get("status"),
                    "reason_code": payload.get("reason_code"),
                    "data": {"approval_sha256": payload.get("data", {}).get("approval_sha256")}},
                   ensure_ascii=False))
    approval = payload["data"]["approval_sha256"]
    say("$ work --project-root <root> source capture --input-file capture.json --payload-file request.txt \\")
    say(f"    --approved-sha256 {approval}")
    code, payload = cli("source", "capture", "--input-file", metadata_path,
                        "--payload-file", payload_path, "--approved-sha256", approval,
                        user_config=False)
    say(f"# exit {code}")
    say(json.dumps({"status": payload.get("status"),
                    "reason_code": payload.get("reason_code"),
                    "data": {"schema": payload.get("data", {}).get("schema"),
                             "source_id": payload.get("data", {}).get("source_id")}},
                   ensure_ascii=False))
    say("```")
    say()

    say("### 1.2 Open the Discussion at revision 1")
    say()
    init_example = None
    request_contract = contract("work-discussion-request")
    for field in request_contract["fields"]:
        if field["name"] == "command":
            init_example = field["constraints"]["schema"]["$defs"]["InitBranch"]["examples"][0]
    say("Source of the request shape: `contract describe work-discussion-request` →")
    say("`command.constraints.schema.$defs.InitBranch.examples[0]`.")
    say("The caller supplies semantics plus `commit.operation_id`; `operation_sha256`")
    say("and `content_sha256` are derived by the CLI (see `decision-init-integrity.md`).")
    say()
    init_example["session"]["authorization"]["project_root"] = CANONICAL
    init_path = write_request("init.json", init_example)
    say("```console")
    say("$ work --project-root <root> discussion init --input-file init.json")
    code, payload = cli("discussion", "init", "--input-file", init_path)
    say(f"# exit {code}")
    say(json.dumps({"status": payload.get("status"), "data": summarize(payload)},
                   ensure_ascii=False))
    say("```")
    say()
    session_sha = payload["data"]["data"]["view"]["content_sha256"]
    say(f"Committed `content_sha256`: `{session_sha}` (revision 1, derived by the CLI).")
    say()

    say("### 1.3 Continue with a decision and a saved question")
    say()
    say("Source of the request shape: the `UpdateBranch` and `QuestionBranch` examples.")
    say()
    view = payload["data"]["data"]["view"]
    decision = {
        "id": "D001", "question": "Which export format?", "question_version": 1,
        "options": [{"id": "csv", "label": "CSV", "explanation": "Portable"},
                    {"id": "json", "label": "JSON", "explanation": "Structured"}],
        "status": "pending", "tentative_option_id": None, "resolution": None,
        "status_reason": "", "status_evidence": "",
        "added_reason": "Format must be confirmed before planning",
        "task_ids": [], "dependencies": [], "source_references": ["source.txt"],
    }
    update_path = write_request("update.json", {
        "kind": "update",
        "operation": {"operation_id": "add-d001", "expected_revision": view["revision"],
                      "previous_sha256": view["content_sha256"],
                      "change": {"action": "add_decision", "decision": decision}}})
    say("```console")
    say("$ work --project-root <root> discussion update --input-file update.json")
    code, payload = cli("discussion", "update", "--input-file", update_path)
    say(f"# exit {code}")
    say(json.dumps({"status": payload.get("status"), "data": summarize(payload)},
                   ensure_ascii=False))
    say("```")
    say()
    view = payload["data"]["data"]["view"]

    question_path = write_request("question.json", {
        "kind": "question",
        "operation": {"operation_id": "ask-d001", "expected_revision": view["revision"],
                      "previous_sha256": view["content_sha256"],
                      "change": {"action": "prepare_question",
                                 "question": {"decision_id": "D001", "question_version": 1,
                                              "text": "Which export format?",
                                              "display_options": {"1": "csv", "2": "json"}}}}})
    say("```console")
    say("$ work --project-root <root> discussion question --input-file question.json")
    code, payload = cli("discussion", "question", "--input-file", question_path)
    say(f"# exit {code}")
    say(json.dumps({"status": payload.get("status"), "data": summarize(payload)},
                   ensure_ascii=False))
    say("```")
    say()
    view = payload["data"]["data"]["view"]
    question = view["continuation"]["question"]
    say(f"The saved question is displayed as numbers: {json.dumps(question['display_options'])}.")
    say("The answer uses the saved option number and question version, never an option ID.")
    say()
    answer_path = write_request("answer.json", {
        "kind": "answer",
        "operation": {"operation_id": "answer-d001", "expected_revision": view["revision"],
                      "previous_sha256": view["content_sha256"],
                      "change": {"action": "answer_question", "decision_id": "D001",
                                 "question_version": question["question_version"],
                                 "display_number": "1",
                                 "rationale": "CSV is portable",
                                 "evidence": "User selected 1"}}})
    say("```console")
    say("$ work --project-root <root> discussion answer --input-file answer.json")
    code, payload = cli("discussion", "answer", "--input-file", answer_path)
    say(f"# exit {code}")
    say(json.dumps({"status": payload.get("status"), "data": summarize(payload)},
                   ensure_ascii=False))
    say("```")
    say()
    view = payload["data"]["data"]["view"]
    say(f"Progress after the answer: confirmed={view['progress']['confirmed']}, "
        f"remaining={view['progress']['remaining']}.")
    say()

    say("### 1.4 Preview and apply the formal TASK")
    say()
    say("`task preview` and `task apply` take the same `work-discussion-request` shape,")
    say("with kind `preview` / `apply` (see `constraints.trigger_paths`).")
    say()
    say("Source of planning shapes: `contract describe work-task-index` and")
    say("`contract describe work-discussion-request` → `UpdatePlanningChange.examples[0]`.")
    say("Selection fingerprints are returned by the public selection-build commands.")
    say()
    say("```console")
    hierarchy = selection("hierarchy-build", ("hierarchy", "selection-build"),
                          {"decision": "general_only", "selections": []})
    skills = selection("skills-build", ("skills", "selection-build"),
                       {"decision": "base_only", "skills": []})
    say("```")
    say()
    index_example = contract("work-task-index")["example"]
    manifest_path = os.path.join(ROOT, "outputs", "work", "sources", "example", "SRC-001", "manifest.json")
    manifest = json.load(io.open(manifest_path, encoding="utf-8"))
    full_session = None
    history_path = write_request("planning-history.json", {"kind": "history", "revision": view["revision"]})
    code, history = cli("discussion", "history", "--input-file", history_path)
    assert code == 0, history
    full_session = history["data"]["data"]
    context = full_session["context"]
    context["revision"] += 1
    context["confirmed_source"] = {
        "snapshot": manifest,
        "artifacts": index_example["artifacts"],
        "hierarchy_selection": hierarchy,
        "skill_selection": skills,
        "acceptance_criteria": context["acceptance_criteria"],
    }
    context_path = write_request("confirm-source.json", {
        "kind": "update", "operation": {
            "operation_id": "confirm-source", "expected_revision": view["revision"],
            "previous_sha256": view["content_sha256"],
            "change": {"action": "update_context", "context": context}}})
    say("```console")
    say("$ work --project-root <root> discussion update --input-file confirm-source.json")
    code, result = cli("discussion", "update", "--input-file", context_path)
    say(f"# exit {code}")
    say(json.dumps(result, ensure_ascii=False))
    assert code == 0, result
    view = result["data"]["data"]["view"]
    history_path = write_request("confirmed-history.json", {"kind": "history", "revision": view["revision"]})
    code, history = cli("discussion", "history", "--input-file", history_path)
    assert code == 0, history
    decision = history["data"]["data"]["decisions"][0]
    decision["status"] = "confirmed"
    decision["status_reason"] = "Source and scope were reconfirmed"
    decision["status_evidence"] = "User confirmed the CSV choice against the captured Source"
    reconfirm_path = write_request("reconfirm-decision.json", {
        "kind": "update", "operation": {
            "operation_id": "reconfirm-d001", "expected_revision": view["revision"],
            "previous_sha256": view["content_sha256"],
            "change": {"action": "update_decision", "decision": decision}}})
    say("$ work --project-root <root> discussion update --input-file reconfirm-decision.json")
    code, result = cli("discussion", "update", "--input-file", reconfirm_path)
    say(f"# exit {code}")
    say(json.dumps(result, ensure_ascii=False))
    assert code == 0, result
    view = result["data"]["data"]["view"]
    task = branch_example("UpdatePlanningChange")["task"]
    task["title"] = "Deliver CSV export"
    task["goal"] = "Produce and verify a CSV export"
    task["acceptance_criteria"][0]["id"] = "TASK-001-ACCEPTANCE-001"
    task["decision_ids"] = []
    task["review"]["decision_versions"] = {}
    task["review"]["context_revision"] = context["revision"]
    task["scope"] = ["src/export.ts"]
    task["skill_id"] = None
    task["inputs"] = []
    task["files"] = []
    task["commands"] = []
    task["steps"] = [{"key": "verify", "action": "Review the CSV export outcome",
                      "references": [{"kind": "validations", "key": "verify"}]}]
    task["validations"] = [{"key": "verify", "kind": "manual",
        "acceptance_ids": ["ACCEPTANCE-001", "TASK-001-ACCEPTANCE-001"],
        "criteria": "The CSV export outcome is observable", "confirmer": "User"}]
    code, selected = cli("instructions", "select", "--mode", "task", "--reference",
                         "task.general.task-records", user_config=False)
    assert code == 0, selected
    say("$ work --project-root <root> instructions select --mode task --reference task.general.task-records")
    say(f"# exit {code}")
    say(json.dumps(selected, ensure_ascii=False))
    instruction_selection = selected["data"]["instruction_selection"]
    task["instruction_selection"] = {
        "selected_paths": instruction_selection["selected_paths"],
        "references": instruction_selection["references"],
    }
    task["instructions_sha256"] = instruction_selection["instructions_sha256"]
    planning_path = write_request("planning.json", {
        "kind": "update", "operation": {
            "operation_id": "plan-task-001", "expected_revision": view["revision"],
            "previous_sha256": view["content_sha256"],
            "change": {"action": "update_planning", "task": task}}})
    say("$ work --project-root <root> discussion update --input-file planning.json")
    code, result = cli("discussion", "update", "--input-file", planning_path)
    say(f"# exit {code}")
    say(json.dumps(result, ensure_ascii=False))
    assert code == 0, result
    view = result["data"]["data"]["view"]
    say("```")
    say()
    metadata_record = {"title": "Export feature", "summary": "One verified export outcome"}
    preview_path = write_request("preview.json", {"kind": "preview", "metadata": metadata_record})
    say("```console")
    say("$ work --project-root <root> task preview --input-file preview.json")
    code, payload = cli("task", "preview", "--input-file", preview_path)
    say(f"# exit {code}")
    say(json.dumps(payload, ensure_ascii=False))
    assert code == 0, payload
    preview = payload["data"]["data"]
    apply_path = write_request("apply.json", {"kind": "apply",
        "expected_revision": view["revision"], "session_sha256": view["content_sha256"],
        "metadata": metadata_record, "approved_sha256": preview["approval_sha256"],
        "publication_evidence": "User approved the complete preview"})
    say("$ work --project-root <root> task apply --input-file apply.json")
    code, payload = cli("task", "apply", "--input-file", apply_path)
    say(f"# exit {code}")
    say(json.dumps(payload, ensure_ascii=False))
    assert code == 0, payload
    say("$ work --project-root <root> task validate --path outputs/work/tasks/example/index.json")
    code, payload = cli("task", "validate", "--path", "outputs/work/tasks/example/index.json")
    say(f"# exit {code}")
    say(json.dumps(payload, ensure_ascii=False))
    assert code == 0, payload
    say("```")
    say()
    say("The approved TASK was published and independently validated.")
    say()

    # ---- Scenario 2: resuming a saved discussion ---------------------------
    say("## Scenario 2: resuming a saved discussion")
    say()
    say("### 2.1 Read the committed Session from a fresh process")
    say()
    say("```console")
    say("$ work --project-root <root> discussion read --input-file read.json")
    read_path = write_request("read.json", {"kind": "read"})
    code, payload = cli("discussion", "read", "--input-file", read_path)
    say(f"# exit {code}")
    say(json.dumps({"status": payload.get("status"), "data": summarize(payload)},
                   ensure_ascii=False))
    say("```")
    say()
    view = payload["data"]["data"]
    say(f"Recovered: revision {view['revision']}, content SHA `{view['content_sha256']}`,")
    say(f"`pending_decision_ids` {json.dumps(view['pending_decision_ids'])} and")
    say(f"`progress` {json.dumps(view['progress'])}.")
    say()
    say("### 2.2 Status uses the same committed state")
    say()
    status_path = write_request("status.json", {"kind": "status"})
    say("```console")
    say("$ work --project-root <root> discussion status --input-file status.json")
    code, payload = cli("discussion", "status", "--input-file", status_path)
    say(f"# exit {code}")
    say(json.dumps({"status": payload.get("status"), "data": summarize(payload)},
                   ensure_ascii=False))
    say("```")
    say()
    status_view = payload["data"]["data"]
    assert status_view["content_sha256"] == view["content_sha256"]
    say("Both commands returned the identical `content_sha256`, so `discussion read`")
    say("and `discussion status` are two views of the same committed revision.")
    say("Nothing here depends on the conversation: a fresh process reached the same")
    say("revision from `outputs/work/discussions/example/session.json` alone.")
    say()
    say("### 2.3 Immutable history still holds the earlier revision")
    say()
    history_path = write_request("history.json", {"kind": "history", "revision": 1})
    say("```console")
    say("$ work --project-root <root> discussion history --input-file history.json")
    code, payload = cli("discussion", "history", "--input-file", history_path)
    say(f"# exit {code}")
    first = payload["data"]["data"]
    say(json.dumps({"status": payload.get("status"),
                    "revision": first["revision"],
                    "goal": first["context"]["goal"],
                    "decisions": len(first["decisions"])}, ensure_ascii=False))
    say("```")
    say()
    say("Revision 1 still has no decisions; the decision added later lives only in")
    say("later revisions. History is never rewritten.")

    report = os.environ.get("SCENARIO_REPORT") or os.path.join(
        REPO, "rust", "target", "task-publication-acceptance.md")
    io.open(report, "w", encoding="utf-8", newline="\n").write("\n".join(lines) + "\n")
    print("\n".join(lines[-12:]))
    print("\nwritten")


if __name__ == "__main__":
    run_work_public_smoke()
