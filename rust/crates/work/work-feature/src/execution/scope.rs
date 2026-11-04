//! Formal execution identity, authorization, and file scope checks.

use crate::error::{ExitCode, WorkError};
use serde_json::{Value, json};

/// Formal TASK, Execution index and Attempt identities and the authorized scope.
/// Host tool selection and Tooling policy are outside this Work validation.
pub struct ExecutionScopeDocuments<'a> {
    pub task_validation: &'a Value,
    pub index: &'a Value,
    pub attempt: &'a Value,
}

pub fn validate_execution_scope(
    documents: ExecutionScopeDocuments<'_>,
    host_authorized: bool,
    requested_files: &[&str],
) -> Result<(), WorkError> {
    let ExecutionScopeDocuments {
        task_validation,
        index,
        attempt,
    } = documents;
    if !host_authorized {
        return Err(WorkError::new(
            ExitCode::WorkflowState,
            "execute_scope_not_authorized",
            "Execution requires Host authorization.",
            json!({}),
        ));
    }
    let task_id = attempt["task_id"].as_str().unwrap_or("");
    let attempt_id = attempt["attempt_id"].as_str().unwrap_or("");
    let row = index["tasks"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["id"] == task_id));
    if task_id.is_empty()
        || attempt_id.is_empty()
        || !task_validation["task_ids"]
            .as_array()
            .is_some_and(|ids| ids.iter().any(|id| id == task_id))
        || attempt["status"] != "in_progress"
        || row
            .is_none_or(|row| row["status"] != "in_progress" || row["latest_attempt"] != attempt_id)
        || index["lock"]["task_id"] != task_id
        || index["lock"]["attempt_id"] != attempt_id
        || task_validation["task_item_sha256"][task_id] != attempt["task_item_sha256"]
        || task_validation["task_collection_sha256"] != attempt["task_collection_sha256"]
        || task_validation["task_index_sha256"] != attempt["task_index_sha256"]
    {
        return Err(WorkError::new(
            ExitCode::WorkflowState,
            "execution_attempt_invalid",
            "Execution scope is not bound to a current validated TASK and Attempt.",
            json!({}),
        ));
    }
    let allowed = attempt["authorization"]["modifiable_files"]
        .as_array()
        .ok_or_else(|| {
            WorkError::new(
                ExitCode::WorkflowState,
                "execution_scope_invalid",
                "Attempt authorization has no valid modifiable-file scope.",
                json!({}),
            )
        })?;
    if requested_files.is_empty()
        || requested_files
            .iter()
            .any(|path| !allowed.iter().any(|value| value == path))
    {
        return Err(WorkError::new(
            ExitCode::WorkflowState,
            "execution_scope_exceeded",
            "Requested files exceed the Attempt's authorized files.",
            json!({}),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_scope_requires_current_attempt_and_authorized_files() {
        let validation = json!({
            "task_ids":["TASK-001"],
            "task_item_sha256":{"TASK-001":"c".repeat(64)},
            "task_collection_sha256":"a".repeat(64),
            "task_index_sha256":"b".repeat(64)
        });
        let index = json!({
            "tasks":[{"id":"TASK-001","status":"in_progress","latest_attempt":"ATTEMPT-001"}],
            "lock":{"task_id":"TASK-001","attempt_id":"ATTEMPT-001"}
        });
        let attempt = json!({
            "task_id":"TASK-001","attempt_id":"ATTEMPT-001","status":"in_progress",
            "task_item_sha256":"c".repeat(64),
            "task_collection_sha256":"a".repeat(64),
            "task_index_sha256":"b".repeat(64),
            "authorization":{"modifiable_files":["src/main.rs"]}
        });
        let check = |attempt: &Value, validation: &Value, authorized, files: &[&str]| {
            validate_execution_scope(
                ExecutionScopeDocuments {
                    task_validation: validation,
                    index: &index,
                    attempt,
                },
                authorized,
                files,
            )
        };
        assert!(check(&attempt, &validation, true, &["src/main.rs"]).is_ok());
        assert_eq!(
            check(&attempt, &validation, false, &["src/main.rs"])
                .unwrap_err()
                .reason_code,
            "execute_scope_not_authorized"
        );
        assert_eq!(
            check(&attempt, &validation, true, &["src/other.rs"])
                .unwrap_err()
                .reason_code,
            "execution_scope_exceeded"
        );
        let mut stopped = attempt.clone();
        stopped["status"] = json!("stopped");
        assert_eq!(
            check(&stopped, &validation, true, &["src/main.rs"])
                .unwrap_err()
                .reason_code,
            "execution_attempt_invalid"
        );
        let mut drifted = validation.clone();
        drifted["task_item_sha256"]["TASK-001"] = json!("d".repeat(64));
        assert_eq!(
            check(&attempt, &drifted, true, &["src/main.rs"])
                .unwrap_err()
                .reason_code,
            "execution_attempt_invalid"
        );
    }
}
