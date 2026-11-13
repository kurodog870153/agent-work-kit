"""Exercise the installed Tooling CLI using only public Skill resources."""

import argparse
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


def run_tooling_public_smoke(skill_root):
    skill_root = Path(skill_root)
    binary = skill_root / "scripts" / ("tooling.exe" if sys.platform == "win32" else "tooling")
    contract = skill_root / "references/cli-contract.md"
    if not contract.is_file():
        raise AssertionError("installed Tooling omitted its public contract")
    public_text = contract.read_text(encoding="utf-8")
    if not all(term in public_text for term in ("policy discover", "policy load", "operation evaluate", "previous", "authorization_verified")):
        raise AssertionError("installed Tooling public contract is incomplete")

    def invoke(*args, expected=0):
        result = subprocess.run([str(binary), *map(str, args)], capture_output=True, timeout=30)
        response = json.loads(result.stdout)
        if result.returncode != expected or response.get("schema") != "tooling-cli-result":
            raise AssertionError(f"Tooling CLI failed: {args!r}, {result.returncode}, {response!r}")
        return response

    for args in (("--help",), ("contract", "example", "--help"), ("policy", "discover", "--help"),
                 ("policy", "load", "--help"), ("operation", "evaluate", "--help")):
        if "help" not in invoke(*args)["data"]:
            raise AssertionError(f"Tooling help missing: {args!r}")

    def example(name):
        if name.startswith("policy-load-"):
            contract_id, scenario = "tooling-policy-load-request", name.removeprefix("policy-load-")
        elif name.startswith("operation-evaluate-"):
            contract_id, scenario = "tooling-operation-evaluate-request", name.removeprefix("operation-evaluate-")
        else:
            contract_id, scenario = "tooling-operation-evaluate-request", "valid"
        return invoke("contract", "example", contract_id, "--scenario", scenario)["data"]["example"]

    for contract_id in ("tooling-policy-discover-response", "tooling-policy-load-response",
                        "tooling-operation-evaluate-response"):
        response = invoke("contract", "example", contract_id, "--scenario", "default")["data"]["example"]
        if not isinstance(response, dict):
            raise AssertionError(f"invalid public response example: {contract_id}")
    for scenario in ("success", "error"):
        response = invoke("contract", "example", "tooling-cli-result", "--scenario", scenario)["data"]["example"]
        if response["schema"] != "tooling-cli-result" or not isinstance(response["data"], dict):
            raise AssertionError(f"invalid public envelope example: {scenario}")
    if invoke("contract", "example", "tooling-cli-result", "--scenario", "unknown", expected=4)["reason_code"] != "unknown_scenario":
        raise AssertionError("unknown scenario was accepted")
    index = invoke("contract", "list")["data"]
    if not index["commands"] or not index["contracts"]:
        raise AssertionError("installed Tooling contract index is empty")
    if invoke("contract", "describe", "tooling-policy-load-request")["data"].get("example") is not None:
        raise AssertionError("default description embedded a full example")
    if "previous" not in invoke("contract", "scaffold", "tooling-policy-load-request")["data"]["scaffold"]:
        raise AssertionError("required nullable field was omitted")

    with tempfile.TemporaryDirectory() as directory:
        fixture = Path(directory) / "project"
        fixture.mkdir()
        target = fixture / "existing.txt"
        target.write_text("existing", encoding="utf-8")
        request_file = fixture / "request.json"

        def request(command, payload, expected=0):
            request_file.write_text(json.dumps(payload), encoding="utf-8")
            return invoke(*command, "--input-file", request_file, expected=expected)

        policy_command = ("policy", "load", "--skill-root", skill_root)
        invalid_policy = example("policy-load-invalid-missing-previous")
        if request(policy_command, invalid_policy, expected=3)["reason_code"] != "invalid_request_json":
            raise AssertionError("missing previous was accepted")
        discovered = invoke("policy", "discover", "--skill-root", skill_root, "--node", ".")
        if discovered["data"]["node"] != "." or not discovered["data"]["children"]:
            raise AssertionError("Tooling root discovery failed")
        first = example("policy-load-first")
        loaded = request(policy_command, first)
        if loaded["status"] != "success" or not loaded["data"]["summary"]["sources"]:
            raise AssertionError("first public policy load failed")
        if loaded["data"]["semantic_review"]["claim"]["compatible"] is not False:
            raise AssertionError("Host-reported incompatible claim was transformed")
        host_allows_first = loaded["status"] == "success" and loaded["data"]["semantic_review"]["claim"]["compatible"] is True
        if host_allows_first:
            raise AssertionError("incompatible Host claim allowed a dependent operation")
        reload_request = example("policy-load-reload")
        reload_request["previous"] = loaded["data"]["summary"]
        unchanged = request(policy_command, reload_request)
        if unchanged["data"]["change"] != {"added_or_invalidated": [], "removed": []}:
            raise AssertionError("unchanged policy reload reported a change")
        changed_skill = Path(directory) / "changed-tooling"
        shutil.copytree(skill_root, changed_skill)
        node = changed_skill / "references/instructions/capabilities/code-analysis/ast/instructions.md"
        node.write_text(node.read_text(encoding="utf-8") +
                        "\n## References\n\n| Reference | Load condition |\n| --- | --- |\n| `references/extra.md` | Simulated new requirement. |\n",
                        encoding="utf-8")
        reference = node.parent / "references/extra.md"
        reference.parent.mkdir()
        reference.write_text("---\nname: Extra\ndescription: Simulated applicable policy.\nmetadata:\n  tags: [extra-policy]\n---\nAdditional rule.\n", encoding="utf-8")
        reload_request["applicable_references"] = ["capabilities/code-analysis/ast/references/extra.md"]
        changed = request(("policy", "load", "--skill-root", changed_skill), reload_request)
        changed_paths = changed["data"]["change"]["added_or_invalidated"]
        if set(changed_paths) != {"capabilities/code-analysis/ast/instructions.md",
                                  "capabilities/code-analysis/ast/references/extra.md"}:
            raise AssertionError(f"source and Reference change were not detected: {changed_paths!r}")
        reload_request["previous"] = changed["data"]["summary"]
        reload_request["applicable_references"] = []
        removed_reference = request(("policy", "load", "--skill-root", changed_skill), reload_request)
        if removed_reference["data"]["change"]["removed"] != ["capabilities/code-analysis/ast/references/extra.md"]:
            raise AssertionError("Reference applicability removal was not detected")
        operation = example("operation-evaluate")
        invalid_operation = example("operation-evaluate-invalid-missing-target")
        invalid_operation["project_root"] = str(fixture)
        if request(("operation", "evaluate"), invalid_operation, expected=3)["reason_code"] != "invalid_request_json":
            raise AssertionError("missing target was accepted")
        operation.update(project_root=str(fixture), target=str(target))
        valid = request(("operation", "evaluate"), operation)
        if valid["data"]["target_valid"] is not True or any(valid["data"][key] is not False for key in (
                "authorization_verified", "tool_fit_verified", "outcome_verified")):
            raise AssertionError("target validation claimed Host verification")
        operation["target"] = str(fixture / "missing.txt")
        if request(("operation", "evaluate"), operation, expected=4)["reason_code"] != "target_boundary_invalid":
            raise AssertionError("missing target was accepted")
        operation["target"] = str(fixture.parent / "outside.txt")
        (fixture.parent / "outside.txt").write_text("outside", encoding="utf-8")
        if request(("operation", "evaluate"), operation, expected=4)["reason_code"] != "target_boundary_invalid":
            raise AssertionError("outside target was accepted")
        return {"first_sources": len(loaded["data"]["summary"]["sources"]),
                "unchanged_change": unchanged["data"]["change"],
                "changed_paths": changed_paths,
                "removed_references": removed_reference["data"]["change"]["removed"],
                "changed_sources": len(changed["data"]["summary"]["sources"]),
                "target_boundary": valid["reason_code"], "incompatible_policy_blocked": not host_allows_first}




def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--skill-root", required=True, type=Path)
    args = parser.parse_args()
    print(json.dumps(run_tooling_public_smoke(args.skill_root)))


if __name__ == "__main__":
    main()
