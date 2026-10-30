use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

fn fixture() -> (PathBuf, Value) {
    let root = std::env::temp_dir().join(format!(
        "tooling-operation-cli-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    fs::write(root.join("source.rs"), "fn main() {}\n").unwrap();
    let request = json!({
        "policy_state":"valid",
        "semantic_review":{
            "policy_reviewed":true,
            "policy_compatible":true,
            "fitness_reviewed":true,
            "tool_fit":true
        },
        "observed_tools":[{
            "name":"analyzer",
            "interfaces":["mcp"],
            "capabilities":["code-analysis"],
            "supported_operations":["find-references"],
            "project_root":root,
            "session":null
        }],
        "need":{
            "operation":"find-references",
            "interface":"mcp",
            "capability":"code-analysis",
            "project_root":root,
            "target":root.join("source.rs"),
            "session":null
        },
        "effects":[{"kind":"read"}],
        "authorization":{
            "writes":false,
            "external_recipients":[],
            "data_recipients":[]
        }
    });
    (root, request)
}

fn evaluate(root: &Path, request: &Value) -> (i32, Value) {
    let input = root.join("request.json");
    fs::write(&input, serde_json::to_vec(request).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_tooling"))
        .args([
            "operation",
            "evaluate",
            "--input-file",
            input.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["schema"], "tooling-cli-result");
    (output.status.code().unwrap(), result)
}

#[test]
fn operation_evaluate_requires_host_observation_and_review() {
    let (root, mut request) = fixture();
    let source = fs::read(root.join("source.rs")).unwrap();
    let (code, ready) = evaluate(&root, &request);
    assert_eq!(code, 0);
    assert_eq!(ready["data"]["decision"], "continue");
    assert_eq!(ready["reason_code"], "operation_ready");

    request["policy_state"] = json!("unknown");
    let (_, drift) = evaluate(&root, &request);
    assert_eq!(drift["data"]["decision"], "needs_inspection");
    assert_eq!(drift["reason_code"], "policy_unverified");
    request["policy_state"] = json!("valid");
    request["semantic_review"]["policy_compatible"] = json!(false);
    let (_, conflict) = evaluate(&root, &request);
    assert_eq!(conflict["data"]["decision"], "stop");
    assert_eq!(conflict["reason_code"], "policy_conflict");
    request["semantic_review"]["policy_compatible"] = json!(true);
    request["effects"] = json!([{"kind":"write"}]);
    let (_, unauthorized) = evaluate(&root, &request);
    assert_eq!(unauthorized["data"]["decision"], "stop");
    assert_eq!(unauthorized["reason_code"], "effects_unauthorized");
    request["effects"] = json!([{"kind":"unknown"}]);
    let (_, unknown) = evaluate(&root, &request);
    assert_eq!(unknown["data"]["decision"], "needs_inspection");
    request["effects"] = json!([{"kind":"read"}]);
    request["observed_tools"] = json!([]);
    let (_, missing_tool) = evaluate(&root, &request);
    assert_eq!(missing_tool["reason_code"], "tool_unobserved");
    assert_eq!(fs::read(root.join("source.rs")).unwrap(), source);
    assert!(!root.join("install-state.json").exists());
}

#[test]
fn operation_evaluate_blocks_unknown_outcome_and_invalid_input() {
    let (root, mut request) = fixture();
    request["outcome"] = json!("unknown");
    let (_, uninspected) = evaluate(&root, &request);
    assert_eq!(uninspected["data"]["decision"], "needs_inspection");
    assert_eq!(uninspected["reason_code"], "effects_inspection_required");
    request["reconciliation"] = json!({"effects_inspected":true,"unresolved_effects":true});
    let (_, unresolved) = evaluate(&root, &request);
    assert_eq!(unresolved["data"]["decision"], "stop");
    assert_eq!(unresolved["reason_code"], "effects_unresolved");
    request.as_object_mut().unwrap().remove("need");
    let (code, missing) = evaluate(&root, &request);
    assert_eq!(code, 3);
    assert_eq!(missing["reason_code"], "invalid_request_json");
}
