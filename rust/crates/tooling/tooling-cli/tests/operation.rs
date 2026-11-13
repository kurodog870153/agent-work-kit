use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

#[test]
fn public_operation_example_matches_the_cli_request_contract() {
    let output = Command::new(env!("CARGO_BIN_EXE_tooling"))
        .args([
            "contract",
            "example",
            "tooling-operation-evaluate-request",
            "--scenario",
            "valid",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    let mut request = response["data"]["example"].clone();
    let (root, fixture_request) = fixture();
    request["project_root"] = fixture_request["project_root"].clone();
    request["target"] = fixture_request["target"].clone();
    assert_eq!(evaluate(&root, &request).0, 0);
    request["extra"] = json!(true);
    assert_eq!(
        evaluate(&root, &request).1["reason_code"],
        "invalid_request_json"
    );
    request.as_object_mut().unwrap().remove("extra");
    let invalid_output = Command::new(env!("CARGO_BIN_EXE_tooling"))
        .args([
            "contract",
            "example",
            "tooling-operation-evaluate-request",
            "--scenario",
            "invalid-missing-target",
        ])
        .output()
        .unwrap();
    let invalid_example: Value = serde_json::from_slice(&invalid_output.stdout).unwrap();
    let mut invalid_request = invalid_example["data"]["example"].clone();
    invalid_request["project_root"] = fixture_request["project_root"].clone();
    assert_eq!(
        evaluate(&root, &invalid_request).1["reason_code"],
        "invalid_request_json"
    );
    request["target"] = json!(root.join("missing.txt"));
    assert_eq!(
        evaluate(&root, &request).1["reason_code"],
        "target_boundary_invalid"
    );
    let project = root.join("subproject");
    fs::create_dir(&project).unwrap();
    request["project_root"] = json!(project);
    request["target"] = json!(root.join("source.rs"));
    assert_eq!(
        evaluate(&root, &request).1["reason_code"],
        "target_boundary_invalid"
    );
    #[cfg(unix)]
    {
        let escaped = project.join("escaped.txt");
        std::os::unix::fs::symlink(root.join("source.rs"), &escaped).unwrap();
        request["target"] = json!(escaped);
        assert_eq!(
            evaluate(&root, &request).1["reason_code"],
            "target_boundary_invalid"
        );
    }
}

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
    let request = json!({"project_root":root,"target":root.join("source.rs")});
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
    assert_eq!(
        result["execution"]["host"]["rust_workspace_version"],
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(result["schema"], "tooling-cli-result");
    (output.status.code().unwrap(), result)
}

#[test]
fn operation_evaluate_only_validates_the_real_target_boundary() {
    let (root, mut request) = fixture();
    let source = fs::read(root.join("source.rs")).unwrap();
    let (code, valid) = evaluate(&root, &request);
    assert_eq!(code, 0);
    assert_eq!(valid["reason_code"], "target_valid");
    assert_eq!(valid["data"]["target_valid"], true);
    assert_eq!(valid["data"]["authorization_verified"], false);
    assert_eq!(valid["data"]["tool_fit_verified"], false);
    assert_eq!(valid["data"]["outcome_verified"], false);

    request["target"] = json!(root.join("missing.rs"));
    let (code, invalid) = evaluate(&root, &request);
    assert_eq!(code, 4);
    assert_eq!(invalid["reason_code"], "target_boundary_invalid");
    assert_eq!(fs::read(root.join("source.rs")).unwrap(), source);
}

#[test]
fn legacy_host_claims_cannot_become_a_cli_continue_decision() {
    let (root, mut request) = fixture();
    request["policy_state"] = json!("valid");
    request["authorization"] = json!({"writes":true});
    let (code, retired) = evaluate(&root, &request);
    assert_eq!(code, 4);
    assert_eq!(retired["reason_code"], "operation_contract_retired");
    request.as_object_mut().unwrap().remove("policy_state");
    request.as_object_mut().unwrap().remove("authorization");
    request.as_object_mut().unwrap().remove("target");
    let (code, missing) = evaluate(&root, &request);
    assert_eq!(code, 3);
    assert_eq!(missing["reason_code"], "invalid_request_json");
}
