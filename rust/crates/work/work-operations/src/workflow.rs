//! Pure workflow state decisions before execution artifacts are available.

use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowDecision {
    pub status: String,
    pub target_artifact: &'static str,
    pub requires_user_confirmation: bool,
    pub required_checks: Vec<String>,
    pub details: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowIssue {
    pub reason_code: &'static str,
    pub message: &'static str,
}

pub fn decide_pre_execution(
    source_validation: Option<&Value>,
    discussion: Option<&Value>,
    task_validation: Option<&Value>,
    execution_index_exists: bool,
) -> Result<Option<WorkflowDecision>, WorkflowIssue> {
    let decision = match (source_validation, task_validation, execution_index_exists) {
        (None, _, _) => WorkflowDecision {
            status: "source_required".into(),
            target_artifact: "source",
            requires_user_confirmation: true,
            required_checks: vec![],
            details: json!({}),
        },
        (Some(source_validation), None, _) => {
            let (status, details) = match discussion {
                None => (
                    "task_discussion_required",
                    json!({"source":source_validation}),
                ),
                Some(discussion) => {
                    if discussion["view"].is_null() || !discussion["planning_ready"].is_boolean() {
                        return Err(WorkflowIssue {
                            reason_code: "workflow_discussion_invalid",
                            message: "A verified committed Session view is required.",
                        });
                    }
                    (
                        "task_discussion",
                        json!({"source":source_validation,"discussion":discussion}),
                    )
                }
            };
            WorkflowDecision {
                status: status.into(),
                target_artifact: "task",
                requires_user_confirmation: true,
                required_checks: vec!["source validate".into()],
                details,
            }
        }
        (Some(_), Some(task_validation), false) => WorkflowDecision {
            status: "execution_recovery_required".into(),
            target_artifact: "execution",
            requires_user_confirmation: true,
            required_checks: vec!["task validate".into()],
            details: json!({"task_collection_sha256":task_validation["task_collection_sha256"]}),
        },
        _ => return Ok(None),
    };
    Ok(Some(decision))
}

pub fn decide_execution(index: &Value, attempts: &Value) -> WorkflowDecision {
    let target = "execution";
    let checks = vec!["task validate".into(), "execute preflight".into()];
    if !index["lock"].is_null() {
        let lock = &index["lock"];
        let task_id = lock["task_id"].as_str().unwrap_or("");
        let row = index["tasks"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|row| row["id"] == task_id);
        let attempt = &attempts[task_id];
        let active = lock["kind"] == "execution"
            && row.is_some_and(|row| {
                row["status"] == "in_progress" && row["latest_attempt"] == lock["attempt_id"]
            })
            && attempt["status"] == "in_progress"
            && attempt["task_id"] == lock["task_id"]
            && attempt["attempt_id"] == lock["attempt_id"]
            && attempt["execute_instructions_sha256"] == lock["execute_instructions_sha256"];
        if !active {
            return WorkflowDecision {
                status: "execution_locked".into(),
                target_artifact: target,
                requires_user_confirmation: true,
                required_checks: vec!["execute preflight".into()],
                details: json!({"lock":lock}),
            };
        }
    }
    let overall = index["overall_status"].as_str().unwrap_or("");
    let mut unresolved = Vec::new();
    if let Some(attempts) = attempts.as_object() {
        for attempt in attempts.values() {
            if attempt["status"] == "in_progress" {
                continue;
            }
            let resolved = attempt["_reconciliation_resolved_ids"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            for deviation in attempt["execution_deviations"]
                .as_array()
                .into_iter()
                .flatten()
            {
                if deviation["decision"]["outcome"] == "approved"
                    && deviation["reconciliation_status"] == "pending"
                    && !resolved.contains(&deviation["deviation_id"])
                {
                    unresolved.push(deviation["deviation_id"].clone());
                }
            }
        }
    }
    if matches!(overall, "completed" | "cancelled") && !unresolved.is_empty() {
        unresolved.sort_by(|left, right| left.as_str().cmp(&right.as_str()));
        return WorkflowDecision {
            status: "execution_reconciliation_pending".into(),
            target_artifact: target,
            requires_user_confirmation: true,
            required_checks: checks,
            details: json!({"overall_status":overall,"pending_deviation_ids":unresolved}),
        };
    }
    let confirmation = overall != "in_progress";
    WorkflowDecision {
        status: format!("execution_{overall}"),
        target_artifact: target,
        requires_user_confirmation: confirmation,
        required_checks: checks,
        details: json!({"overall_status":overall}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formal_states_follow_artifact_and_lock_evidence() {
        let missing = decide_pre_execution(None, None, None, false)
            .unwrap()
            .unwrap();
        assert_eq!(
            (missing.status.as_str(), missing.target_artifact),
            ("source_required", "source")
        );
        let source = json!({"sources":["SRC-001"]});
        let discussion = json!({"view":{"revision":1},"planning_ready":false});
        let pending = decide_pre_execution(Some(&source), Some(&discussion), None, false)
            .unwrap()
            .unwrap();
        assert_eq!(
            (pending.status.as_str(), pending.target_artifact),
            ("task_discussion", "task")
        );
        let task = json!({"task_collection_sha256":"2".repeat(64)});
        let recovery = decide_pre_execution(Some(&source), None, Some(&task), false)
            .unwrap()
            .unwrap();
        assert_eq!(recovery.status, "execution_recovery_required");
        assert!(
            decide_pre_execution(Some(&source), None, Some(&task), true)
                .unwrap()
                .is_none()
        );

        let index = json!({"overall_status":"in_progress","lock":{"kind":"execution","task_id":"TASK-001","attempt_id":"ATTEMPT-001","execute_instructions_sha256":"c".repeat(64)},"tasks":[{"id":"TASK-001","status":"in_progress","latest_attempt":"ATTEMPT-001"}]});
        let attempts = json!({"TASK-001":{"status":"in_progress","task_id":"TASK-001","attempt_id":"ATTEMPT-001","execute_instructions_sha256":"c".repeat(64)}});
        let active = decide_execution(&index, &attempts);
        assert_eq!(active.status, "execution_in_progress");
        assert!(!active.requires_user_confirmation);
        let mut drifted = attempts;
        drifted["TASK-001"]["execute_instructions_sha256"] = json!("d".repeat(64));
        assert_eq!(
            decide_execution(&index, &drifted).status,
            "execution_locked"
        );
    }
}
