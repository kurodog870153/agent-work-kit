//! Current Host file evidence and retired writer boundaries.
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

fn setup() -> (PathBuf, PathBuf) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "work-file-current-{}-{}",
        std::process::id(),
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
    work_infrastructure::fixture_support::file_transaction_fixture(&root, &skills).unwrap();
    (root, skills)
}

fn invoke(
    root: &Path,
    skills: &Path,
    command: &str,
    task: &str,
    request: Option<Value>,
) -> (i32, Value) {
    let mut args = vec![
        "--project-root".into(),
        root.to_string_lossy().into_owned(),
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
    if let Some(request) = request {
        let input = root.join("outputs/work/transactions/example/file-test/request.json");
        fs::write(&input, serde_json::to_vec(&request).unwrap()).unwrap();
        args.extend(["--input-file".into(), input.to_string_lossy().into_owned()]);
    }
    let (code, result) = work_cli::runtime::run_with_skill_root(&args, skills);
    (code, serde_json::to_value(result).unwrap())
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
