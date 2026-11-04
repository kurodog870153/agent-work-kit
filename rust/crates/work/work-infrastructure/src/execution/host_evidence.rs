//! Capture and verify actual command evidence independently of host policy.
use super::evidence_file::read;
#[cfg(test)]
use crate::files::LocalFiles;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use work_feature::error::{ExitCode, WorkError};
#[cfg(test)]
use work_feature::ports::{ArtifactStore, CommandOutputCapture};
use work_model::execution::command::capture::{
    CapturedArtifact, CapturedReport, CommandCapturePolicy, CommandCapturedEvidence,
    MAX_STREAM_BYTES,
};

fn invalid(stage: &str) -> WorkError {
    WorkError::new(
        ExitCode::ArtifactIntegrity,
        "command_capture_unverified",
        "Command evidence is missing, changed or incomplete; preserve the original evidence and do not rerun automatically.",
        json!({"stage":stage}),
    )
}
#[cfg(test)]
fn save(
    root: &Path,
    directory: &str,
    name: &str,
    bytes: &[u8],
    complete: bool,
) -> Result<CapturedArtifact, WorkError> {
    let path = crate::files::resolve_runtime_path(root, &format!("{directory}/{name}"))?;
    LocalFiles.create_new(&path, bytes)?;
    if read(&path, bytes.len() as u64)? != bytes {
        return Err(invalid("saved_capture"));
    }
    Ok(CapturedArtifact {
        file: name.into(),
        size_bytes: bytes.len() as u64,
        sha256: work_operations::derivation::fingerprint::raw(bytes),
        complete,
    })
}

#[cfg(test)]
pub(super) fn freeze(
    root: &Path,
    directory: &str,
    report_root: &Path,
    policy: &CommandCapturePolicy,
    capture: &CommandOutputCapture,
) -> Result<CommandCapturedEvidence, WorkError> {
    if capture.stdout.len() > MAX_STREAM_BYTES || capture.stderr.len() > MAX_STREAM_BYTES {
        return Err(invalid("stream_limit"));
    }
    let stdout = save(
        root,
        directory,
        "stdout.bin",
        &capture.stdout,
        capture.stdout_complete,
    )?;
    let stderr = save(
        root,
        directory,
        "stderr.bin",
        &capture.stderr,
        capture.stderr_complete,
    )?;
    let mut reports = BTreeMap::new();
    let mut remaining = policy
        .report_total_limit_bytes
        .min(work_model::execution::command::capture::MAX_REPORT_TOTAL_BYTES);
    for (position, report) in policy.report_paths.iter().enumerate() {
        let bytes = crate::files::resolve_runtime_path(report_root, report).and_then(|path| {
            read(
                &path,
                remaining
                    .min(policy.report_limit_bytes)
                    .min(work_model::execution::command::capture::MAX_REPORT_BYTES),
            )
        });
        let report_capture = match bytes {
            Ok(bytes) => {
                remaining = remaining
                    .checked_sub(bytes.len() as u64)
                    .ok_or_else(|| invalid("report_budget"))?;
                CapturedReport::Available {
                    artifact: save(
                        root,
                        directory,
                        &format!("report-{position:04}.bin"),
                        &bytes,
                        true,
                    )?,
                }
            }
            Err(error) => CapturedReport::Unavailable {
                reason_code: error.reason_code,
            },
        };
        reports.insert(report.clone(), report_capture);
    }
    Ok(CommandCapturedEvidence {
        stdout,
        stderr,
        reports,
    })
}

pub(super) fn read_artifact(
    root: &Path,
    directory: &str,
    artifact: &CapturedArtifact,
    expected_name: &str,
    limit: u64,
) -> Result<Option<Vec<u8>>, WorkError> {
    if artifact.file != expected_name || artifact.size_bytes > limit {
        return Err(invalid("descriptor"));
    }
    let path = crate::files::resolve_runtime_path(root, &format!("{directory}/{}", artifact.file))?;
    let bytes = read(&path, limit)?;
    if bytes.len() as u64 != artifact.size_bytes
        || work_operations::derivation::fingerprint::raw(&bytes) != artifact.sha256
    {
        return Err(invalid("capture_bytes"));
    }
    Ok(artifact.complete.then_some(bytes))
}

pub(super) struct ReceiptIdentity<'a> {
    pub execution_dir: &'a str,
    pub task_id: &'a str,
    pub attempt_id: &'a str,
    pub record_id: &'a str,
}
pub(super) fn verify_receipts(
    expected: &ReceiptIdentity<'_>,
    started: Option<&Value>,
    finished: Option<&Value>,
) -> Result<i64, WorkError> {
    let started = started.ok_or_else(|| invalid("started_missing"))?;
    let finished = finished.ok_or_else(|| invalid("finished_missing"))?;
    let preview = &started["preview"];
    let approved = preview["approved_sha256"]
        .as_str()
        .ok_or_else(|| invalid("preview"))?;
    let paths = work_feature::execution::command_publication::approved_receipt_paths(
        preview,
        expected.execution_dir,
        approved,
    )?;
    if started["schema"] != "work-command-started"
        || started["authorization_evidence"]
            .as_str()
            .is_none_or(|s| s.trim().is_empty())
        || preview["task_id"] != expected.task_id
        || preview["attempt_id"] != expected.attempt_id
        || preview["record_id"] != expected.record_id
        || finished["schema"] != "work-command-result"
        || finished["record_id"] != expected.record_id
        || finished["approved_sha256"] != approved
        || finished
            .get("receipt_dir")
            .is_some_and(|path| *path != paths.directory)
        || finished["status"] != "exited"
        || finished.get("signal").is_some()
        || finished.get("cleanup_error").is_some()
    {
        return Err(invalid("receipt_identity_or_result"));
    }
    finished["exit_code"]
        .as_i64()
        .ok_or_else(|| invalid("unknown_exit"))
}

pub(super) struct VerifiedCaptures {
    pub stdout: Option<Vec<u8>>,
    pub stderr: Option<Vec<u8>>,
    pub reports: BTreeMap<String, Option<Vec<u8>>>,
}
pub(super) fn verify_captures(
    root: &Path,
    directory: &str,
    captures: &CommandCapturedEvidence,
    policy: &CommandCapturePolicy,
) -> Result<VerifiedCaptures, WorkError> {
    if captures.reports.keys().collect::<BTreeSet<_>>() != policy.report_paths.iter().collect() {
        return Err(invalid("capture_sources"));
    }
    let stdout = read_artifact(
        root,
        directory,
        &captures.stdout,
        "stdout.bin",
        MAX_STREAM_BYTES as u64,
    )?;
    let stderr = read_artifact(
        root,
        directory,
        &captures.stderr,
        "stderr.bin",
        MAX_STREAM_BYTES as u64,
    )?;
    let mut remaining = policy
        .report_total_limit_bytes
        .min(work_model::execution::command::capture::MAX_REPORT_TOTAL_BYTES);
    let mut reports = BTreeMap::new();
    for (position, (path, report)) in captures.reports.iter().enumerate() {
        let bytes = match report {
            CapturedReport::Available { artifact } => {
                let bytes = read_artifact(
                    root,
                    directory,
                    artifact,
                    &format!("report-{position:04}.bin"),
                    remaining
                        .min(policy.report_limit_bytes)
                        .min(work_model::execution::command::capture::MAX_REPORT_BYTES),
                )?;
                remaining = remaining
                    .checked_sub(artifact.size_bytes)
                    .ok_or_else(|| invalid("report_budget"))?;
                bytes
            }
            CapturedReport::Unavailable { .. } => None,
        };
        reports.insert(path.clone(), bytes);
    }
    Ok(VerifiedCaptures {
        stdout,
        stderr,
        reports,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::host::{self, Status};
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    fn root() -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "work host evidence {} {}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("receipts/CMD-001")).unwrap();
        std::fs::create_dir_all(root.join("tool-output")).unwrap();
        root.canonicalize().unwrap()
    }
    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn host_output_and_normal_reports_are_frozen_without_relocation_or_cleanup() {
        let root = root();
        #[cfg(windows)]
        let argv = vec![
            "cmd.exe".into(),
            "/d".into(),
            "/c".into(),
            "echo evidence".into(),
        ];
        #[cfg(target_os = "macos")]
        let argv = vec!["/usr/bin/printf".into(), "evidence".into()];
        let capture = host::run(
            &work_feature::ports::CommandRequest {
                argv,
                cwd: root.clone(),
                timeout: Duration::from_secs(3),
            },
            &AtomicBool::new(false),
        );
        assert_eq!(capture.status, Status::Exited);
        assert_eq!(capture.exit_code, Some(0));
        let report = root.join("tool-output/report.json");
        std::fs::write(&report, b"{\"passed\":true}").unwrap();
        let mut policy = CommandCapturePolicy::default();
        policy.report_paths.extend([
            "tool-output/report.json".into(),
            "tool-output/missing.json".into(),
        ]);
        let output = host::into_outcome(capture, None).complete_output.unwrap();
        let captures = freeze(&root, "receipts/CMD-001", &root, &policy, &output).unwrap();
        assert!(report.is_file());
        assert_eq!(std::fs::read(&report).unwrap(), b"{\"passed\":true}");
        let VerifiedCaptures {
            stdout,
            stderr,
            reports,
        } = verify_captures(&root, "receipts/CMD-001", &captures, &policy).unwrap();
        assert!(
            String::from_utf8(stdout.unwrap())
                .unwrap()
                .contains("evidence")
        );
        assert!(stderr.unwrap().is_empty());
        assert_eq!(
            reports["tool-output/report.json"],
            Some(b"{\"passed\":true}".to_vec())
        );
        assert_eq!(reports["tool-output/missing.json"], None);
        std::fs::write(&report, b"new tool output").unwrap();
        let VerifiedCaptures { reports, .. } =
            verify_captures(&root, "receipts/CMD-001", &captures, &policy).unwrap();
        assert_eq!(
            reports["tool-output/report.json"],
            Some(b"{\"passed\":true}".to_vec())
        );
        std::fs::write(root.join("receipts/CMD-001/stdout.bin"), b"drift").unwrap();
        assert!(verify_captures(&root, "receipts/CMD-001", &captures, &policy).is_err());
        assert!(freeze(&root, "receipts/CMD-001", &root, &policy, &output).is_err());
        assert!(!root.join("receipts/CMD-001/finished.json").exists());
    }
    #[test]
    fn missing_truncated_and_invalid_descriptor_never_become_complete_evidence() {
        let root = root();
        let path = root.join("bounded");
        std::fs::write(&path, b"1234").unwrap();
        assert!(read(&path, 3).is_err());
        assert!(read(&root.join("missing"), 10).is_err());
        assert!(read(&root, 10).is_err());
        let artifact = save(&root, "receipts/CMD-001", "stdout.bin", b"partial", false).unwrap();
        assert_eq!(
            read_artifact(&root, "receipts/CMD-001", &artifact, "stdout.bin", 100).unwrap(),
            None
        );
        assert!(read_artifact(&root, "receipts/CMD-001", &artifact, "stderr.bin", 100).is_err());
        std::fs::hard_link(&path, root.join("alias")).unwrap();
        assert!(read(&path, 10).is_err());
    }
    #[test]
    fn receipt_identity_requires_started_known_finished_and_exact_task_attempt_record() {
        use work_operations::execution::command::run::{
            CommandReceiptPreviewInput, build_command_preview_with_receipts,
        };
        let preview = build_command_preview_with_receipts(CommandReceiptPreviewInput {
            request: &json!({"schema":"work-command-run-request","timeout_seconds":10}),
            execution_dir:"executions",task_id:"TASK-001",attempt_id:"ATTEMPT-001",record_id:"CMD-001",
            working_directory:"/project",execution:&json!({"os":"macos","working_directory":"."}),
            invocation:&json!({"kind":"direct","executable":"/usr/bin/printf","executable_sha256":"a".repeat(64),"argv":["printf","proof"]}),
            sources:&json!({"task.json":"b".repeat(64)}),
        }).unwrap();
        let started = json!({"schema":"work-command-started","authorization_evidence":"Approved scope","preview":preview});
        let mut finished =
            work_operations::execution::command::run::build_command_result_with_receipts(
                &preview,
                &json!({"status":"exited","exit_code":7,"stdout_tail":"proof","stdout_truncated":false,
                "stderr_tail":"","stderr_truncated":false}),
                false,
            );
        let expected = ReceiptIdentity {
            execution_dir: "executions",
            task_id: "TASK-001",
            attempt_id: "ATTEMPT-001",
            record_id: "CMD-001",
        };
        assert_eq!(
            verify_receipts(&expected, Some(&started), Some(&finished)).unwrap(),
            7
        );
        assert!(verify_receipts(&expected, None, Some(&finished)).is_err());
        assert!(verify_receipts(&expected, Some(&started), None).is_err());
        for field in ["task_id", "attempt_id", "record_id"] {
            let mut foreign = started.clone();
            foreign["preview"][field] = json!("foreign");
            assert!(verify_receipts(&expected, Some(&foreign), Some(&finished)).is_err());
        }
        for status in ["timed_out", "interrupted", "unknown", "launch_failed"] {
            finished["status"] = json!(status);
            assert!(verify_receipts(&expected, Some(&started), Some(&finished)).is_err());
        }
        finished["status"] = json!("exited");
        finished["exit_code"] = Value::Null;
        assert!(verify_receipts(&expected, Some(&started), Some(&finished)).is_err());
    }
}
