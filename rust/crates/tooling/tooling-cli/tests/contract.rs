use std::process::Command;

use serde_json::Value;

fn run(args: &[&str]) -> (i32, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_tooling"))
        .args(args)
        .output()
        .expect("run Tooling CLI");
    (
        output.status.code().expect("exit code"),
        serde_json::from_slice(&output.stdout).expect("Tooling envelope"),
    )
}

#[test]
fn four_queries_keep_tooling_envelope_and_named_payloads() {
    let (code, list) = run(&[
        "contract",
        "list",
        "--scope",
        "contracts",
        "--prefix",
        "tooling-",
    ]);
    assert_eq!(code, 0);
    assert_eq!(list["schema"], "tooling-cli-result");
    assert!(list["execution"].is_object());
    assert_eq!(list["data"]["commands"].as_array().unwrap().len(), 0);
    assert!(list["data"]["contracts"].as_array().unwrap().len() >= 6);

    let (code, command) = run(&["contract", "describe", "--command", "tooling policy load"]);
    assert_eq!(code, 0);
    assert_eq!(
        command["data"]["request_contract"],
        "tooling-policy-load-request"
    );
    assert!(command["data"]["arguments"].is_array());

    let (code, description) = run(&["contract", "describe", "tooling-policy-load-request"]);
    assert_eq!(code, 0);
    assert_eq!(description["data"]["kind"], "semantic_request");
    assert!(description["data"].get("example").is_none());

    let (code, scaffold) = run(&["contract", "scaffold", "tooling-policy-load-request"]);
    assert_eq!(code, 0);
    assert!(
        scaffold["data"]["scaffold"]
            .get("semantic_review")
            .is_none()
    );
    assert!(
        scaffold["data"]["unresolved_paths"]
            .as_array()
            .unwrap()
            .contains(&Value::String("/previous".into()))
    );

    let (code, example) = run(&[
        "contract",
        "example",
        "tooling-policy-load-request",
        "--scenario",
        "invalid-missing-previous",
    ]);
    assert_eq!(code, 0);
    assert_eq!(example["data"]["valid"], false);
    assert!(example["data"]["example"].is_object());
}

#[test]
fn invalid_queries_are_rejected_without_partial_results() {
    for args in [
        vec!["contract", "list", "--scope", "invalid"],
        vec!["contract", "describe", "unknown"],
        vec![
            "contract",
            "describe",
            "tooling-policy-load-request",
            "--variant",
            "x",
        ],
        vec![
            "contract",
            "describe",
            "tooling-policy-load-request",
            "--command",
            "tooling policy load",
        ],
        vec!["contract", "scaffold", "tooling-cli-result"],
        vec![
            "contract",
            "example",
            "tooling-policy-load-request",
            "--scenario",
            "missing",
        ],
        vec!["contract", "example", "--name", "policy-load-first"],
    ] {
        let (code, response) = run(&args);
        assert_ne!(code, 0, "{args:?}");
        assert_eq!(response["schema"], "tooling-cli-result");
        assert_eq!(response["status"], "rejected");
    }
}
