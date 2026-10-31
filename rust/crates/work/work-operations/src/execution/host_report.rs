//! Pure binding between a reserved CMD and a Host-reported result.

use serde_json::{Value, json};
use work_model::execution::host_report::{HostExecutionReport, HostResultStatus};

use crate::derivation::{fingerprint, identity::next_record_id};
use crate::execution::authorization::{effective_task_for_record, require_record_scope};
use crate::execution::{ExecutionIssue, command::correction::resolve_corrected_command};

fn issue(reason_code: &'static str, message: &'static str) -> ExecutionIssue {
    ExecutionIssue {
        reason_code,
        message,
        details: json!({}),
    }
}

fn command_details(
    task: &Value,
    attempt: &Value,
    collection: &Value,
    record_id: &str,
    correction: Option<&Value>,
    input_files: &Value,
) -> Result<(Value, Value, Value, String), ExecutionIssue> {
    let base = record_id.split('#').next().unwrap_or(record_id);
    require_record_scope(attempt, base)?;
    let effective = effective_task_for_record(task, attempt, Some(record_id))?;
    let formal = effective["commands"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|command| command["id"] == base)
        .ok_or_else(|| {
            issue(
                "host_command_missing",
                "The reserved CMD is not in the TASK.",
            )
        })?;
    let mut command = json!({"mode":formal["mode"]});
    match formal["mode"].as_str() {
        Some("argv") => command["argv"] = formal["argv"].clone(),
        Some("shell") => command["script"] = formal["script"].clone(),
        _ => {
            return Err(issue(
                "host_command_mode_invalid",
                "The CMD mode is invalid.",
            ));
        }
    }
    if let Some(correction) = correction {
        command = resolve_corrected_command(task, attempt, base, &command, correction)?;
    }
    let settings = formal
        .get("execution")
        .unwrap_or(&collection["execution_defaults"])
        .clone();
    if settings.is_null() {
        return Err(issue(
            "host_command_binding_missing",
            "Execution settings are required.",
        ));
    }
    let capture = serde_json::to_value(crate::execution::command::capture::policy(
        &effective, base,
    )?)
    .map_err(|_| {
        issue(
            "host_command_binding_invalid",
            "The CMD capture policy is invalid.",
        )
    })?;
    let command_sha256 =
        fingerprint::structured(&json!({"command":command,"execution":settings,"capture":capture,"input_files":input_files}))
            .map_err(|_| {
                issue(
                    "host_command_binding_invalid",
                    "The CMD binding is invalid.",
                )
            })?;
    Ok((command, settings, capture, command_sha256))
}

pub fn binding(
    task: &Value,
    attempt: &Value,
    index: &Value,
    collection: &Value,
    file_version: &str,
    input_files: &Value,
) -> Result<Value, ExecutionIssue> {
    let lock = &index["lock"];
    let task_id = attempt["task_id"].as_str().unwrap_or("");
    let attempt_id = attempt["attempt_id"].as_str().unwrap_or("");
    let record_id = lock["record_id"].as_str().ok_or_else(|| {
        issue(
            "host_command_not_reserved",
            "The active lock must reserve a CMD.",
        )
    })?;
    let base = record_id.split('#').next().unwrap_or(record_id);
    if lock["kind"] != "execution"
        || lock["task_id"] != task_id
        || lock["attempt_id"] != attempt_id
        || lock["execute_instructions_sha256"] != attempt["execute_instructions_sha256"]
        || attempt["status"] != "in_progress"
        || task["id"] != task_id
        || !base.starts_with("CMD-")
        || next_record_id(base, attempt)? != record_id
    {
        return Err(issue(
            "host_command_not_reserved",
            "The active Attempt and lock must reserve the next CMD instance.",
        ));
    }
    if file_version.trim().is_empty() {
        return Err(issue(
            "host_command_binding_missing",
            "Current file version is required.",
        ));
    }
    let (command, settings, capture, command_sha256) = command_details(
        task,
        attempt,
        collection,
        record_id,
        lock.get("command_correction"),
        input_files,
    )?;
    Ok(json!({
        "schema":"work-host-command-binding",
        "task_id":task_id,
        "attempt_id":attempt_id,
        "record_id":record_id,
        "task_collection_sha256":attempt["task_collection_sha256"],
        "authorization_sha256":attempt["authorization_sha256"],
        "file_version":file_version,
        "command_sha256":command_sha256,
        "command":command,
        "execution":settings,
        "capture":capture,
        "input_files":input_files,
    }))
}

pub fn validate_report(
    report: &HostExecutionReport,
    expected: &Value,
) -> Result<i64, ExecutionIssue> {
    if !report.valid_shape() {
        return Err(issue(
            "host_result_invalid",
            "The Host result shape is invalid.",
        ));
    }
    if report.status != HostResultStatus::Exited {
        return Err(issue(
            "host_result_unknown",
            "A rejected, unknown or partial Host result cannot finish a CMD.",
        ));
    }
    for (field, actual) in [
        ("task_id", report.task_id.as_str()),
        ("attempt_id", report.attempt_id.as_str()),
        ("record_id", report.record_id.as_str()),
        (
            "task_collection_sha256",
            report.task_collection_sha256.as_str(),
        ),
        ("authorization_sha256", report.authorization_sha256.as_str()),
        ("file_version", report.file_version.as_str()),
        ("command_sha256", report.command_sha256.as_str()),
    ] {
        if expected[field] != actual {
            return Err(issue(
                "host_result_binding_mismatch",
                "The Host result does not match the current formal CMD binding.",
            ));
        }
    }
    report
        .exit_code
        .ok_or_else(|| issue("host_result_unknown", "A known CMD exit code is required."))
}

pub fn validate_frozen_binding(
    expected: &Value,
    report: &HostExecutionReport,
) -> Result<i64, ExecutionIssue> {
    let object = expected.as_object().ok_or_else(|| {
        issue(
            "host_result_binding_invalid",
            "A frozen CMD binding is required.",
        )
    })?;
    let keys = [
        "schema",
        "task_id",
        "attempt_id",
        "record_id",
        "task_collection_sha256",
        "authorization_sha256",
        "file_version",
        "command_sha256",
        "command",
        "execution",
        "capture",
        "input_files",
    ];
    if object.len() != keys.len()
        || keys.iter().any(|key| !object.contains_key(*key))
        || expected["schema"] != "work-host-command-binding"
        || expected["command"].is_null()
        || expected["execution"].is_null()
        || expected["capture"].is_null()
    {
        return Err(issue(
            "host_result_binding_invalid",
            "The frozen CMD binding is malformed.",
        ));
    }
    let current = fingerprint::structured(&json!({
        "command":expected["command"],
        "execution":expected["execution"],
        "capture":expected["capture"],
        "input_files":expected["input_files"],
    }))
    .map_err(|_| {
        issue(
            "host_result_binding_invalid",
            "The frozen CMD binding is invalid.",
        )
    })?;
    if expected["command_sha256"] != current {
        return Err(issue(
            "host_result_binding_invalid",
            "The frozen CMD fingerprint is invalid.",
        ));
    }
    validate_report(report, expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn current() -> (Value, Value, Value, Value) {
        let task = json!({"id":"TASK-001","commands":[{"id":"CMD-001","mode":"argv","argv":["tool","check"]}]});
        let attempt = json!({
            "task_id":"TASK-001","attempt_id":"ATTEMPT-001","status":"in_progress",
            "task_collection_sha256":"a".repeat(64),"authorization_sha256":"b".repeat(64),
            "execute_instructions_sha256":"c".repeat(64),"records":[],
            "authorization":{"commands":[{"id":"CMD-001"}]}
        });
        let index = json!({"lock":{"kind":"execution","task_id":"TASK-001",
            "attempt_id":"ATTEMPT-001","record_id":"CMD-001",
            "execute_instructions_sha256":"c".repeat(64)}});
        let collection = json!({"execution_defaults":{"working_directory":".","os":"macos"}});
        (task, attempt, index, collection)
    }

    fn report(expected: &Value) -> HostExecutionReport {
        serde_json::from_value(json!({
            "schema":"work-host-execution-report",
            "task_id":expected["task_id"],"attempt_id":expected["attempt_id"],
            "record_id":expected["record_id"],
            "task_collection_sha256":expected["task_collection_sha256"],
            "authorization_sha256":expected["authorization_sha256"],
            "file_version":expected["file_version"],
            "command_sha256":expected["command_sha256"],
            "result_origin":"host_reported","result_reference":"host-evidence/result.json",
            "result_sha256":"d".repeat(64),"status":"exited","exit_code":0,
            "output_complete":true
        }))
        .unwrap()
    }

    #[test]
    fn current_binding_rejects_scope_and_host_result_drift() {
        let (task, attempt, index, collection) = current();
        let expected = binding(
            &task,
            &attempt,
            &index,
            &collection,
            "version-1",
            &Value::Null,
        )
        .unwrap();
        assert_eq!(expected["command"]["argv"], json!(["tool", "check"]));
        assert_eq!(validate_report(&report(&expected), &expected).unwrap(), 0);
        let mut drifted = report(&expected);
        drifted.command_sha256 = "e".repeat(64);
        assert_eq!(
            validate_report(&drifted, &expected)
                .unwrap_err()
                .reason_code,
            "host_result_binding_mismatch"
        );
        drifted = report(&expected);
        drifted.status = HostResultStatus::Partial;
        drifted.exit_code = None;
        assert_eq!(
            validate_report(&drifted, &expected)
                .unwrap_err()
                .reason_code,
            "host_result_unknown"
        );
        let mut unauthorized = attempt.clone();
        unauthorized["authorization"]["commands"] = json!([]);
        assert_eq!(
            binding(
                &task,
                &unauthorized,
                &index,
                &collection,
                "version-1",
                &Value::Null
            )
            .unwrap_err()
            .reason_code,
            "execution_authorization_scope_expansion"
        );
    }
}
