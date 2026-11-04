//! Direct complete file changes, retained recovery and independent DAG acceptance.
#![cfg(historical_writer_tests)]
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

fn setup() -> (PathBuf, PathBuf) {
    setup_scope(false)
}
fn setup_scope(scoped: bool) -> (PathBuf, PathBuf) {
    setup_config(scoped, false)
}
fn setup_config(scoped: bool, native: bool) -> (PathBuf, PathBuf) {
    setup_methods(scoped, native, false)
}
fn setup_methods(scoped: bool, native: bool, mixed: bool) -> (PathBuf, PathBuf) {
    setup_fixture(scoped, native, mixed, None)
}
fn setup_fixture(
    scoped: bool,
    native: bool,
    mixed: bool,
    supplemental_order: Option<bool>,
) -> (PathBuf, PathBuf) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "work-file-cli-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    fs::create_dir(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let skills = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../../skills/work"
    ))
    .canonicalize()
    .unwrap();
    if let Some(ordered) = supplemental_order {
        work_infrastructure::fixture_support::supplemental_files_fixture(&root, &skills, ordered)
            .unwrap();
    } else if mixed {
        work_infrastructure::fixture_support::mixed_validation_fixture(&root, &skills).unwrap();
    } else if native {
        work_infrastructure::fixture_support::file_transaction_command_scope_fixture(
            &root, &skills,
        )
        .unwrap();
    } else if scoped {
        work_infrastructure::fixture_support::file_transaction_scope_fixture(&root, &skills, true)
            .unwrap();
    } else {
        work_infrastructure::fixture_support::file_transaction_fixture(&root, &skills).unwrap();
    }
    (root, skills)
}
fn invoke(
    root: &Path,
    skills: &Path,
    command: &str,
    task: &str,
    request: Option<Value>,
) -> (i32, Value) {
    invoke_approved(root, skills, command, task, request, None)
}
fn invoke_approved(
    root: &Path,
    skills: &Path,
    command: &str,
    task: &str,
    request: Option<Value>,
    approval: Option<&str>,
) -> (i32, Value) {
    invoke_extra(root, skills, command, task, request, approval, &[])
}
fn invoke_extra(
    root: &Path,
    skills: &Path,
    command: &str,
    task: &str,
    request: Option<Value>,
    approval: Option<&str>,
    extra: &[&str],
) -> (i32, Value) {
    let mut args = vec![
        "--project-root".into(),
        root.to_string_lossy().into_owned(),
        "--verbose".into(),
        "execute".into(),
        command.into(),
        "--task-path".into(),
        "outputs/work/tasks/example/index.json".into(),
        "--execution-dir".into(),
        "outputs/work/executions/example".into(),
        "--task-id".into(),
        task.into(),
        "--user-config-root".into(),
        root.to_string_lossy().into_owned(),
    ];
    if let Some(mut request) = request {
        // Ordinary fixtures read immediately before invocation. Drift tests supply
        // an explicit earlier baseline, which this helper must never replace.
        if request["command"]["kind"] == "apply"
            && request["command"].get("expected_before").is_none()
            && request["command"]["changes"].is_array()
        {
            request["command"]["expected_before"] =
                read_baseline(root, &request["command"]["changes"]);
        }
        if matches!(
            command,
            "file-prepare" | "file-apply" | "file-recovery-prepare" | "file-restore"
        ) {
            // Historical transaction semantics stay under test after the public writer retires.
            let context = work_cli::runtime::prepare_execution_writer_context(
                root,
                skills,
                &[],
                work_flow::execution::ExecutionProjectTarget {
                    task_path: "outputs/work/tasks/example/index.json",
                    execution_dir: "outputs/work/executions/example",
                    task_id: task,
                },
            );
            let result = context.and_then(|context| {
                let repository = work_infrastructure::execution::file_transaction::storage::LocalFileTransactions::for_project(
                    root.to_path_buf(), skills.to_path_buf()
                );
                work_flow::execution::historical_writer_tests::run_file_transaction(&repository, &context, command, &request)
            });
            return match result {
                Ok(data) => (
                    0,
                    serde_json::to_value(work_cli::CliResult::success(data, false)).unwrap(),
                ),
                Err(error) => (
                    error.exit_code as i32,
                    serde_json::to_value(work_cli::CliResult::from_error(error)).unwrap(),
                ),
            };
        }
        let input = root.join("outputs/work/transactions/example/file-test/request.json");
        fs::write(&input, serde_json::to_vec(&request).unwrap()).unwrap();
        args.extend(["--input-file".into(), input.to_string_lossy().into_owned()]);
    }
    if let Some(approval) = approval {
        args.extend(["--approved-sha256".into(), approval.into()]);
    }
    args.extend(extra.iter().map(|s| (*s).to_owned()));
    let (code, result) = work_cli::runtime::run_with_skill_root(&args, skills);
    (code, serde_json::to_value(result).unwrap())
}
fn deviation(root: &Path, skills: &Path, action: Value, files: Value) -> Value {
    let semantic = json!({"gap":"Necessary adjustment after inspecting actual execution evidence",
        "action":action,"modifiable_files":files,"impact":{"summary":"Within the approved Task outcome",
        "requirement_changed":false,"scope_changed":false,"acceptance_criteria_changed":false,
        "deliverables_changed":false,"safety_boundary_changed":false,"external_side_effect_boundary_changed":false},
        "side_effects":[]});
    let (code, prepared) = invoke(
        root,
        skills,
        "deviation-prepare-semantic",
        "TASK-001",
        Some(semantic),
    );
    assert_eq!(code, 0, "{prepared}");
    let (code, recorded) = invoke_extra(
        root,
        skills,
        "deviation-record",
        "TASK-001",
        Some(prepared["data"]["proposal"].clone()),
        prepared["data"]["preview_sha256"].as_str(),
        &[
            "--authorization-evidence",
            "User explicitly approves this reviewed adjustment",
        ],
    );
    assert_eq!(code, 0, "{recorded}");
    recorded["data"].clone()
}

fn skip_reserved(root: &Path, skills: &Path) {
    let reason =
        "Retain the current inputs and replace obsolete evidence with a fresh verification";
    let recorded = deviation(
        root,
        skills,
        json!({"kind":"skip_record","reason":reason}),
        json!([]),
    );
    let (code, response) = invoke(
        root,
        skills,
        "record-finish",
        "TASK-001",
        Some(json!({
        "schema":"work-record-finish-request","record":{"status":"skipped","reason":reason,
        "deviation_id":recorded["deviation_id"]}})),
    );
    assert_eq!(code, 0, "{response}");
}

fn stop_and_restart(root: &Path, skills: &Path, commands: Value, files: Value, carried: Value) {
    let (code, response) = invoke(
        root,
        skills,
        "attempt-close",
        "TASK-001",
        Some(json!({
        "schema":"work-attempt-close-request","status":"stopped","final_type":"other",
        "reason":"User stops after known results and retains complete output","authorization_evidence":"User authorizes this normal stop"})),
    );
    assert_eq!(code, 0, "{response}");
    let original: Value = serde_json::from_slice(
        &fs::read(root.join("outputs/work/executions/example/TASK-001/ATTEMPT-001/attempt.json"))
            .unwrap(),
    )
    .unwrap();
    let choice = json!({"command_positions":commands,"validation_positions":[1],"modifiable_files":files,
        "external_operation_positions":[],"allowed_deviations":[],"scope":original["authorization"]["scope"],
        "authorization_evidence":"User authorizes the next Attempt against preserved results","carried_records":carried});
    let (code, response) = invoke(
        root,
        skills,
        "attempt-start-prepare",
        "TASK-001",
        Some(choice),
    );
    assert_eq!(code, 0, "{response}");
    let (code, response) = invoke(
        root,
        skills,
        "attempt-start",
        "TASK-001",
        Some(response["data"]["request"].clone()),
    );
    assert_eq!(code, 0, "{response}");
}

#[test]
fn additional_validation_requires_explicit_reconciliation_before_retry_without_carry() {
    for failed in [false, true] {
        let (root, skills) = setup_scope(true);
        let changes = work_infrastructure::fixture_support::file_changes_fixture(&root).unwrap();
        let (code, response) = invoke(
            &root,
            &skills,
            "file-apply",
            "TASK-001",
            Some(
                json!({"schema":"work-file-transaction-request","command":{"kind":"apply","attempt_id":"ATTEMPT-001","changes":changes}}),
            ),
        );
        assert_eq!(code, 0, "{response}");
        let context = begin_validation(&root, &skills);
        deviation(
            &root,
            &skills,
            json!({"kind":"add_validation","validation":{"kind":"manual","confirmer":"user","criteria":"Additional necessary condition","acceptance_positions":[1]}}),
            json!([]),
        );
        let (code, response) = invoke(
            &root,
            &skills,
            "record-finish",
            "TASK-001",
            Some(user_finish(&root, &context, "Original validation passed")),
        );
        assert_eq!(code, 0, "{response}");
        if failed {
            let context = reserve_record(&root, &skills, "VAL-002");
            let finish = work_infrastructure::fixture_support::user_confirmation_fixture(
                &root,
                &context,
                work_model::execution::attempt::ValidationOutcome::Failed,
                "Additional condition actually failed",
            )
            .unwrap();
            let (code, response) =
                invoke(&root, &skills, "record-finish", "TASK-001", Some(finish));
            assert_eq!(code, 0, "{response}");
        }
        let (code, response) = invoke(
            &root,
            &skills,
            "attempt-close",
            "TASK-001",
            Some(
                json!({"schema":"work-attempt-close-request","status":"stopped","final_type":"other","reason":"Stop while preserving unmet additional requirements","authorization_evidence":"User approves normal stop"}),
            ),
        );
        assert_eq!(code, 0, "{response}");
        let old_path =
            root.join("outputs/work/executions/example/TASK-001/ATTEMPT-001/attempt.json");
        let old = fs::read(&old_path).unwrap();
        let attempt: Value = serde_json::from_slice(&old).unwrap();
        let choice = json!({"command_positions":[],"validation_positions":[1],"modifiable_files":attempt["authorization"]["modifiable_files"],"external_operation_positions":[],"allowed_deviations":[],"scope":attempt["authorization"]["scope"],"authorization_evidence":"Fresh independent retry approval","carried_records":[]});
        let (code, response) = invoke(
            &root,
            &skills,
            "attempt-start-prepare",
            "TASK-001",
            Some(choice.clone()),
        );
        assert_ne!(code, 0, "{response}");
        assert_eq!(
            response["reason_code"],
            "attempt_start_validation_reconciliation_required"
        );
        let (code, worktree) = invoke(&root, &skills, "worktree", "TASK-001", None);
        assert_eq!(code, 0, "{worktree}");
        let mut authorization = attempt["authorization"].clone();
        authorization["authorization_evidence"] = json!("Fresh independent retry approval");
        let direct = json!({"schema":"work-attempt-start-request","worktree_snapshot_sha256":worktree["data"]["snapshot_sha256"],"authorization":authorization,"continuation":{"source_attempt_id":"ATTEMPT-001","carried_records":[]}});
        let (code, response) = invoke(&root, &skills, "attempt-start", "TASK-001", Some(direct));
        assert_ne!(code, 0, "{response}");
        assert_eq!(
            response["reason_code"], "attempt_start_validation_reconciliation_required",
            "{response}"
        );
        assert!(
            !root
                .join("outputs/work/executions/example/TASK-001/ATTEMPT-002/attempt.json")
                .exists()
        );
        assert_eq!(fs::read(&old_path).unwrap(), old);

        // This is an explicit reviewed decision to retain the deviation as history,
        // rather than an implicit cancellation caused by choosing no carried results.
        let call = |command: &str, request: &Value, approval: Option<&str>| {
            let input = root.join("reconciliation-request.json");
            fs::write(&input, serde_json::to_vec(request).unwrap()).unwrap();
            let mut args = vec![
                "--project-root".into(),
                root.to_string_lossy().into_owned(),
                "--verbose".into(),
                "specification".into(),
                command.into(),
                "--input-file".into(),
                input.to_string_lossy().into_owned(),
                "--user-config-root".into(),
                root.to_string_lossy().into_owned(),
            ];
            if let Some(approval) = approval {
                args.extend(["--approved-sha256".into(), approval.into()]);
            }
            let (code, response) = work_cli::runtime::run_with_skill_root(&args, &skills);
            let response = serde_json::to_value(response).unwrap();
            assert_eq!(code, 0, "{response}");
            response["data"].clone()
        };
        let prepared = call(
            "reconciliation-prepare",
            &json!({"schema":"work-spec-reconciliation-prepare-request","requirement_id":"example","task_position":1,"attempt_position":1,"choice":"retain_only","deviation_positions":[],"edits":[],"semantic_decisions":[],"reason":null}),
            None,
        );
        let preview = call("reconciliation-preview", &prepared["request"], None);
        call(
            "reconciliation-apply",
            &prepared["request"],
            preview["fingerprint"].as_str(),
        );
        let (code, response) = invoke(
            &root,
            &skills,
            "attempt-start-prepare",
            "TASK-001",
            Some(choice),
        );
        assert_eq!(code, 0, "{response}");
        let (code, response) = invoke(
            &root,
            &skills,
            "attempt-start",
            "TASK-001",
            Some(response["data"]["request"].clone()),
        );
        assert_eq!(code, 0, "{response}");
        let new: Value = serde_json::from_slice(
            &fs::read(
                root.join("outputs/work/executions/example/TASK-001/ATTEMPT-002/attempt.json"),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(new["carried_records"].as_array().is_none_or(Vec::is_empty));
        assert_eq!(
            new["authorization"]["authorization_evidence"],
            "Fresh independent retry approval"
        );
        assert_eq!(fs::read(&old_path).unwrap(), old);
    }
}

#[test]
fn passed_additional_validation_still_requires_reconciliation_before_carry() {
    let (root, skills) = setup_scope(true);
    let changes = work_infrastructure::fixture_support::file_changes_fixture(&root).unwrap();
    let (code, response) = invoke(
        &root,
        &skills,
        "file-apply",
        "TASK-001",
        Some(
            json!({"schema":"work-file-transaction-request","command":{"kind":"apply","attempt_id":"ATTEMPT-001","changes":changes}}),
        ),
    );
    assert_eq!(code, 0, "{response}");
    let original = begin_validation(&root, &skills);
    deviation(
        &root,
        &skills,
        json!({"kind":"add_validation","validation":{"kind":"manual","confirmer":"user","criteria":"Additional necessary condition","acceptance_positions":[1]}}),
        json!([]),
    );
    let (code, response) = invoke(
        &root,
        &skills,
        "record-finish",
        "TASK-001",
        Some(user_finish(&root, &original, "Original validation passed")),
    );
    assert_eq!(code, 0, "{response}");
    let additional = reserve_record(&root, &skills, "VAL-002");
    let finish = work_infrastructure::fixture_support::user_confirmation_fixture(
        &root,
        &additional,
        work_model::execution::attempt::ValidationOutcome::Passed,
        "Additional condition passed",
    )
    .unwrap();
    let (code, response) = invoke(&root, &skills, "record-finish", "TASK-001", Some(finish));
    assert_eq!(code, 0, "{response}");
    let (code, response) = invoke(
        &root,
        &skills,
        "attempt-close",
        "TASK-001",
        Some(
            json!({"schema":"work-attempt-close-request","status":"stopped","final_type":"other","reason":"Stop with both validations passed and other work pending","authorization_evidence":"User approves normal stop"}),
        ),
    );
    assert_eq!(code, 0, "{response}");

    let attempt_path =
        root.join("outputs/work/executions/example/TASK-001/ATTEMPT-001/attempt.json");
    let index_path = root.join("outputs/work/executions/example/index.json");
    let old_attempt = fs::read(&attempt_path).unwrap();
    let old_index = fs::read(&index_path).unwrap();
    let attempt: Value = serde_json::from_slice(&old_attempt).unwrap();
    let records = attempt["records"].as_array().unwrap();
    let position = records
        .iter()
        .position(|record| record["id"] == "VAL-002")
        .unwrap()
        + 1;
    assert_eq!(records[position - 1]["outcome"], "passed");
    let carried = json!([{"position":position,"evidence":"User explicitly carries the passed additional validation"}]);
    let choice = json!({"command_positions":[],"validation_positions":[1],
        "modifiable_files":attempt["authorization"]["modifiable_files"],
        "external_operation_positions":[],"allowed_deviations":[],
        "scope":attempt["authorization"]["scope"],
        "authorization_evidence":"Fresh authorization with explicit carry",
        "carried_records":carried});
    let (code, response) = invoke(
        &root,
        &skills,
        "attempt-start-prepare",
        "TASK-001",
        Some(choice),
    );
    assert_ne!(code, 0, "{response}");
    assert_eq!(
        response["reason_code"],
        "attempt_start_validation_reconciliation_required"
    );
    let (code, worktree) = invoke(&root, &skills, "worktree", "TASK-001", None);
    assert_eq!(code, 0, "{worktree}");
    let mut authorization = attempt["authorization"].clone();
    authorization["authorization_evidence"] = json!("Fresh authorization with explicit carry");
    let direct = json!({"schema":"work-attempt-start-request",
        "worktree_snapshot_sha256":worktree["data"]["snapshot_sha256"],
        "authorization":authorization,
        "continuation":{"source_attempt_id":"ATTEMPT-001",
            "carried_records":[{"record_id":"VAL-002",
                "evidence":"User explicitly carries the passed additional validation"}]}});
    let (code, response) = invoke(&root, &skills, "attempt-start", "TASK-001", Some(direct));
    assert_ne!(code, 0, "{response}");
    assert_eq!(
        response["reason_code"],
        "attempt_start_validation_reconciliation_required"
    );
    assert_eq!(fs::read(&attempt_path).unwrap(), old_attempt);
    assert_eq!(fs::read(&index_path).unwrap(), old_index);
    assert!(
        !root
            .join("outputs/work/executions/example/TASK-001/ATTEMPT-002/attempt.json")
            .exists()
    );
}

#[cfg(any(target_os = "macos", windows))]
fn run_owned_command(root: &Path, skills: &Path, id: &str) -> Value {
    reserve_record(root, skills, id);
    run_reserved_host_command(root, skills)
}

#[cfg(any(target_os = "macos", windows))]
fn run_reserved_host_command(root: &Path, skills: &Path) -> Value {
    let (code, binding) = invoke(root, skills, "command-binding", "TASK-001", None);
    assert_eq!(code, 0, "{binding}");
    let binding = &binding["data"];
    let argv = binding["command"]["argv"].as_array().unwrap();
    let executable = argv[0].as_str().unwrap();
    let mut child = std::process::Command::new(executable);
    for argument in &argv[1..] {
        child.arg(argument.as_str().unwrap());
    }
    let cwd = binding["execution"]["working_directory"]
        .as_str()
        .unwrap_or(".");
    let output = child.current_dir(root.join(cwd)).output().unwrap();
    let exit = output.status.code().expect("known Host exit") as i64;
    let relative = format!("host-evidence/{}", binding["record_id"].as_str().unwrap());
    let directory = root.join(&relative);
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("stdout.bin"), &output.stdout).unwrap();
    fs::write(directory.join("stderr.bin"), &output.stderr).unwrap();
    let artifact = |file: &str, bytes: &[u8]| {
        json!({
            "file":file,"size_bytes":bytes.len(),
            "sha256":work_infrastructure::fixture_support::raw_sha256(bytes),"complete":true
        })
    };
    let mut reports = serde_json::Map::new();
    for (position, path) in binding["capture"]["report_paths"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let path = path.as_str().unwrap();
        let bytes = fs::read(root.join(cwd).join(path)).unwrap();
        let file = format!("report-{position:04}.bin");
        fs::write(directory.join(&file), &bytes).unwrap();
        reports.insert(
            path.into(),
            json!({"status":"available","artifact":artifact(&file, &bytes)}),
        );
    }
    let captures = json!({"stdout":artifact("stdout.bin", &output.stdout),
        "stderr":artifact("stderr.bin", &output.stderr),"reports":reports});
    let raw = serde_json::to_vec(&captures).unwrap();
    fs::write(directory.join("result.json"), &raw).unwrap();
    let report = json!({"schema":"work-host-execution-report",
        "task_id":binding["task_id"],"attempt_id":binding["attempt_id"],
        "record_id":binding["record_id"],"task_collection_sha256":binding["task_collection_sha256"],
        "authorization_sha256":binding["authorization_sha256"],"file_version":binding["file_version"],
        "command_sha256":binding["command_sha256"],"result_origin":"host_reported",
        "result_reference":format!("{relative}/result.json"),
        "result_sha256":work_infrastructure::fixture_support::raw_sha256(&raw),
        "status":"exited","exit_code":exit,"output_complete":true});
    let (code, response) = invoke(
        root,
        skills,
        "record-finish",
        "TASK-001",
        Some(json!({"schema":"work-record-finish-request","record":{"host_report":report}})),
    );
    assert_eq!(code, 0, "{response}");
    let mut input_files = serde_json::Map::new();
    for path in ["a-old.txt", "b-moved.txt", "c-existing.txt", "d-new.txt"] {
        if let Ok(bytes) = fs::read(root.join(path)) {
            input_files.insert(
                path.into(),
                json!({"sha256":work_infrastructure::fixture_support::raw_sha256(&bytes)}),
            );
        }
    }
    json!({"execution":{"capture":binding["capture"],"input_files":input_files},
        "receipt_dir":relative,"data":{"exit_code":exit,"stdout_tail":String::from_utf8_lossy(&output.stdout),
        "command_sha256":binding["command_sha256"],"record_id":binding["record_id"],
        "result_sha256":work_infrastructure::fixture_support::raw_sha256(&raw)}})
}

fn reserve_fresh_record(root: &Path, skills: &Path, id: &str) -> Value {
    let (code, response) = invoke_extra(
        root,
        skills,
        "record-begin",
        "TASK-001",
        None,
        None,
        &[
            "--record-id",
            id,
            "--authorization-evidence",
            "User authorizes fresh verification after the explicitly skipped obsolete reservation",
        ],
    );
    assert_eq!(code, 0, "{response}");
    response["data"]["validation_context"].clone()
}

#[cfg(any(target_os = "macos", windows))]
#[test]
fn moved_declared_input_supports_correction_and_retry_without_recreating_old_path() {
    for retry in [false, true] {
        let root = std::env::temp_dir().join(format!(
            "work-moved-input-cli-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let skills = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../skills/work"
        ))
        .canonicalize()
        .unwrap();
        work_infrastructure::fixture_support::moved_input_fixture(&root, &skills).unwrap();
        let mut changes = serde_json::to_value(
            work_infrastructure::fixture_support::file_changes_fixture(&root).unwrap(),
        )
        .unwrap();
        changes[0]["bytes"] = json!(b"known failing moved result\n".to_vec());
        let (code, response) = invoke(
            &root,
            &skills,
            "file-apply",
            "TASK-001",
            Some(
                json!({"schema":"work-file-transaction-request","command":{"kind":"apply","attempt_id":"ATTEMPT-001","changes":changes}}),
            ),
        );
        assert_eq!(code, 0, "{response}");
        let preview = run_owned_command(&root, &skills, "CMD-002");
        assert!(
            preview["execution"]["input_files"]
                .get("a-old.txt")
                .is_none()
        );
        assert_eq!(
            preview["execution"]["input_files"]["b-moved.txt"]["sha256"],
            work_infrastructure::fixture_support::raw_sha256(b"known failing moved result\n")
        );
        begin_validation(&root, &skills);
        let (code, response) = invoke(
            &root,
            &skills,
            "record-finish",
            "TASK-001",
            Some(
                json!({"schema":"work-record-finish-request","record":{"outcome":"failed","evidence":"Actual moved output test failed"}}),
            ),
        );
        assert_eq!(code, 0, "{response}");
        if retry {
            stop_and_restart(
                &root,
                &skills,
                json!([2]),
                json!(["a-old.txt", "b-moved.txt", "c-existing.txt", "d-new.txt"]),
                json!([]),
            );
        }
        let attempt_id = if retry { "ATTEMPT-002" } else { "ATTEMPT-001" };
        let (code, response) = invoke(
            &root,
            &skills,
            "file-apply",
            "TASK-001",
            Some(
                json!({"schema":"work-file-transaction-request","command":{"kind":"apply","attempt_id":attempt_id,"changes":[{"kind":"write","path":"b-moved.txt","bytes":b"final move\n".to_vec()}]}}),
            ),
        );
        assert_eq!(code, 0, "{response}");
        let preview = run_owned_command(&root, &skills, "CMD-002");
        assert_eq!(
            preview["execution"]["input_files"]["b-moved.txt"]["sha256"],
            work_infrastructure::fixture_support::raw_sha256(b"final move\n")
        );
        begin_validation(&root, &skills);
        let (code, response) = invoke(
            &root,
            &skills,
            "record-finish",
            "TASK-001",
            Some(
                json!({"schema":"work-record-finish-request","record":{"outcome":"passed","evidence":"Fresh corrected moved output passed"}}),
            ),
        );
        assert_eq!(code, 0, "{response}");
        let (code, response) = invoke(
            &root,
            &skills,
            "attempt-close",
            "TASK-001",
            Some(json!({"schema":"work-attempt-close-request","status":"completed"})),
        );
        assert_eq!(code, 0, "{response}");
        assert!(!root.join("a-old.txt").exists());
    }
}

#[test]
fn missing_declared_input_without_a_published_move_is_rejected() {
    let root = std::env::temp_dir().join(format!(
        "work-missing-input-cli-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let skills = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../../skills/work"
    ))
    .canonicalize()
    .unwrap();
    work_infrastructure::fixture_support::moved_input_fixture(&root, &skills).unwrap();
    fs::rename(
        root.join("a-old.txt"),
        root.join("unexpected-external-move.txt"),
    )
    .unwrap();
    let before = fs::read(root.join("c-existing.txt")).unwrap();
    let (code, response) = invoke(
        &root,
        &skills,
        "file-apply",
        "TASK-001",
        Some(
            json!({"schema":"work-file-transaction-request","command":{"kind":"apply","attempt_id":"ATTEMPT-001","changes":[{"kind":"write","path":"c-existing.txt","bytes":b"proposed correction".to_vec()}]}}),
        ),
    );
    assert_ne!(code, 0, "{response}");
    assert_eq!(
        response["reason_code"], "file_transaction_input_unverified",
        "{response}"
    );
    assert_eq!(fs::read(root.join("c-existing.txt")).unwrap(), before);
    assert!(!root.join("a-old.txt").exists());
    assert!(!root.join("b-moved.txt").exists());
    assert_eq!(
        fs::read(root.join("unexpected-external-move.txt")).unwrap(),
        b"original move\r\n"
    );
}

#[cfg(any(target_os = "macos", windows))]
#[test]
fn added_report_validation_preserves_historical_capture_and_requires_fresh_report() {
    let (root, skills) = setup_scope(true);
    let changes = work_infrastructure::fixture_support::file_changes_fixture(&root).unwrap();
    let (code, response) = invoke(
        &root,
        &skills,
        "file-apply",
        "TASK-001",
        Some(
            json!({"schema":"work-file-transaction-request","command":{"kind":"apply","attempt_id":"ATTEMPT-001","changes":changes}}),
        ),
    );
    assert_eq!(code, 0, "{response}");
    let context = begin_validation(&root, &skills);
    #[cfg(windows)]
    let argv = json!([
        std::env::var("COMSPEC").unwrap(),
        "/d",
        "/c",
        "echo {\"ok\":true}>report.json"
    ]);
    #[cfg(target_os = "macos")]
    let argv = json!(["/bin/sh", "-c", "printf '{\"ok\":true}' > report.json"]);
    deviation(
        &root,
        &skills,
        json!({"kind":"add_command","command":{"mode":"argv","argv":argv,"execution":{"working_directory":".","os":std::env::consts::OS,"shell":if cfg!(windows) {"cmd"} else {"sh"}}}}),
        json!([]),
    );
    let (code, response) = invoke(
        &root,
        &skills,
        "record-finish",
        "TASK-001",
        Some(user_finish(
            &root,
            &context,
            "Original manual condition passed",
        )),
    );
    assert_eq!(code, 0, "{response}");
    let old = run_owned_command(&root, &skills, "CMD-001");
    assert_eq!(old["execution"]["capture"]["report_paths"], json!([]));
    let receipt = root.join(old["receipt_dir"].as_str().unwrap());
    let captured_result = fs::read(receipt.join("result.json")).unwrap();
    let captured_stdout = fs::read(receipt.join("stdout.bin")).unwrap();
    reserve_record(&root, &skills, "VAL-001");
    deviation(
        &root,
        &skills,
        json!({"kind":"add_validation","validation":{"kind":"automated","command_positions":[1],"pass_condition":"The actual report confirms success","verification":{"kind":"json_report_equals","command":1,"path":"report.json","pointer":"/ok","equals":true},"acceptance_positions":[1]}}),
        json!([]),
    );
    skip_reserved(&root, &skills);
    reserve_record(&root, &skills, "VAL-002");
    let finish = json!({"schema":"work-record-finish-request","record":{"outcome":"passed","evidence":"Use the captured actual report"}});
    let (code, response) = invoke(
        &root,
        &skills,
        "record-finish",
        "TASK-001",
        Some(finish.clone()),
    );
    assert_ne!(code, 0, "{response}");
    // The old execution remains true; the uncaptured live report cannot satisfy VAL.
    assert_eq!(
        response["reason_code"], "validation_execution_unverified",
        "{response}"
    );
    assert!(
        matches!(
            response["data"]["evidence_stage"].as_str(),
            Some("condition_evaluation" | "validation_outcome")
        ),
        "{response}"
    );
    skip_reserved(&root, &skills);
    let fresh = run_owned_command(&root, &skills, "CMD-001");
    assert_eq!(
        fresh["execution"]["capture"]["report_paths"],
        json!(["report.json"])
    );
    reserve_fresh_record(&root, &skills, "VAL-002");
    let (code, response) = invoke(&root, &skills, "record-finish", "TASK-001", Some(finish));
    assert_eq!(code, 0, "{response}");
    // Replace the deliberately skipped original manual check with an actual pass.
    let context = reserve_fresh_record(&root, &skills, "VAL-001");
    let (code, response) = invoke(
        &root,
        &skills,
        "record-finish",
        "TASK-001",
        Some(user_finish(
            &root,
            &context,
            "Current manual condition passed",
        )),
    );
    assert_eq!(code, 0, "{response}");
    let (code, response) = invoke(
        &root,
        &skills,
        "attempt-close",
        "TASK-001",
        Some(json!({"schema":"work-attempt-close-request","status":"completed"})),
    );
    assert_eq!(code, 0, "{response}");
    assert_eq!(
        fs::read(receipt.join("result.json")).unwrap(),
        captured_result
    );
    assert_eq!(
        fs::read(receipt.join("stdout.bin")).unwrap(),
        captured_stdout
    );
}

#[test]
fn complete_created_and_moved_files_survive_a_verification_only_retry() {
    let (root, skills) = setup_scope(true);
    let changes = work_infrastructure::fixture_support::file_changes_fixture(&root).unwrap();
    let (code, response) = invoke(
        &root,
        &skills,
        "file-apply",
        "TASK-001",
        Some(json!({
        "schema":"work-file-transaction-request","command":{"kind":"apply","attempt_id":"ATTEMPT-001","changes":changes}})),
    );
    assert_eq!(code, 0, "{response}");
    let paths = ["b-moved.txt", "c-existing.txt", "d-new.txt"];
    let bytes: Vec<_> = paths
        .iter()
        .map(|path| fs::read(root.join(path)).unwrap())
        .collect();
    let history = root.join("outputs/work/runtime/staging/example/project-files");
    let count = fs::read_dir(&history).unwrap().count();
    stop_and_restart(
        &root,
        &skills,
        json!([]),
        json!(["a-old.txt", "b-moved.txt", "c-existing.txt", "d-new.txt"]),
        json!([]),
    );
    let original_path =
        root.join("outputs/work/executions/example/TASK-001/ATTEMPT-001/attempt.json");
    let original = fs::read(&original_path).unwrap();
    let context = begin_validation(&root, &skills);
    let (code, response) = invoke(
        &root,
        &skills,
        "record-finish",
        "TASK-001",
        Some(user_finish(
            &root,
            &context,
            "Preserved output has now been verified",
        )),
    );
    assert_eq!(code, 0, "{response}");
    let (code, response) = invoke(
        &root,
        &skills,
        "attempt-close",
        "TASK-001",
        Some(json!({"schema":"work-attempt-close-request","status":"completed"})),
    );
    assert_eq!(code, 0, "{response}");
    assert_eq!(fs::read(&original_path).unwrap(), original);
    assert_eq!(fs::read_dir(&history).unwrap().count(), count);
    assert!(!root.join("a-old.txt").exists());
    for (path, expected) in paths.iter().zip(bytes) {
        assert_eq!(fs::read(root.join(path)).unwrap(), expected);
    }
}

#[test]
fn whole_file_sets_formalize_and_apply_from_one_initial_tree() {
    for kind in ["swap", "replacement", "intermediate"] {
        let root = std::env::temp_dir().join(format!(
            "work-whole-set-{kind}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let skills = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../skills/work"
        ))
        .canonicalize()
        .unwrap();
        let fixture =
            work_infrastructure::fixture_support::whole_file_set_fixture(&root, &skills, kind);
        if kind == "intermediate" {
            assert!(
                fixture.is_err(),
                "Planning must reject a move whose source only exists after a create"
            );
            assert!(!root.join("d-new.txt").exists());
            continue;
        }
        fixture.unwrap();
        let a = fs::read(root.join("a-old.txt")).unwrap();
        let c = fs::read(root.join("c-existing.txt")).unwrap();
        let changes = if kind == "swap" {
            json!([
            {"kind":"move","source":"a-old.txt","destination":"c-existing.txt"},
            {"kind":"move","source":"c-existing.txt","destination":"a-old.txt"},
            {"kind":"write","path":"d-new.txt","bytes":b"new".to_vec()}])
        } else {
            json!([
            {"kind":"move","source":"a-old.txt","destination":"b-moved.txt"},
            {"kind":"write","path":"a-old.txt","bytes":b"replacement".to_vec()},
            {"kind":"write","path":"c-existing.txt","bytes":c}])
        };
        let (code, response) = invoke(
            &root,
            &skills,
            "file-apply",
            "TASK-001",
            Some(json!({
            "schema":"work-file-transaction-request","command":{"kind":"apply","attempt_id":"ATTEMPT-001","changes":changes}})),
        );
        assert_eq!(code, 0, "{kind}: {response}");
        if kind == "swap" {
            assert_eq!(fs::read(root.join("a-old.txt")).unwrap(), c);
            assert_eq!(fs::read(root.join("c-existing.txt")).unwrap(), a);
        } else {
            assert_eq!(fs::read(root.join("b-moved.txt")).unwrap(), a);
            assert_eq!(fs::read(root.join("a-old.txt")).unwrap(), b"replacement");
        }
    }
}

#[test]
fn migration_publication_requires_real_review_and_separate_execute_authority() {
    migration_publication_file_flow(false);
}

#[test]
fn migration_preserves_snapshot_provenance_without_fabricating_discussion() {
    migration_publication_file_flow(true);
}

fn migration_publication_file_flow(snapshot: bool) {
    let root = std::env::temp_dir().join(format!(
        "work-migration-file-cli-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let current_skills = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../../skills/work"
    ))
    .canonicalize()
    .unwrap();
    let skills = current_skills;
    let fixture = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../work-infrastructure/fixtures/cases/specification/migration/reconstruction"
    ));
    for path in [
        "outputs/work/plans/example.json",
        "outputs/work/tasks/example/index.json",
        "outputs/work/tasks/example/tasks/TASK-001.json",
    ] {
        fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
        fs::copy(fixture.join("project").join(path), root.join(path)).unwrap();
    }
    fs::write(root.join("src.txt"), b"original source").unwrap();
    let input = root.join("semantic.json");
    fs::copy(fixture.join("input/semantic-request.json"), &input).unwrap();
    let repository = work_infrastructure::hierarchy_catalog::LocalHierarchyCatalog {
        skill_root: skills.clone(),
    };
    let hierarchy = work_flow::hierarchy::selection_build(
        &repository,
        &json!({"decision":"general_only","selections":[]}),
    )
    .unwrap();
    let mut request: Value = serde_json::from_slice(&fs::read(&input).unwrap()).unwrap();
    request["task_context"]["hierarchy_selection"] = hierarchy;
    fs::write(&input, serde_json::to_vec(&request).unwrap()).unwrap();
    if snapshot {
        let source_fixture = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../work-infrastructure/fixtures/cases/specification/update/collection-summary/project"
        ));
        let source_index: Value = serde_json::from_slice(
            &fs::read(source_fixture.join("outputs/work/tasks/example/index.json")).unwrap(),
        )
        .unwrap();
        let mut request: Value = serde_json::from_slice(&fs::read(&input).unwrap()).unwrap();
        request["task_context"]["source"] = source_index["source"].clone();
        for relative in [
            "outputs/work/sources/example/SRC-001/manifest.json",
            "outputs/work/sources/example/SRC-001/source.txt",
            "outputs/work/sources/example/SRC-001/manifest.json.done",
        ] {
            let raw = fs::read(source_fixture.join(relative)).unwrap();
            fs::create_dir_all(root.join(relative).parent().unwrap()).unwrap();
            fs::write(root.join(relative), &raw).unwrap();
            request["sources"].as_array_mut().unwrap().push(json!({"path":relative,"raw_sha256":work_infrastructure::fixture_support::raw_sha256(&raw)}));
        }
        fs::write(&input, serde_json::to_vec(&request).unwrap()).unwrap();
    }
    let candidate = root.join("candidate.json");
    let migrate = |command: &str, input: &Path, approval: Option<&str>| {
        let mut args = vec![
            "--project-root".into(),
            root.to_string_lossy().into_owned(),
            "--verbose".into(),
            "migration".into(),
            command.into(),
            "--input-file".into(),
            input.to_string_lossy().into_owned(),
            "--user-config-root".into(),
            root.to_string_lossy().into_owned(),
        ];
        if command == "prepare" {
            args.extend([
                "--output-file".into(),
                candidate.to_string_lossy().into_owned(),
            ]);
        }
        if let Some(sha) = approval {
            args.extend(["--approved-sha256".into(), sha.into()]);
        }
        let (code, response) = work_cli::runtime::run_with_skill_root(&args, &skills);
        let response = serde_json::to_value(response).unwrap();
        assert_eq!(code, 0, "{command}: {response}");
        response["data"].clone()
    };
    migrate("prepare", &input, None);
    let preview = migrate("preview", &candidate, None);
    let approval = preview["fingerprint"].as_str().unwrap();
    migrate("apply", &candidate, Some(approval));
    let verified = migrate("verify", &candidate, Some(approval));
    assert_eq!(verified["status"], "valid");
    let index: Value = serde_json::from_slice(
        &fs::read(root.join("outputs/work/tasks/example/index.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        index["source"]["kind"],
        if snapshot { "snapshot" } else { "migration" }
    );
    assert!(index.get("discussion").is_none());
    fs::create_dir_all(root.join("outputs/work/transactions/example/file-test")).unwrap();
    let apply = json!({"schema":"work-file-transaction-request","command":{"kind":"apply","attempt_id":"ATTEMPT-001",
        "changes":[{"kind":"write","path":"src.txt","bytes":b"reviewed change".to_vec()}]}});
    assert_ne!(
        invoke(
            &root,
            &skills,
            "file-apply",
            "TASK-001",
            Some(apply.clone())
        )
        .0,
        0
    );
    assert_eq!(fs::read(root.join("src.txt")).unwrap(), b"original source");
    for args in [
        vec!["init"],
        vec!["add", "."],
        vec![
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-m",
            "Fixture baseline",
        ],
    ] {
        let result = std::process::Command::new("git")
            .args(args)
            .current_dir(&root)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let choice = json!({"command_positions":[1],"validation_positions":[1],"modifiable_files":["src.txt"],
        "external_operation_positions":[],"allowed_deviations":[],"authorization_evidence":"Independent user Execute approval after Migration verification"});
    let (code, prepared) = invoke(
        &root,
        &skills,
        "attempt-start-prepare",
        "TASK-001",
        Some(choice),
    );
    assert_eq!(code, 0, "{prepared}");
    let (code, response) = invoke(
        &root,
        &skills,
        "attempt-start",
        "TASK-001",
        Some(prepared["data"]["request"].clone()),
    );
    assert_eq!(code, 0, "{response}");
    let journal = verified["result"]["journal"].as_str().unwrap();
    let marker = root.join(journal).with_file_name("committed.sha256");
    let original = fs::read(&marker).unwrap();
    fs::write(&marker, b"tampered migration evidence").unwrap();
    assert_ne!(
        invoke(
            &root,
            &skills,
            "file-apply",
            "TASK-001",
            Some(apply.clone())
        )
        .0,
        0
    );
    assert_eq!(fs::read(root.join("src.txt")).unwrap(), b"original source");
    fs::write(marker, original).unwrap();
    let (code, response) = invoke(&root, &skills, "file-apply", "TASK-001", Some(apply));
    assert_eq!(code, 0, "{response}");
    assert_eq!(fs::read(root.join("src.txt")).unwrap(), b"reviewed change");
}

#[cfg(any(target_os = "macos", windows))]
#[test]
fn additional_files_alone_registers_idle_scope_and_rechecks_dependencies() {
    let (root, skills) = setup_inputs();
    let (code, response) = invoke(
        &root,
        &skills,
        "attempt-close",
        "TASK-001",
        Some(json!({"schema":"work-attempt-close-request",
        "status":"stopped","final_type":"other","reason":"Select the exact file-only scope","authorization_evidence":"User approves stopping"})),
    );
    assert_eq!(code, 0, "{response}");
    let first: Value = serde_json::from_slice(
        &fs::read(root.join("outputs/work/executions/example/TASK-001/ATTEMPT-001/attempt.json"))
            .unwrap(),
    )
    .unwrap();
    let mut scope = first["authorization"]["scope"].clone();
    scope["adjustments"] = json!(["additional_files"]);
    scope["modification_types"] = json!(["create"]);
    scope["module_paths"] = json!(["src", "independent.txt"]);
    let choice = json!({"command_positions":[1],"validation_positions":[1],"modifiable_files":[],"external_operation_positions":[],
        "allowed_deviations":[],"scope":scope,"authorization_evidence":"User authorizes only necessary additional source files","carried_records":[]});
    let (code, prepared) = invoke(
        &root,
        &skills,
        "attempt-start-prepare",
        "TASK-001",
        Some(choice),
    );
    assert_eq!(code, 0, "{prepared}");
    let (code, response) = invoke(
        &root,
        &skills,
        "attempt-start",
        "TASK-001",
        Some(prepared["data"]["request"].clone()),
    );
    assert_eq!(code, 0, "{response}");
    let apply = json!({"schema":"work-file-transaction-request","command":{"kind":"apply","attempt_id":"ATTEMPT-002",
        "changes":[{"kind":"write","path":"src/Extra.java","bytes":b"necessary source".to_vec()}]}});
    assert_ne!(
        invoke(
            &root,
            &skills,
            "file-apply",
            "TASK-001",
            Some(apply.clone())
        )
        .0,
        0
    );
    let action = json!({"kind":"add_files","modification_types":["create"]});
    let recorded = deviation(&root, &skills, action.clone(), json!(["src/Extra.java"]));
    assert_eq!(recorded["lock_status"], "attempt_held");
    let attempt: Value = serde_json::from_slice(
        &fs::read(root.join("outputs/work/executions/example/TASK-001/ATTEMPT-002/attempt.json"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(attempt["records"], json!([]));
    assert_eq!(attempt["authorization"]["modifiable_files"], json!([]));
    let (code, response) = invoke(&root, &skills, "file-apply", "TASK-001", Some(apply));
    assert_eq!(code, 0, "{response}");
    assert_eq!(
        fs::read(root.join("src/Extra.java")).unwrap(),
        b"necessary source"
    );
    for (path, kind, reason) in [
        (
            "other/Outside.java",
            "create",
            "execution_authorization_scope_expansion",
        ),
        ("independent.txt", "create", "task_file_dependency_required"),
        (
            "src/UserService.java",
            "modify",
            "execution_authorization_scope_expansion",
        ),
    ] {
        let semantic = json!({"gap":"Inspect additional necessary file","action":{"kind":"add_files","modification_types":[kind]},"modifiable_files":[path],
            "impact":{"summary":"Same outcome","requirement_changed":false,"scope_changed":false,"acceptance_criteria_changed":false,
            "deliverables_changed":false,"safety_boundary_changed":false,"external_side_effect_boundary_changed":false},"side_effects":[]});
        let (code, response) = invoke(
            &root,
            &skills,
            "deviation-prepare-semantic",
            "TASK-001",
            Some(semantic),
        );
        assert_ne!(code, 0, "{response}");
        assert_eq!(response["reason_code"], reason, "{response}");
    }
}

#[cfg(any(target_os = "macos", windows))]
#[test]
fn carried_validation_drift_does_not_block_fresh_results() {
    let (root, skills) = setup_inputs();
    let run = || {
        assert_eq!(
            run_reserved_host_command(&root, &skills)["data"]["exit_code"],
            0
        );
    };
    let finish = json!({"schema":"work-record-finish-request","record":{"outcome":"passed","evidence":"Actual input evidence"}});
    reserve_record(&root, &skills, "CMD-001");
    run();
    begin_validation(&root, &skills);
    let (code, response) = invoke(
        &root,
        &skills,
        "record-finish",
        "TASK-001",
        Some(finish.clone()),
    );
    assert_eq!(code, 0, "{response}");
    stop_and_restart(
        &root,
        &skills,
        json!([1]),
        json!([]),
        json!([
        {"position":1,"evidence":"Carry actual A command"},{"position":2,"evidence":"Carry actual A validation"}]),
    );
    let old_path = root.join("outputs/work/executions/example/TASK-001/ATTEMPT-001/attempt.json");
    let old = fs::read(&old_path).unwrap();
    fs::write(root.join("src/UserService.java"), b"version B\n").unwrap();
    for id in ["CMD-001", "VAL-001"] {
        let (code, response) = invoke_extra(
            &root,
            &skills,
            "record-begin",
            "TASK-001",
            None,
            None,
            &[
                "--record-id",
                id,
                "--authorization-evidence",
                "User authorizes fresh B evidence replacing carried A",
            ],
        );
        assert_eq!(code, 0, "{response}");
        if id.starts_with("CMD") {
            run();
        } else {
            let (code, response) = invoke(
                &root,
                &skills,
                "record-finish",
                "TASK-001",
                Some(finish.clone()),
            );
            assert_eq!(code, 0, "{response}");
        }
    }
    let (code, response) = invoke(
        &root,
        &skills,
        "attempt-close",
        "TASK-001",
        Some(json!({"schema":"work-attempt-close-request","status":"completed"})),
    );
    assert_eq!(code, 0, "{response}");
    assert_eq!(fs::read(old_path).unwrap(), old);
    assert_eq!(
        fs::read(root.join("src/UserService.java")).unwrap(),
        b"version B\n"
    );
}

#[test]
fn supplemental_validation_must_pass_before_public_close() {
    let (root, skills) = setup_scope(true);
    let changes = work_infrastructure::fixture_support::file_changes_fixture(&root).unwrap();
    let (code, response) = invoke(
        &root,
        &skills,
        "file-apply",
        "TASK-001",
        Some(json!({
        "schema":"work-file-transaction-request","command":{"kind":"apply","attempt_id":"ATTEMPT-001","changes":changes}})),
    );
    assert_eq!(code, 0, "{response}");
    let context = begin_validation(&root, &skills);
    deviation(
        &root,
        &skills,
        json!({"kind":"add_validation","validation":{"kind":"manual",
        "confirmer":"user","criteria":"Inspect the additional necessary condition","acceptance_positions":[1]}}),
        json!([]),
    );
    let (code, response) = invoke(
        &root,
        &skills,
        "record-finish",
        "TASK-001",
        Some(user_finish(&root, &context, "Original condition verified")),
    );
    assert_eq!(code, 0, "{response}");
    let close = json!({"schema":"work-attempt-close-request","status":"completed"});
    let check_rejected = || {
        let (code, response) = invoke(
            &root,
            &skills,
            "attempt-close",
            "TASK-001",
            Some(close.clone()),
        );
        assert_ne!(code, 0, "{response}");
        assert_eq!(
            response["reason_code"], "attempt_close_incomplete_validations",
            "{response}"
        );
    };
    check_rejected();
    reserve_record(&root, &skills, "VAL-002");
    skip_reserved(&root, &skills);
    check_rejected();
    for outcome in [
        work_model::execution::attempt::ValidationOutcome::Failed,
        work_model::execution::attempt::ValidationOutcome::Passed,
    ] {
        let (code, response) = invoke_extra(
            &root,
            &skills,
            "record-begin",
            "TASK-001",
            None,
            None,
            &[
                "--record-id",
                "VAL-002",
                "--authorization-evidence",
                "User approves fresh verification after the known prior result",
            ],
        );
        assert_eq!(code, 0, "{response}");
        let finish = work_infrastructure::fixture_support::user_confirmation_fixture(
            &root,
            &response["data"]["validation_context"],
            outcome,
            "Actual additional condition decision",
        )
        .unwrap();
        let (code, response) = invoke(&root, &skills, "record-finish", "TASK-001", Some(finish));
        assert_eq!(code, 0, "{response}");
        if outcome == work_model::execution::attempt::ValidationOutcome::Failed {
            check_rejected();
        }
    }
    let (code, response) = invoke(&root, &skills, "attempt-close", "TASK-001", Some(close));
    assert_eq!(code, 0, "{response}");
}

#[cfg(any(target_os = "macos", windows))]
#[test]
fn first_command_and_successive_adjustments_use_public_deviations() {
    let (root, skills) = setup_scope(true);
    let changes = work_infrastructure::fixture_support::file_changes_fixture(&root).unwrap();
    let (code, response) = invoke(
        &root,
        &skills,
        "file-apply",
        "TASK-001",
        Some(json!({
        "schema":"work-file-transaction-request","command":{"kind":"apply","attempt_id":"ATTEMPT-001","changes":changes}})),
    );
    assert_eq!(code, 0, "{response}");
    let task_path = root.join("outputs/work/tasks/example/tasks/TASK-001.json");
    // This fixture publishes a manual Task through Discussion and then obtains Execute authority.
    let task: Value = serde_json::from_slice(&fs::read(task_path).unwrap()).unwrap();
    assert!(task.get("commands").is_none());
    let context = begin_validation(&root, &skills);
    let command = |text: &str| {
        #[cfg(windows)]
        let argv = json!([
            std::env::var("COMSPEC").unwrap(),
            "/d",
            "/c",
            format!("echo {text}")
        ]);
        #[cfg(target_os = "macos")]
        let argv = json!(["/usr/bin/printf", text]);
        json!({"mode":"argv","argv":argv,"execution":{"working_directory":".","os":std::env::consts::OS,
            "shell":if cfg!(windows) {"cmd"} else {"sh"}}})
    };
    let invalid = json!({"gap":"Diagnostic command","action":{"kind":"add_command","command":{"mode":"argv","argv":["missing"]}},
        "modifiable_files":[],"impact":{"summary":"Same review","requirement_changed":false,"scope_changed":false,
        "acceptance_criteria_changed":false,"deliverables_changed":false,"safety_boundary_changed":false,"external_side_effect_boundary_changed":false},"side_effects":[]});
    let (code, response) = invoke(
        &root,
        &skills,
        "deviation-prepare-semantic",
        "TASK-001",
        Some(invalid),
    );
    assert_ne!(code, 0, "{response}");
    assert_eq!(
        response["reason_code"], "deviation_command_execution_invalid",
        "{response}"
    );
    deviation(
        &root,
        &skills,
        json!({"kind":"add_command","command":command("initial")}),
        json!([]),
    );
    let (code, response) = invoke(
        &root,
        &skills,
        "record-finish",
        "TASK-001",
        Some(user_finish(&root, &context, "Manual review completed")),
    );
    assert_eq!(code, 0, "{response}");
    let attempt_path =
        root.join("outputs/work/executions/example/TASK-001/ATTEMPT-001/attempt.json");
    let mut first_record = None;
    let mut first_approval = None;
    for text in ["first", "second"] {
        reserve_record(&root, &skills, "CMD-001");
        deviation(
            &root,
            &skills,
            json!({"kind":"replace_command","replacement":command(text)}),
            json!([]),
        );
        let actual = run_reserved_host_command(&root, &skills);
        assert_eq!(actual["data"]["exit_code"], 0);
        assert_eq!(actual["data"]["stdout_tail"].as_str().unwrap().trim(), text);
        if let Some(first) = &first_approval {
            assert_ne!(&actual["data"]["command_sha256"], first);
            assert_eq!(actual["data"]["record_id"], "CMD-001#1");
        } else {
            first_approval = Some(actual["data"]["command_sha256"].clone());
            assert_eq!(actual["data"]["record_id"], "CMD-001");
        }
        let attempt: Value = serde_json::from_slice(&fs::read(&attempt_path).unwrap()).unwrap();
        if let Some(first) = &first_record {
            assert_eq!(&attempt["records"][1], first);
        } else {
            first_record = Some(attempt["records"][1].clone());
        }
    }
    let (code, response) = invoke(
        &root,
        &skills,
        "attempt-close",
        "TASK-001",
        Some(json!({"schema":"work-attempt-close-request","status":"completed"})),
    );
    assert_eq!(code, 0, "{response}");
}
fn begin_validation(root: &Path, skills: &Path) -> Value {
    reserve_record(root, skills, "VAL-001")
}
fn read_baseline(root: &Path, changes: &Value) -> Value {
    let mut baseline = serde_json::Map::new();
    for change in changes.as_array().unwrap() {
        for key in ["path", "source", "destination"] {
            if let Some(path) = change[key].as_str() {
                let value = match fs::read(root.join(path)) {
                    Ok(bytes) => json!(work_infrastructure::fixture_support::raw_sha256(&bytes)),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Value::Null,
                    Err(e) => panic!("Cannot read baseline: {e}"),
                };
                baseline.insert(path.into(), value);
            }
        }
    }
    Value::Object(baseline)
}

#[cfg(any(target_os = "macos", windows))]
fn setup_inputs() -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "work-read-only-input-cli-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let skills = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../../skills/work"
    ))
    .canonicalize()
    .unwrap();
    #[cfg(windows)]
    let argv = vec![
        std::env::var("COMSPEC").unwrap(),
        "/d".into(),
        "/c".into(),
        "type src\\UserService.java".into(),
    ];
    #[cfg(target_os = "macos")]
    let argv = vec!["/bin/cat".into(), "src/UserService.java".into()];
    work_infrastructure::fixture_support::input_verification_fixture(
        &root,
        &skills,
        &std::collections::BTreeMap::from([(
            "src/UserService.java".into(),
            b"version A\n".to_vec(),
        )]),
        &argv,
    )
    .unwrap();
    (root, skills)
}

#[cfg(any(target_os = "macos", windows))]
#[test]
fn unchanged_declared_inputs_require_fresh_evidence_without_any_file_apply_history() {
    let (root, skills) = setup_inputs();
    let run = || run_owned_command(&root, &skills, "CMD-001");
    let finish = json!({"schema":"work-record-finish-request","record":{"outcome":"passed","evidence":"Actual current input verification"}});
    let close = json!({"schema":"work-attempt-close-request","status":"completed"});
    let attempt_path =
        root.join("outputs/work/executions/example/TASK-001/ATTEMPT-001/attempt.json");
    let first = run();
    assert_eq!(first["data"]["stdout_tail"], "version A\n");
    let original_attempt: Value =
        serde_json::from_slice(&fs::read(&attempt_path).unwrap()).unwrap();
    fs::write(root.join("src/UserService.java"), b"version B\n").unwrap();
    begin_validation(&root, &skills);
    let before_rejection = fs::read(&attempt_path).unwrap();
    let (code, rejected) = invoke(
        &root,
        &skills,
        "record-finish",
        "TASK-001",
        Some(finish.clone()),
    );
    assert_ne!(code, 0, "{rejected}");
    assert_eq!(rejected["reason_code"], "host_result_evidence_drift");
    assert_eq!(fs::read(&attempt_path).unwrap(), before_rejection);
    skip_reserved(&root, &skills);
    assert_eq!(
        fs::read(root.join("src/UserService.java")).unwrap(),
        b"version B\n"
    );
    let (code, rejected) = invoke(
        &root,
        &skills,
        "attempt-close",
        "TASK-001",
        Some(close.clone()),
    );
    assert_ne!(code, 0, "{rejected}");
    assert_eq!(
        rejected["reason_code"],
        "attempt_close_incomplete_validations"
    );
    let second = run();
    assert_eq!(second["data"]["stdout_tail"], "version B\n");
    let (code, response) = invoke_extra(
        &root,
        &skills,
        "record-begin",
        "TASK-001",
        None,
        None,
        &[
            "--record-id",
            "VAL-001",
            "--authorization-evidence",
            "User approves current B validation after obsolete reservation was skipped",
        ],
    );
    assert_eq!(code, 0, "{response}");
    let (code, result) = invoke(&root, &skills, "record-finish", "TASK-001", Some(finish));
    assert_eq!(code, 0, "{result}");
    let (code, result) = invoke(&root, &skills, "attempt-close", "TASK-001", Some(close));
    assert_eq!(code, 0, "{result}");
    let completed: Value = serde_json::from_slice(&fs::read(attempt_path).unwrap()).unwrap();
    assert_eq!(completed["records"][0], original_attempt["records"][0]);
    assert!(
        !root
            .join("outputs/work/runtime/staging/example/project-files")
            .exists()
    );
    assert_eq!(
        fs::read(root.join("src/UserService.java")).unwrap(),
        b"version B\n"
    );
}

#[test]
fn later_new_path_still_requires_its_original_read_baseline() {
    let (root, skills) = setup_scope(true);
    work_infrastructure::fixture_support::authorize_supplemental_file_fixture(
        &root,
        "TASK-001",
        "supplement.txt",
    )
    .unwrap();
    let changes = work_infrastructure::fixture_support::file_changes_fixture(&root).unwrap();
    let (code, response) = invoke(
        &root,
        &skills,
        "file-apply",
        "TASK-001",
        Some(json!({
            "schema":"work-file-transaction-request","command":{"kind":"apply","attempt_id":"ATTEMPT-001","changes":changes}
        })),
    );
    assert_eq!(code, 0, "{response}");
    fs::write(root.join("supplement.txt"), b"read version").unwrap();
    let changes =
        json!([{"kind":"write","path":"supplement.txt","bytes":b"AI replacement".to_vec()}]);
    let baseline = read_baseline(&root, &changes);
    fs::write(root.join("supplement.txt"), b"later user version").unwrap();
    let (code, response) = invoke(
        &root,
        &skills,
        "file-apply",
        "TASK-001",
        Some(json!({
            "schema":"work-file-transaction-request","command":{"kind":"apply","attempt_id":"ATTEMPT-001","changes":changes,"expected_before":baseline}
        })),
    );
    assert_ne!(code, 0);
    assert_eq!(
        response["reason_code"],
        "file_transaction_read_baseline_stale"
    );
    assert_eq!(
        fs::read(root.join("supplement.txt")).unwrap(),
        b"later user version"
    );
}

#[test]
fn completed_tasks_supplemental_files_block_independent_later_writers() {
    for ordered in [false, true] {
        let (root, skills) = setup_fixture(true, false, false, Some(ordered));
        work_infrastructure::fixture_support::authorize_supplemental_file_fixture(
            &root,
            "TASK-001",
            "shared.txt",
        )
        .unwrap();
        if !ordered {
            work_infrastructure::fixture_support::authorize_supplemental_file_fixture(
                &root,
                "TASK-001",
                "independent.txt",
            )
            .unwrap();
            let (code, response) = invoke(
                &root,
                &skills,
                "file-apply",
                "TASK-001",
                Some(json!({
                    "schema":"work-file-transaction-request","command":{"kind":"apply","attempt_id":"ATTEMPT-001",
                        "changes":[{"kind":"write","path":"independent.txt","bytes":[10]}]}
                })),
            );
            assert_ne!(code, 0);
            assert_eq!(response["reason_code"], "task_file_dependency_required");
            assert!(!root.join("independent.txt").exists());
        }
        let changes = work_infrastructure::fixture_support::file_changes_fixture(&root).unwrap();
        let mut changes = serde_json::to_value(changes).unwrap();
        changes
            .as_array_mut()
            .unwrap()
            .push(json!({"kind":"write","path":"shared.txt","bytes":b"first task".to_vec()}));
        let (code, response) = invoke(
            &root,
            &skills,
            "file-apply",
            "TASK-001",
            Some(json!({
                "schema":"work-file-transaction-request","command":{"kind":"apply","attempt_id":"ATTEMPT-001","changes":changes}
            })),
        );
        assert_eq!(code, 0, "{response}");
        let context = begin_validation(&root, &skills);
        let (code, response) = invoke(
            &root,
            &skills,
            "record-finish",
            "TASK-001",
            Some(user_finish(
                &root,
                &context,
                "Verified complete first Task including shared output",
            )),
        );
        assert_eq!(code, 0, "{response}");
        let (code, response) = invoke(
            &root,
            &skills,
            "attempt-close",
            "TASK-001",
            Some(json!({"schema":"work-attempt-close-request","status":"completed"})),
        );
        assert_eq!(code, 0, "{response}");
        let choice = json!({"command_positions":[],"validation_positions":[1],"modifiable_files":["independent.txt"],
            "external_operation_positions":[],"allowed_deviations":[],"authorization_evidence":"Approve second Task and supplemental shared output",
            "scope":{"project_root":root.to_string_lossy(),"requirement_id":"example","goal":"Complete second output","module_paths":["."],
                "modification_types":["create","modify"],"risk_boundary":"Local files","validation_scope":["Declared validations"],
                "task_ids":["TASK-002"],"adjustments":["additional_files"],"revoked_reason":null}});
        let (code, response) = invoke(
            &root,
            &skills,
            "attempt-start-prepare",
            "TASK-002",
            Some(choice),
        );
        assert_eq!(code, 0, "{response}");
        let (code, response) = invoke(
            &root,
            &skills,
            "attempt-start",
            "TASK-002",
            Some(response["data"]["request"].clone()),
        );
        assert_eq!(code, 0, "{response}");
        work_infrastructure::fixture_support::authorize_supplemental_file_fixture(
            &root,
            "TASK-002",
            "shared.txt",
        )
        .unwrap();
        let (code, response) = invoke(
            &root,
            &skills,
            "file-apply",
            "TASK-002",
            Some(json!({
                "schema":"work-file-transaction-request","command":{"kind":"apply","attempt_id":"ATTEMPT-001",
                    "changes":[{"kind":"write","path":"shared.txt","bytes":b"second task".to_vec()}]}
            })),
        );
        if ordered {
            assert_eq!(code, 0, "{response}");
            assert_eq!(fs::read(root.join("shared.txt")).unwrap(), b"second task");
        } else {
            assert_ne!(code, 0, "{response}");
            assert_eq!(response["reason_code"], "task_file_dependency_required");
            assert_eq!(fs::read(root.join("shared.txt")).unwrap(), b"first task");
        }
    }
}

#[test]
fn read_time_baseline_rejects_first_write_and_new_paths_without_overwriting_user_changes() {
    for path in ["c-existing.txt", "d-new.txt", "a-old.txt", "b-moved.txt"] {
        let (root, skills) = setup();
        let changes = work_infrastructure::fixture_support::file_changes_fixture(&root).unwrap();
        let changes = serde_json::to_value(changes).unwrap();
        let baseline = read_baseline(&root, &changes);
        fs::write(root.join(path), b"later user contents").unwrap();
        let (code, response) = invoke(
            &root,
            &skills,
            "file-apply",
            "TASK-001",
            Some(json!({
                "schema":"work-file-transaction-request","command":{"kind":"apply",
                "attempt_id":"ATTEMPT-001","changes":changes,"expected_before":baseline}
            })),
        );
        assert_ne!(code, 0, "{response}");
        assert_eq!(fs::read(root.join(path)).unwrap(), b"later user contents");
        assert!(
            !root
                .join("outputs/work/runtime/staging/example/project-files")
                .exists()
        );
    }
}

#[test]
fn baseline_requires_the_complete_path_inventory_and_explicit_absence() {
    for baseline in [
        json!({}),
        json!({"d-new.txt":"0".repeat(64)}),
        json!({"d-new.txt":null,"extra.txt":null}),
        Value::Null,
    ] {
        let (root, skills) = setup();
        let (code, response) = invoke(
            &root,
            &skills,
            "file-apply",
            "TASK-001",
            Some(json!({
                "schema":"work-file-transaction-request","command":{"kind":"apply",
                "attempt_id":"ATTEMPT-001","changes":[{"kind":"write","path":"d-new.txt","bytes":[10]}],
                "expected_before":baseline}
            })),
        );
        assert_ne!(code, 0, "{response}");
        assert!(!root.join("d-new.txt").exists());
    }
}
fn reserve_record(root: &Path, skills: &Path, id: &str) -> Value {
    let args = vec![
        "--project-root".into(),
        root.to_string_lossy().into_owned(),
        "execute".into(),
        "record-begin".into(),
        "--task-path".into(),
        "outputs/work/tasks/example/index.json".into(),
        "--execution-dir".into(),
        "outputs/work/executions/example".into(),
        "--task-id".into(),
        "TASK-001".into(),
        "--user-config-root".into(),
        root.to_string_lossy().into_owned(),
        "--record-id".into(),
        id.into(),
    ];
    let (code, result) = work_cli::runtime::run_with_skill_root(&args, skills);
    assert_eq!(code, 0, "{result:?}");
    let response = serde_json::to_value(result).unwrap();
    if id.starts_with("VAL-") {
        assert!(
            response["data"]["validation_context"]["source"].is_string(),
            "{response}"
        );
    }
    response["data"]["validation_context"].clone()
}
fn user_finish(root: &Path, context: &Value, statement: &str) -> Value {
    work_infrastructure::fixture_support::user_confirmation_fixture(
        root,
        context,
        work_model::execution::attempt::ValidationOutcome::Passed,
        statement,
    )
    .unwrap()
}

#[test]
fn host_file_record_requires_complete_atomic_evidence_and_current_bytes() {
    let (root, skills) = setup();
    let bytes = b"Host-edited final contents\n";
    let second = b"Host-edited second file\n";
    let request = json!({"schema":"work-file-transaction-request","command":{
        "kind":"bind","attempt_id":"ATTEMPT-001",
        "changes":[
            {"kind":"write","path":"c-existing.txt","bytes":bytes},
            {"kind":"write","path":"a-old.txt","bytes":second}
        ]
    }});
    let (code, response) = invoke(&root, &skills, "file-binding", "TASK-001", Some(request));
    assert_eq!(code, 0, "{response}");
    let manifest = &response["data"]["manifest"];
    let paths: serde_json::Map<String, Value> = manifest["targets"].as_array().unwrap().iter().map(|target| (
        target["path"].as_str().unwrap().to_owned(),
        json!({"before_sha256":target["before"]["sha256"],"after_sha256":target["after"]["sha256"]})
    )).collect();
    let identity = manifest["transaction_identity"].as_str().unwrap();
    let mut evidence = json!({
        "schema":"work-host-file-evidence", "task_id":"TASK-001", "attempt_id":"ATTEMPT-001",
        "transaction_identity":identity, "status":"completed_atomic",
        "source_reference":"Host edit operation", "atomicity_statement":"The complete set was committed atomically",
        "paths":paths
    });
    let evidence_path = root.join("host-file-evidence.json");
    fs::write(&evidence_path, serde_json::to_vec(&evidence).unwrap()).unwrap();
    let report = |status: &str| {
        json!({
            "schema":"work-host-file-report", "task_id":"TASK-001", "attempt_id":"ATTEMPT-001",
            "transaction_identity":identity, "approval_sha256":manifest["approval_sha256"],
            "result_origin":"host_reported", "result_reference":"host-file-evidence.json",
            "result_sha256":work_infrastructure::codec::fingerprint::raw(&fs::read(&evidence_path).unwrap()),
            "status":status
        })
    };
    let record = |report: Value| json!({"schema":"work-file-transaction-request","command":{"kind":"record","report":report}});
    assert_ne!(
        invoke(
            &root,
            &skills,
            "file-record",
            "TASK-001",
            Some(record(report("partial")))
        )
        .0,
        0
    );
    assert_ne!(
        invoke(
            &root,
            &skills,
            "file-record",
            "TASK-001",
            Some(record(report("unknown")))
        )
        .0,
        0
    );
    assert_ne!(
        invoke(
            &root,
            &skills,
            "file-record",
            "TASK-001",
            Some(record(report("completed_atomic")))
        )
        .0,
        0
    );
    fs::write(root.join("c-existing.txt"), bytes).unwrap();
    assert_ne!(
        invoke(
            &root,
            &skills,
            "file-record",
            "TASK-001",
            Some(record(report("completed_atomic")))
        )
        .0,
        0
    );
    fs::write(root.join("a-old.txt"), second).unwrap();
    evidence["status"] = json!("partial");
    fs::write(&evidence_path, serde_json::to_vec(&evidence).unwrap()).unwrap();
    assert_ne!(
        invoke(
            &root,
            &skills,
            "file-record",
            "TASK-001",
            Some(record(report("completed_atomic")))
        )
        .0,
        0
    );
    evidence["status"] = json!("completed_atomic");
    fs::write(&evidence_path, serde_json::to_vec(&evidence).unwrap()).unwrap();
    let (code, result) = invoke(
        &root,
        &skills,
        "file-record",
        "TASK-001",
        Some(record(report("completed_atomic"))),
    );
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["data"]["phase"], "published-verified");
    assert_eq!(fs::read(root.join("c-existing.txt")).unwrap(), bytes);
    assert_eq!(fs::read(root.join("a-old.txt")).unwrap(), second);
    evidence["status"] = json!("unknown");
    fs::write(&evidence_path, serde_json::to_vec(&evidence).unwrap()).unwrap();
    let next = json!({"schema":"work-file-transaction-request","command":{
        "kind":"bind","attempt_id":"ATTEMPT-001",
        "changes":[{"kind":"write","path":"c-existing.txt","bytes":b"next"}]
    }});
    let (code, rejected) = invoke(&root, &skills, "file-binding", "TASK-001", Some(next));
    assert_ne!(code, 0);
    assert_eq!(
        rejected["reason_code"],
        "file_transaction_host_evidence_drift"
    );
}

#[test]
fn retired_public_file_writer_rejects_before_reading_input() {
    let (root, skills) = setup();
    let formal = root.join("outputs/work/executions/example/index.json");
    let original = fs::read(&formal).unwrap();
    for command in [
        "file-prepare",
        "file-apply",
        "file-recovery-prepare",
        "file-restore",
    ] {
        let args = vec![
            "--project-root".into(),
            root.to_string_lossy().into_owned(),
            "execute".into(),
            command.into(),
            "--user-config-root".into(),
            root.to_string_lossy().into_owned(),
            "--task-path".into(),
            "outputs/work/tasks/example/index.json".into(),
            "--execution-dir".into(),
            "outputs/work/executions/example".into(),
            "--task-id".into(),
            "TASK-001".into(),
            "--input-file".into(),
            root.join("missing-request.json")
                .to_string_lossy()
                .into_owned(),
        ];
        let (code, result) = work_cli::runtime::run_with_skill_root(&args, &skills);
        assert_ne!(code, 0, "{command}");
        assert_eq!(result.reason_code, "file_writer_retired", "{command}");
        assert_eq!(fs::read(&formal).unwrap(), original);
    }
}
#[test]
fn direct_changes_publish_under_existing_authority_then_acceptance_enables_downstream_task() {
    let (root, skills) = setup();
    let changes = work_infrastructure::fixture_support::file_changes_fixture(&root).unwrap();
    let request = json!({"schema":"work-file-transaction-request","command":{"kind":"prepare","attempt_id":"ATTEMPT-001","changes":changes}});
    let (code, response) = invoke(
        &root,
        &skills,
        "file-prepare",
        "TASK-001",
        Some(request.clone()),
    );
    assert_eq!(code, 0, "{response}");
    let preview = response["data"].clone();
    assert_eq!(preview["manifest"]["targets"].as_array().unwrap().len(), 4);
    assert!(
        !root
            .join("outputs/work/runtime/staging/example/project-files")
            .exists()
    );
    assert_eq!(
        fs::read(root.join("a-old.txt")).unwrap(),
        b"original move\r\n"
    );
    assert_ne!(invoke(&root, &skills, "preflight", "TASK-002", None).0, 0);
    let mut wrong = json!({"schema":"work-file-transaction-request","command":{"kind":"apply","preview":preview,"approved_sha256":"0".repeat(64),"authorization_evidence":"Approve wrong preview"}});
    assert_ne!(
        invoke(
            &root,
            &skills,
            "file-apply",
            "TASK-001",
            Some(wrong.clone())
        )
        .0,
        0
    );
    assert!(!root.join("d-new.txt").exists());
    wrong = json!({"schema":"work-file-transaction-request","command":{"kind":"apply","attempt_id":"ATTEMPT-001","changes":changes}});
    let (code, response) = invoke(&root, &skills, "file-apply", "TASK-001", Some(wrong));
    assert_eq!(code, 0, "{response}");
    assert_eq!(response["data"]["phase"], "published-verified");
    assert!(!root.join("a-old.txt").exists());
    assert!(root.join("d-new.txt").exists());
    let close = json!({"schema":"work-attempt-close-request","status":"completed"});
    assert_ne!(
        invoke(
            &root,
            &skills,
            "attempt-close",
            "TASK-001",
            Some(close.clone())
        )
        .0,
        0
    );
    let validation_context = begin_validation(&root, &skills);
    let (code, response) = invoke(
        &root,
        &skills,
        "record-finish",
        "TASK-001",
        Some(user_finish(
            &root,
            &validation_context,
            "User verified all published create/modify/move paths and complete bytes",
        )),
    );
    assert_eq!(code, 0, "{response}");
    let (code, response) = invoke(&root, &skills, "attempt-close", "TASK-001", Some(close));
    assert_eq!(code, 0, "{response}");
    assert_eq!(response["data"]["task_status"], "completed");
    let (code, response) = invoke(&root, &skills, "preflight", "TASK-002", None);
    assert_eq!(code, 0, "{response}");
    assert_eq!(response["data"]["dependencies"], json!(["TASK-001"]));
}

#[test]
fn direct_changes_do_not_depend_on_caller_managed_staging_or_publication_approval() {
    let (root, skills) = setup();
    let changes = work_infrastructure::fixture_support::file_changes_fixture(&root).unwrap();
    let original = fs::read(root.join("c-existing.txt")).unwrap();
    fs::write(
        root.join("outputs/work/transactions/example/file-test/staging/d-new.txt"),
        b"unrelated generated bytes",
    )
    .unwrap();
    let (code, response) = invoke(
        &root,
        &skills,
        "file-apply",
        "TASK-001",
        Some(
            json!({"schema":"work-file-transaction-request","command":{"kind":"apply","attempt_id":"ATTEMPT-001","changes":changes}}),
        ),
    );
    assert_eq!(code, 0, "{response}");
    assert_eq!(
        fs::read(root.join("d-new.txt")).unwrap(),
        b"new complete output\n"
    );
    let identity = response["data"]["transaction_identity"].as_str().unwrap();
    let manifest: Value = serde_json::from_slice(
        &fs::read(root.join(format!(
            "outputs/work/runtime/staging/example/project-files/{identity}/transaction.json"
        )))
        .unwrap(),
    )
    .unwrap();
    assert!(
        manifest["targets"]
            .as_array()
            .unwrap()
            .iter()
            .any(|target| target["path"] == "c-existing.txt"
                && target["before"]["bytes"] == json!(original))
    );
}

#[test]
fn interrupted_publication_blocks_closure_and_downstream_until_fresh_authorized_restore() {
    let (root, skills) = setup();
    let target = work_flow::execution::ExecutionProjectTarget {
        task_path: "outputs/work/tasks/example/index.json",
        execution_dir: "outputs/work/executions/example",
        task_id: "TASK-001",
    };
    let context =
        work_cli::runtime::prepare_execution_writer_context(&root, &skills, &[], target).unwrap();
    let identity = work_infrastructure::fixture_support::interrupt_file_transaction_fixture(
        &context, &skills, 2,
    )
    .unwrap();
    assert_ne!(
        invoke(
            &root,
            &skills,
            "attempt-close",
            "TASK-001",
            Some(json!({"schema":"work-attempt-close-request","status":"completed"}))
        )
        .0,
        0
    );
    assert_ne!(invoke(&root, &skills, "preflight", "TASK-002", None).0, 0);
    let request = json!({"schema":"work-file-transaction-request","command":{"kind":"recovery-prepare","transaction_identity":identity}});
    let (code, response) = invoke(
        &root,
        &skills,
        "file-recovery-prepare",
        "TASK-001",
        Some(request),
    );
    assert_eq!(code, 0, "{response}");
    let recovery = response["data"].clone();
    let mut restore = json!({"schema":"work-file-transaction-request","command":{"kind":"restore","transaction_identity":identity,"approved_sha256":recovery["approval_sha256"],"authorization_evidence":""}});
    assert_ne!(
        invoke(
            &root,
            &skills,
            "file-restore",
            "TASK-001",
            Some(restore.clone())
        )
        .0,
        0
    );
    restore["command"]["authorization_evidence"] =
        json!("User freshly approved restore of this exact before/actual set");
    let (code, response) = invoke(&root, &skills, "file-restore", "TASK-001", Some(restore));
    assert_eq!(code, 0, "{response}");
    assert_eq!(response["data"]["phase"], "restored");
    assert_eq!(
        fs::read(root.join("a-old.txt")).unwrap(),
        b"original move\r\n"
    );
    assert_eq!(
        fs::read(root.join("c-existing.txt")).unwrap(),
        b"original modify\0bytes\n"
    );
    assert!(!root.join("b-moved.txt").exists());
    assert!(!root.join("d-new.txt").exists());
    assert_ne!(invoke(&root, &skills, "preflight", "TASK-002", None).0, 0);
    let validation_context = begin_validation(&root, &skills);
    let (code, response) = invoke(
        &root,
        &skills,
        "record-finish",
        "TASK-001",
        Some(user_finish(
            &root,
            &validation_context,
            "Manual record cannot substitute for missing published outcome",
        )),
    );
    assert_eq!(code, 0, "{response}");
    let (code, response) = invoke(
        &root,
        &skills,
        "attempt-close",
        "TASK-001",
        Some(json!({"schema":"work-attempt-close-request","status":"completed"})),
    );
    assert_ne!(code, 0);
    assert_eq!(
        response["reason_code"],
        "file_transaction_publication_required"
    );
    let (code, response) = invoke(
        &root,
        &skills,
        "attempt-close",
        "TASK-001",
        Some(
            json!({"schema":"work-attempt-close-request","status":"stopped","final_type":"other","reason":"Publication restored; authorize fresh execution","authorization_evidence":"Approve closing this restored attempt"}),
        ),
    );
    assert_eq!(code, 0, "{response}");
    let choice = json!({"command_positions":[],"validation_positions":[1],"modifiable_files":["a-old.txt","b-moved.txt","c-existing.txt","d-new.txt"],"external_operation_positions":[],"allowed_deviations":[],"authorization_evidence":"Fresh approval to retry restored publication"});
    let (code, response) = invoke(
        &root,
        &skills,
        "attempt-start-prepare",
        "TASK-001",
        Some(choice),
    );
    assert_eq!(code, 0, "{response}");
    let (code, response) = invoke(
        &root,
        &skills,
        "attempt-start",
        "TASK-001",
        Some(response["data"]["request"].clone()),
    );
    assert_eq!(code, 0, "{response}");
    let changes = work_infrastructure::fixture_support::file_changes_fixture(&root).unwrap();
    let (code, response) = invoke(
        &root,
        &skills,
        "file-prepare",
        "TASK-001",
        Some(
            json!({"schema":"work-file-transaction-request","command":{"kind":"prepare","attempt_id":"ATTEMPT-002","changes":changes}}),
        ),
    );
    assert_eq!(code, 0, "{response}");
    let (code, response) = invoke(
        &root,
        &skills,
        "file-apply",
        "TASK-001",
        Some(
            json!({"schema":"work-file-transaction-request","command":{"kind":"apply","attempt_id":"ATTEMPT-002","changes":changes}}),
        ),
    );
    assert_eq!(code, 0, "{response}");
    let validation_context = begin_validation(&root, &skills);
    let (code, response) = invoke(
        &root,
        &skills,
        "record-finish",
        "TASK-001",
        Some(user_finish(
            &root,
            &validation_context,
            "Fresh verification of retry publication",
        )),
    );
    assert_eq!(code, 0, "{response}");
    let (code, response) = invoke(
        &root,
        &skills,
        "attempt-close",
        "TASK-001",
        Some(json!({"schema":"work-attempt-close-request","status":"completed"})),
    );
    assert_eq!(code, 0, "{response}");
    assert_eq!(invoke(&root, &skills, "preflight", "TASK-002", None).0, 0);
}

#[test]
fn multiple_changes_invalidate_old_validation_and_confirmed_user_contents_become_the_new_baseline()
{
    let (root, skills) = setup_scope(true);
    let changes = work_infrastructure::fixture_support::file_changes_fixture(&root).unwrap();
    let (code, first) = invoke(
        &root,
        &skills,
        "file-apply",
        "TASK-001",
        Some(json!({"schema":"work-file-transaction-request",
        "command":{"kind":"apply","attempt_id":"ATTEMPT-001","changes":changes}})),
    );
    assert_eq!(code, 0, "{first}");
    let validation_context = begin_validation(&root, &skills);
    let finish = user_finish(
        &root,
        &validation_context,
        "User verified this current version",
    );
    let (code, response) = invoke(
        &root,
        &skills,
        "record-finish",
        "TASK-001",
        Some(finish.clone()),
    );
    assert_eq!(code, 0, "{response}");
    fs::write(
        root.join("c-existing.txt"),
        b"new user contents to preserve",
    )
    .unwrap();
    let command = json!({"schema":"work-file-transaction-request","command":{"kind":"apply","attempt_id":"ATTEMPT-001",
        "changes":[{"kind":"write","path":"c-existing.txt","bytes":b"user contents plus complete correction".to_vec()}]}});
    let (code, required) = invoke(
        &root,
        &skills,
        "file-apply",
        "TASK-001",
        Some(command.clone()),
    );
    assert_ne!(code, 0);
    assert_eq!(
        required["reason_code"],
        "file_transaction_baseline_confirmation_required"
    );
    assert_eq!(
        fs::read(root.join("c-existing.txt")).unwrap(),
        b"new user contents to preserve"
    );
    let mut confirmed = command;
    confirmed["command"]["baseline_confirmation"] = json!({
        "previous_transaction":required["data"]["previous_transaction"],
        "before_sha256":required["data"]["before_sha256"],"evidence":"User explicitly confirmed these new current bytes and metadata"});
    let (code, second) = invoke(&root, &skills, "file-apply", "TASK-001", Some(confirmed));
    assert_eq!(code, 0, "{second}");
    let close = json!({"schema":"work-attempt-close-request","status":"completed"});
    let (code, rejected) = invoke(
        &root,
        &skills,
        "attempt-close",
        "TASK-001",
        Some(close.clone()),
    );
    assert_ne!(code, 0);
    assert_eq!(rejected["reason_code"], "validation_execution_unverified");
    assert_eq!(
        rejected["data"]["evidence_stage"],
        "validation_file_version"
    );
    assert_ne!(invoke(&root, &skills, "preflight", "TASK-002", None).0, 0);
    let validation_context = begin_validation(&root, &skills);
    let finish = user_finish(
        &root,
        &validation_context,
        "User verified the corrected current version",
    );
    let (code, fresh) = invoke(&root, &skills, "record-finish", "TASK-001", Some(finish));
    assert_eq!(code, 0, "{fresh}");
    let (code, closed) = invoke(&root, &skills, "attempt-close", "TASK-001", Some(close));
    assert_eq!(code, 0, "{closed}");
    assert_eq!(invoke(&root, &skills, "preflight", "TASK-002", None).0, 0);
    let path = root.join(format!(
        "outputs/work/runtime/staging/example/project-files/{}/transaction.json",
        second["data"]["transaction_identity"].as_str().unwrap()
    ));
    let manifest: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(
        manifest["business_identity"]["previous_transaction"],
        first["data"]["transaction_identity"]
    );
    assert_eq!(
        manifest["targets"][0]["before"]["bytes"],
        json!(b"new user contents to preserve".to_vec())
    );
}

#[cfg(any(target_os = "macos", windows))]
#[test]
fn actual_failing_test_keeps_published_contents_then_scoped_correction_and_revalidation_complete() {
    let (root, skills) = setup_config(true, true);
    let run_command = |id: &str| -> Value { run_owned_command(&root, &skills, id) };
    assert_eq!(run_command("CMD-001")["data"]["exit_code"], 0);
    let mut changes = work_infrastructure::fixture_support::file_changes_fixture(&root).unwrap();
    for change in &mut changes {
        if let work_model::execution::file_transaction::FileChange::Write { path, bytes } = change
            && path == "c-existing.txt"
        {
            *bytes = b"known failing output\n".to_vec();
        }
    }
    let (code, first) = invoke(
        &root,
        &skills,
        "file-apply",
        "TASK-001",
        Some(json!({"schema":"work-file-transaction-request",
        "command":{"kind":"apply","attempt_id":"ATTEMPT-001","changes":changes}})),
    );
    assert_eq!(code, 0, "{first}");
    assert_ne!(run_command("CMD-002")["data"]["exit_code"], 0);
    assert_eq!(
        fs::read(root.join("c-existing.txt")).unwrap(),
        b"known failing output\n"
    );
    begin_validation(&root, &skills);
    let (code, failed) = invoke(
        &root,
        &skills,
        "record-finish",
        "TASK-001",
        Some(json!({"schema":"work-record-finish-request",
        "record":{"outcome":"failed","evidence":"Actual procedure failed for the published version"}})),
    );
    assert_eq!(code, 0, "{failed}");
    let close = json!({"schema":"work-attempt-close-request","status":"completed"});
    assert_ne!(
        invoke(
            &root,
            &skills,
            "attempt-close",
            "TASK-001",
            Some(close.clone())
        )
        .0,
        0
    );
    let (code, corrected) = invoke(
        &root,
        &skills,
        "file-apply",
        "TASK-001",
        Some(json!({"schema":"work-file-transaction-request",
        "command":{"kind":"apply","attempt_id":"ATTEMPT-001","changes":[{"kind":"write","path":"c-existing.txt","bytes":b"final modify\n".to_vec()}]}})),
    );
    assert_eq!(code, 0, "{corrected}");
    assert_eq!(run_command("CMD-002")["data"]["exit_code"], 0);
    begin_validation(&root, &skills);
    let (code, passed) = invoke(
        &root,
        &skills,
        "record-finish",
        "TASK-001",
        Some(json!({"schema":"work-record-finish-request",
        "record":{"outcome":"passed","evidence":"Fresh actual native procedure passed for the corrected version"}})),
    );
    assert_eq!(code, 0, "{passed}");
    let (code, completed) = invoke(&root, &skills, "attempt-close", "TASK-001", Some(close));
    assert_eq!(code, 0, "{completed}");
    assert_eq!(invoke(&root, &skills, "preflight", "TASK-002", None).0, 0);
    let identity = first["data"]["transaction_identity"].as_str().unwrap();
    let retained: Value = serde_json::from_slice(
        &fs::read(root.join(format!(
            "outputs/work/runtime/staging/example/project-files/{identity}/transaction.json"
        )))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        retained["phase"], "published-verified",
        "a failing test never reverses a complete write"
    );
}

#[cfg(any(target_os = "macos", windows))]
#[test]
fn all_three_evidence_sources_are_required_and_cannot_impersonate_each_other() {
    let (root, skills) = setup_methods(true, true, true);
    let command = |id: &str| run_owned_command(&root, &skills, id);
    command("CMD-001");
    let changes = work_infrastructure::fixture_support::file_changes_fixture(&root).unwrap();
    let (code, applied) = invoke(
        &root,
        &skills,
        "file-apply",
        "TASK-001",
        Some(
            json!({"schema":"work-file-transaction-request","command":{"kind":"apply","attempt_id":"ATTEMPT-001","changes":changes}}),
        ),
    );
    assert_eq!(code, 0, "{applied}");
    let command_result = command("CMD-002");
    let finished_sha = command_result["data"]["result_sha256"].as_str().unwrap();
    let close = json!({"schema":"work-attempt-close-request","status":"completed"});
    let blocked = || {
        assert_ne!(
            invoke(
                &root,
                &skills,
                "attempt-close",
                "TASK-001",
                Some(close.clone())
            )
            .0,
            0
        );
        assert_ne!(invoke(&root, &skills, "preflight", "TASK-002", None).0, 0);
    };
    let finish =
        |request: Value| invoke(&root, &skills, "record-finish", "TASK-001", Some(request));
    let mut spoof = json!({"schema":"work-record-finish-request","record":{"outcome":"passed","source":"procedure"}});
    let context = reserve_record(&root, &skills, "VAL-001");
    assert_eq!(context["source"], "procedure");
    assert_ne!(finish(spoof.clone()).0, 0);
    spoof["record"].as_object_mut().unwrap().remove("source");
    let (code, result) = finish(spoof.clone());
    assert_eq!(code, 0, "{result}");
    blocked();
    let context = reserve_record(&root, &skills, "VAL-002");
    assert_eq!(context["source"], "ai_review");
    assert_ne!(finish(spoof.clone()).0, 0); // A bare passed label is not an AI review.
    let hash = work_infrastructure::fixture_support::raw_sha256(
        &fs::read(root.join("c-existing.txt")).unwrap(),
    );
    let mut review = json!({"schema":"work-record-finish-request","record":{"outcome":"passed","review":{
    "reviewer":"fixture-reviewer","rationale":"Reviewed the exact complete modified file and owned publication", "references":[
        {"kind":"artifact","path":"c-existing.txt","sha256":hash,"detail":"Complete current file bytes"},
        {"kind":"change","transaction_identity":applied["data"]["transaction_identity"],"path":"c-existing.txt",
         "before_sha256":work_infrastructure::fixture_support::raw_sha256(b"original modify\0bytes\n"),
         "after_sha256":hash,"detail":"Exact retained before/after modification"},
            {"kind":"command","record_id":"CMD-002","finished_sha256":finished_sha,"detail":"Latest owned complete procedure for this exact file version"}
    ]}}});
    for (pointer, value) in [
        ("/record/review/reviewer", json!("fixture-user")),
        ("/record/review/rationale", json!("")),
        ("/record/review/references", json!([])),
        ("/record/review/references/0/sha256", json!("0".repeat(64))),
        (
            "/record/review/references/1/transaction_identity",
            json!("0".repeat(64)),
        ),
        (
            "/record/review/references/1/before_sha256",
            json!("0".repeat(64)),
        ),
        ("/record/review/references/2/record_id", json!("CMD-001")),
        ("/record/review/references/2/record_id", json!("CMD-003")),
        (
            "/record/review/references/2/finished_sha256",
            json!("0".repeat(64)),
        ),
    ] {
        let mut forged = review.clone();
        *forged.pointer_mut(pointer).unwrap() = value;
        assert_ne!(finish(forged).0, 0, "{pointer}");
    }
    let (code, result) = finish(review.take());
    assert_eq!(code, 0, "{result}");
    blocked();
    let context = reserve_record(&root, &skills, "VAL-003");
    assert_eq!(context["source"], "user_confirmation");
    assert_ne!(finish(spoof).0, 0); // AI text cannot become the designated user's statement.
    let request = user_finish(
        &root,
        &context,
        "Synthetic designated user approves these exact fixture outputs",
    );
    let path = root.join(request["record"]["confirmation"]["path"].as_str().unwrap());
    let original = fs::read(&path).unwrap();
    let document: Value = serde_json::from_slice(&original).unwrap();
    for (field, value) in [
        ("source", json!("ai_review")),
        ("confirmer", json!("fixture-reviewer")),
        ("task_id", json!("TASK-002")),
        ("attempt_id", json!("ATTEMPT-002")),
        ("validation_id", json!("VAL-002")),
        ("record_id", json!("VAL-003#1")),
        ("file_version", json!("0".repeat(64))),
        ("outcome", json!("failed")),
    ] {
        let mut changed = document.clone();
        changed[field] = value;
        let raw = serde_json::to_vec(&changed).unwrap();
        fs::write(&path, &raw).unwrap();
        let mut forged = request.clone();
        forged["record"]["confirmation"]["sha256"] =
            json!(work_infrastructure::fixture_support::raw_sha256(&raw));
        assert_ne!(finish(forged).0, 0, "{field}");
    }
    fs::write(&path, &original).unwrap();
    let (code, result) = finish(request);
    assert_eq!(code, 0, "{result}");
    let attempt: Value = serde_json::from_slice(
        &fs::read(root.join("outputs/work/executions/example/TASK-001/ATTEMPT-001/attempt.json"))
            .unwrap(),
    )
    .unwrap();
    let sources: Vec<_> = attempt["records"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["kind"] == "validation")
        .map(|r| r["source"].clone())
        .collect();
    assert_eq!(
        sources,
        json!(["procedure", "ai_review", "user_confirmation"])
            .as_array()
            .unwrap()
            .clone()
    );
    let (code, result) = invoke(&root, &skills, "attempt-close", "TASK-001", Some(close));
    assert_eq!(code, 0, "{result}");
    assert_eq!(invoke(&root, &skills, "preflight", "TASK-002", None).0, 0);
}

#[cfg(any(target_os = "macos", windows))]
#[test]
fn development_fixture_retains_dirty_baseline_and_an_independent_task_without_a_false_dependency() {
    let root = std::env::temp_dir().join(format!(
        "work-development-cli-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let skills = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../../skills/work"
    ))
    .canonicalize()
    .unwrap();
    let project = std::collections::BTreeMap::from([(
        "development/project.txt".into(),
        b"existing project source\n".to_vec(),
    )]);
    #[cfg(target_os = "macos")]
    let argv = vec![
        "/bin/sh".into(),
        "-c".into(),
        "cat c-existing.txt; grep -q 'final modify' c-existing.txt".into(),
    ];
    #[cfg(windows)]
    let argv = vec![
        std::env::var("COMSPEC").unwrap(),
        "/d".into(),
        "/c".into(),
        "type c-existing.txt & findstr /c:\"final modify\" c-existing.txt >nul".into(),
    ];
    let context = work_infrastructure::fixture_support::development_iteration_fixture(
        &root, &skills, &project, &argv,
    )
    .unwrap();
    assert_eq!(
        fs::read(root.join("c-existing.txt")).unwrap(),
        b"existing uncommitted user contents\n"
    );
    assert_eq!(
        context.task_context().collection["tasks"][1]["dependencies"],
        json!(["TASK-001"])
    );
    assert!(
        context.task_context().collection["tasks"][2]["dependencies"]
            .as_array()
            .is_none_or(Vec::is_empty)
    );
    // Independence does not bypass the shared active-writer lock.
    let (code, blocked) = invoke(&root, &skills, "preflight", "TASK-003", None);
    assert_ne!(code, 0);
    assert_eq!(blocked["reason_code"], "execute_preflight_lock_present");
    assert_eq!(
        fs::read(root.join("development/project.txt")).unwrap(),
        b"existing project source\n"
    );
    assert!(!root.join("independent.txt").exists());
    let changes = work_infrastructure::fixture_support::file_changes_fixture(&root).unwrap();
    let (code, response) = invoke(
        &root,
        &skills,
        "file-prepare",
        "TASK-001",
        Some(json!({
            "schema":"work-file-transaction-request","command":{"kind":"prepare","attempt_id":"ATTEMPT-001","changes":changes}
        })),
    );
    assert_eq!(code, 0, "{response}");
    let binding = &response["data"]["manifest"]["business_identity"];
    assert_eq!(binding["directory_inputs"], json!(["development"]));
    assert!(binding["source_sha256"].get("development").is_none());
    assert!(binding["source_sha256"]["development/project.txt"].is_string());
    let (code, response) = invoke(
        &root,
        &skills,
        "file-apply",
        "TASK-001",
        Some(json!({
            "schema":"work-file-transaction-request","command":{"kind":"apply","attempt_id":"ATTEMPT-001","changes":changes}
        })),
    );
    assert_eq!(code, 0, "{response}");
    assert_eq!(response["data"]["phase"], "published-verified");
}
