use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

fn fixture() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "tooling-cli-policy-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(root.join("references/instructions/capabilities/deep")).unwrap();
    fs::write(
        root.join("SKILL.md"),
        "---\nname: tooling\ndescription: Tool policies.\n---\n",
    )
    .unwrap();
    node(&root.join("references/instructions"), "Root.\n");
    node(
        &root.join("references/instructions/capabilities"),
        "Capabilities.\n",
    );
    node(
        &root.join("references/instructions/capabilities/deep"),
        "Deep.\n",
    );
    root
}

fn node(path: &Path, body: &str) {
    fs::write(
        path.join("instructions.md"),
        format!("---\nname: Policy\ndescription: Applicable policy.\nmetadata:\n  tags: [policy]\n---\n{body}"),
    )
    .unwrap();
}

fn invoke(arguments: &[&str]) -> (i32, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_tooling"))
        .args(arguments)
        .output()
        .unwrap();
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["schema"], "tooling-cli-result");
    assert!(response["reason_code"].is_string());
    assert!(response["data"].is_object());
    (output.status.code().unwrap(), response)
}

fn request(path: &Path, selected: Value, previous: Value) {
    fs::write(
        path,
        serde_json::to_vec(&json!({
            "selected_paths":selected,
            "applicable_references":[],
            "previous":previous,
            "semantic_review":{"reviewed":true,"compatible":true}
        }))
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn policy_commands_return_one_envelope_and_only_direct_metadata() {
    let root = fixture();
    let root_arg = root.to_str().unwrap();
    let (code, discovered) = invoke(&[
        "policy",
        "discover",
        "--skill-root",
        root_arg,
        "--node",
        ".",
    ]);
    assert_eq!(code, 0);
    assert_eq!(discovered["data"]["children"].as_array().unwrap().len(), 1);
    assert_eq!(discovered["data"]["children"][0]["name"], "capabilities");
    assert!(discovered["data"]["children"][0].get("content").is_none());
    let (code, deep) = invoke(&[
        "policy",
        "discover",
        "--skill-root",
        root_arg,
        "--node",
        "capabilities",
    ]);
    assert_eq!(code, 0);
    assert_eq!(deep["data"]["children"][0]["name"], "deep");

    let request_path = root.join("request.json");
    request(&request_path, json!(["capabilities/deep"]), Value::Null);
    let request_arg = request_path.to_str().unwrap();
    let (code, loaded) = invoke(&[
        "policy",
        "load",
        "--skill-root",
        root_arg,
        "--input-file",
        request_arg,
    ]);
    assert_eq!(code, 0);
    assert_eq!(loaded["data"]["sources"].as_array().unwrap().len(), 3);
    assert_eq!(loaded["data"]["semantic_review"]["origin"], "host_reported");
    assert_eq!(
        loaded["data"]["semantic_review"]["claim"]["compatible"],
        true
    );
    assert_eq!(
        loaded["data"]["change"]["added_or_invalidated"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    request(
        &request_path,
        json!(["capabilities/deep"]),
        loaded["data"]["summary"].clone(),
    );
    let (code, unchanged) = invoke(&[
        "policy",
        "load",
        "--skill-root",
        root_arg,
        "--input-file",
        request_arg,
    ]);
    assert_eq!(code, 0);
    assert!(
        unchanged["data"]["change"]["added_or_invalidated"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let mut source_only: Value = serde_json::from_slice(&fs::read(&request_path).unwrap()).unwrap();
    source_only
        .as_object_mut()
        .unwrap()
        .remove("semantic_review");
    fs::write(&request_path, serde_json::to_vec(&source_only).unwrap()).unwrap();
    let (code, without_host_review) = invoke(&[
        "policy",
        "load",
        "--skill-root",
        root_arg,
        "--input-file",
        request_arg,
    ]);
    assert_eq!(code, 0);
    assert!(without_host_review["data"]["semantic_review"]["claim"].is_null());
    request(
        &request_path,
        json!(["capabilities/deep"]),
        loaded["data"]["summary"].clone(),
    );
    node(
        &root.join("references/instructions/capabilities/deep"),
        "Changed.\n",
    );
    let (code, drift) = invoke(&[
        "policy",
        "load",
        "--skill-root",
        root_arg,
        "--input-file",
        request_arg,
    ]);
    assert_eq!(code, 0);
    assert_eq!(
        drift["data"]["change"]["added_or_invalidated"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let branch = root.join("references/instructions/capabilities/deep");
    node(
        &branch,
        "Deep.\n\n## References\n\n| Reference | Load condition |\n| --- | --- |\n| `references/guide.md` | Relevant. |\n",
    );
    fs::create_dir(branch.join("references")).unwrap();
    fs::write(
        branch.join("references/guide.md"),
        "---\nname: Guide\ndescription: Relevant rules.\nmetadata:\n  tags: [guide]\n---\nGuide.\n",
    )
    .unwrap();
    fs::write(
        &request_path,
        serde_json::to_vec(&json!({
            "selected_paths":["capabilities/deep"],
            "applicable_references":["capabilities/deep/references/guide.md"],
            "previous":null,
            "semantic_review":{"reviewed":true,"compatible":true}
        }))
        .unwrap(),
    )
    .unwrap();
    let (code, with_reference) = invoke(&[
        "policy",
        "load",
        "--skill-root",
        root_arg,
        "--input-file",
        request_arg,
    ]);
    assert_eq!(code, 0);
    assert_eq!(
        with_reference["data"]["sources"].as_array().unwrap().len(),
        4
    );
    fs::write(
        &request_path,
        serde_json::to_vec(&json!({
            "selected_paths":["capabilities/deep"],
            "applicable_references":["capabilities/deep/references/undeclared.md"],
            "previous":null,
            "semantic_review":{"reviewed":true,"compatible":true}
        }))
        .unwrap(),
    )
    .unwrap();
    let (code, invalid_reference) = invoke(&[
        "policy",
        "load",
        "--skill-root",
        root_arg,
        "--input-file",
        request_arg,
    ]);
    assert_eq!(code, 4);
    assert_eq!(invalid_reference["reason_code"], "invalid_references");
}

#[test]
fn policy_commands_reject_missing_escape_invalid_source_and_bad_requests() {
    let root = fixture();
    let root_arg = root.to_str().unwrap();
    let (code, missing) = invoke(&[
        "policy",
        "discover",
        "--skill-root",
        root_arg,
        "--node",
        "missing",
    ]);
    assert_ne!(code, 0);
    assert_ne!(missing["reason_code"], "ok");
    let (code, escaped) = invoke(&[
        "policy",
        "discover",
        "--skill-root",
        root_arg,
        "--node",
        "../outside",
    ]);
    assert_eq!(code, 4);
    assert_eq!(escaped["reason_code"], "invalid_node");
    let request_path = root.join("request.json");
    request(&request_path, json!(["missing"]), Value::Null);
    let request_arg = request_path.to_str().unwrap();
    let (code, missing_source) = invoke(&[
        "policy",
        "load",
        "--skill-root",
        root_arg,
        "--input-file",
        request_arg,
    ]);
    assert_eq!(code, 5);
    assert_eq!(missing_source["reason_code"], "policy_source_invalid");
    request(&request_path, json!(["../outside"]), Value::Null);
    let (code, bad_selection) = invoke(&[
        "policy",
        "load",
        "--skill-root",
        root_arg,
        "--input-file",
        request_arg,
    ]);
    assert_eq!(code, 4);
    assert_eq!(bad_selection["reason_code"], "invalid_selection");
    request(&request_path, json!(["capabilities"]), Value::Null);
    fs::write(
        root.join("references/instructions/capabilities/instructions.md"),
        b"invalid\n",
    )
    .unwrap();
    let (code, invalid_source) = invoke(&[
        "policy",
        "load",
        "--skill-root",
        root_arg,
        "--input-file",
        request_arg,
    ]);
    assert_eq!(code, 5);
    assert_eq!(invalid_source["reason_code"], "policy_source_invalid");
    fs::write(&request_path, b"{invalid").unwrap();
    let (code, invalid_json) = invoke(&[
        "policy",
        "load",
        "--skill-root",
        root_arg,
        "--input-file",
        request_arg,
    ]);
    assert_eq!(code, 3);
    assert_eq!(invalid_json["reason_code"], "invalid_request_json");
    let (code, usage) = invoke(&["policy", "load", "--skill-root", root_arg]);
    assert_eq!(code, 2);
    assert_eq!(usage["reason_code"], "cli_usage");
}
