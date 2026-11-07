"""Exercise the independent Work and Tooling CLI boundaries in one Host run."""

import argparse
import hashlib
import json
import shutil
import subprocess
import tempfile
from pathlib import Path

TRACE = []
MODE = None


def invoke(binary, *args):
    result = subprocess.run(
        [str(binary), *map(str, args)], capture_output=True, check=False, timeout=30
    )
    response = json.loads(result.stdout)
    TRACE.append({
        "mode": MODE,
        "binary": binary.name,
        "argv": list(map(str, args)),
        "exit_code": result.returncode,
        "response": response,
    })
    return result.returncode, response


def expect(result, code, schema, reason):
    exit_code, response = result
    assert exit_code == code, response
    assert response["schema"] == schema, response
    assert response["reason_code"] == reason, response
    return response


def policy_node(path, body):
    path.mkdir(parents=True, exist_ok=True)
    (path / "instructions.md").write_text(
        "---\nname: Policy\ndescription: Applicable policy.\n"
        "metadata:\n  tags: [policy]\n---\n" + body,
        encoding="utf-8",
    )


def run(work, tooling, evidence=None):
    global MODE
    with tempfile.TemporaryDirectory(prefix="host-modes-") as directory:
        project = Path(directory)
        repository = Path(__file__).resolve().parents[4]

        def install(mode, name, binary):
            root = project / mode / "installed" / name
            shutil.copytree(repository / "skills" / name, root)
            (root / "scripts").mkdir(exist_ok=True)
            shutil.copy2(binary, root / "scripts" / name)
            return root / "scripts" / name

        work_only_binary = install("work-only", "work", work)
        tooling_only_binary = install("tooling-only", "tooling", tooling)
        joint_work_binary = install("joint", "work", work)
        joint_tooling_binary = install("joint", "tooling", tooling)
        assert not (project / "work-only/installed/tooling").exists()
        assert not (project / "tooling-only/installed/work").exists()
        assert (project / "joint/installed/work").is_dir()
        assert (project / "joint/installed/tooling").is_dir()
        installed = project / "joint/installed"
        skill = project / "policy-fixture"
        skill.mkdir()
        (skill / "SKILL.md").write_text(
            "---\nname: tooling\ndescription: Tool policies.\n---\n",
            encoding="utf-8",
        )
        policy_node(skill / "references/instructions", "Root.\n")
        policy_node(skill / "references/instructions/capabilities", "Capability.\n")
        request = project / "policy-request.json"
        request.write_text(
            json.dumps({
                "selected_paths": ["capabilities"],
                "applicable_references": [],
                "previous": None,
                "semantic_review": {"reviewed": True, "compatible": True},
            }),
            encoding="utf-8",
        )

        MODE = "work-only"
        expect(invoke(work_only_binary, "--help"), 0, "work-cli-result", "ok")

        work_only = expect(
            invoke(work_only_binary, "--project-root", project / "work-only", "workflow", "status",
                   "--requirement-id", "host-mode", "--user-config-root", project / "work-only"),
            0, "work-cli-result", "ok",
        )
        assert work_only["data"]["status"] == "source_required"
        no_formal_task = expect(
            invoke(work_only_binary, "--project-root", project / "work-only", "execute", "record-begin",
                   "--user-config-root", project / "work-only", "--task-path",
                   "outputs/work/tasks/host-mode/index.json",
                   "--execution-dir", "outputs/work/executions/host-mode",
                   "--task-id", "TASK-001", "--record-id", "CMD-001"),
            5, "work-cli-result", "file_not_found",
        )
        assert no_formal_task["status"] == "rejected"
        MODE = "tooling-only"
        expect(invoke(tooling_only_binary, "--help"), 0, "tooling-cli-result", "ok")
        tooling_only = expect(
            invoke(tooling_only_binary, "policy", "discover", "--skill-root", project / "tooling-only/installed/tooling", "--node", "."),
            0, "tooling-cli-result", "ok",
        )
        assert [child["name"] for child in tooling_only["data"]["children"]] == [
            "capabilities", "families", "interfaces"
        ]

        loaded = expect(
            invoke(tooling_only_binary, "policy", "load", "--skill-root", skill,
                   "--input-file", request),
            0, "tooling-cli-result", "ok",
        )
        assert len(loaded["data"]["sources"]) == 2
        assert loaded["data"]["semantic_review"]["origin"] == "host_reported"

        valid_request = request.read_text(encoding="utf-8")
        missing_request = json.loads(valid_request)
        missing_request["selected_paths"] = ["absent"]
        request.write_text(json.dumps(missing_request), encoding="utf-8")
        missing_policy = expect(
            invoke(tooling_only_binary, "policy", "load", "--skill-root", skill,
                   "--input-file", request),
            5, "tooling-cli-result", "policy_source_invalid",
        )
        assert missing_policy["status"] == "rejected"
        request.write_text(valid_request, encoding="utf-8")

        MODE = "joint"
        expect(invoke(joint_work_binary, "--help"), 0, "work-cli-result", "ok")
        expect(invoke(joint_tooling_binary, "--help"), 0, "tooling-cli-result", "ok")
        missing = expect(
            invoke(joint_tooling_binary, "policy", "discover", "--skill-root", skill,
                   "--node", "missing"),
            5, "tooling-cli-result", "node_outside_skill",
        )
        assert missing["status"] == "rejected"
        after_tooling_failure = expect(
            invoke(joint_work_binary, "--project-root", project / "joint", "workflow", "status",
                   "--requirement-id", "host-mode", "--user-config-root", project / "joint"),
            0, "work-cli-result", "ok",
        )
        assert after_tooling_failure["data"] == work_only["data"]

        unknown_work = expect(
            invoke(joint_work_binary, "--project-root", project / "joint", "contract", "describe", "missing"),
            4, "work-cli-result", "unknown_contract_id",
        )
        assert unknown_work["status"] == "rejected"
        after_work_failure = expect(
            invoke(joint_tooling_binary, "policy", "discover", "--skill-root", installed / "tooling", "--node", "."),
            0, "tooling-cli-result", "ok",
        )
        assert after_work_failure["data"] == tooling_only["data"]

        policy_node(skill / "references/instructions/capabilities", "Changed.\n")
        request_data = json.loads(request.read_text(encoding="utf-8"))
        request_data["previous"] = loaded["data"]["summary"]
        request_data["semantic_review"]["compatible"] = False
        request.write_text(json.dumps(request_data), encoding="utf-8")
        drift = expect(
            invoke(joint_tooling_binary, "policy", "load", "--skill-root", skill,
                   "--input-file", request),
            0, "tooling-cli-result", "ok",
        )
        assert drift["data"]["change"]["added_or_invalidated"] == [
            "capabilities/instructions.md"
        ]
        assert drift["data"]["semantic_review"] == {
            "origin": "host_reported",
            "claim": {"reviewed": True, "compatible": False},
        }
        after_drift = expect(
            invoke(joint_work_binary, "--project-root", project / "joint", "workflow", "status",
                   "--requirement-id", "host-mode", "--user-config-root", project / "joint"),
            0, "work-cli-result", "ok",
        )
        assert after_drift["data"] == work_only["data"]
        summary = {
            "work_only": work_only["reason_code"],
            "work_without_formal_task": no_formal_task["reason_code"],
            "tooling_only": tooling_only["reason_code"],
            "joint_tooling_failure": missing["reason_code"],
            "missing_policy": missing_policy["reason_code"],
            "joint_work_after_tooling_failure": after_tooling_failure["reason_code"],
            "joint_work_failure": unknown_work["reason_code"],
            "joint_tooling_after_work_failure": after_work_failure["reason_code"],
            "policy_drift": drift["data"]["change"]["added_or_invalidated"],
            "policy_conflict_origin": drift["data"]["semantic_review"]["origin"],
            "policy_conflict_compatible": drift["data"]["semantic_review"]["claim"]["compatible"],
        }
        if evidence is not None:
            evidence.write_text(
                json.dumps({
                    "fixture_root": str(project),
                    "installed_modes": {
                        mode: sorted(path.name for path in (project / mode / "installed").iterdir())
                        for mode in ("work-only", "tooling-only", "joint")
                    },
                    "runtime_sha256": {
                        name: hashlib.sha256((installed / name / "scripts" / name).read_bytes()).hexdigest()
                        for name in ("work", "tooling")
                    },
                    "bootstrap_sha256": {
                        name: hashlib.sha256(
                            (installed / name / "references/instruction-loading.md").read_bytes()
                        ).hexdigest()
                        for name in ("work", "tooling")
                    },
                    "fixture_inputs": {
                        "skill_entrypoints": [
                            "work-only/installed/work/SKILL.md",
                            "tooling-only/installed/tooling/SKILL.md",
                            "joint/installed/work/SKILL.md",
                            "joint/installed/tooling/SKILL.md",
                        ],
                        "initial_policy_root": "Root.\n",
                        "initial_policy_capability": "Capability.\n",
                        "initial_request": json.loads(valid_request),
                        "drifted_policy_capability": "Changed.\n",
                        "drifted_request": request_data,
                    },
                    "commands": TRACE,
                    "summary": summary,
                }, indent=2, ensure_ascii=False) + "\n",
                encoding="utf-8",
            )
        print(json.dumps(summary, sort_keys=True))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--work", type=Path, required=True)
    parser.add_argument("--tooling", type=Path, required=True)
    parser.add_argument("--evidence", type=Path)
    arguments = parser.parse_args()
    run(arguments.work.resolve(), arguments.tooling.resolve(), arguments.evidence)
