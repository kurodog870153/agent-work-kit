//! Pure workflow state decisions before execution artifacts are available.

use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowDecision {
    pub status: String,
    pub next_action: String,
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
            next_action: "capture_source".into(),
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
                next_action: "review_discussion".into(),
                target_artifact: "task",
                requires_user_confirmation: true,
                required_checks: vec!["source validate".into()],
                details,
            }
        }
        (Some(_), Some(task_validation), false) => WorkflowDecision {
            status: "execution_recovery_required".into(),
            next_action: "inspect_recovery".into(),
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
                next_action: "inspect_recovery".into(),
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
            next_action: "review_reconciliation".into(),
            target_artifact: target,
            requires_user_confirmation: true,
            required_checks: checks,
            details: json!({"overall_status":overall,"pending_deviation_ids":unresolved}),
        };
    }
    let (action, confirmation) = match overall {
        "pending" => ("select_task_for_execution", true),
        "in_progress" => ("continue_execution", false),
        "pending_retry" => ("confirm_retry", true),
        "blocked" => ("review_reconciliation", true),
        "completed" | "cancelled" => ("review_completion", true),
        _ => ("review_state", true),
    };
    WorkflowDecision {
        status: format!("execution_{overall}"),
        next_action: action.into(),
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
    fn source_discussion_and_recovery_states_follow_dependency_order() {
        let missing = decide_pre_execution(None, None, None, false)
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                missing.status.as_str(),
                missing.next_action.as_str(),
                missing.target_artifact
            ),
            ("source_required", "capture_source", "source")
        );
        assert!(missing.requires_user_confirmation);
        let plan = json!({"sources":["SRC-001"]});
        let discussion = json!({"view":{"revision":1},"planning_ready":false});
        let pending = decide_pre_execution(Some(&plan), Some(&discussion), None, false)
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                pending.status.as_str(),
                pending.next_action.as_str(),
                pending.target_artifact
            ),
            ("task_discussion", "review_discussion", "task")
        );
        assert_eq!(pending.details["source"], plan);
        let task = json!({"task_collection_sha256":"2".repeat(64)});
        let recovery = decide_pre_execution(Some(&plan), None, Some(&task), false)
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                recovery.status.as_str(),
                recovery.next_action.as_str(),
                recovery.target_artifact
            ),
            (
                "execution_recovery_required",
                "inspect_recovery",
                "execution"
            )
        );
        assert!(
            decide_pre_execution(Some(&plan), None, Some(&task), true)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn active_lock_and_reconciliation_routes_match_current_contract() {
        let fingerprint = "c".repeat(64);
        let index = json!({
            "overall_status":"in_progress",
            "lock":{"kind":"execution","task_id":"TASK-001","attempt_id":"ATTEMPT-001","execute_instructions_sha256":fingerprint},
            "tasks":[{"id":"TASK-001","status":"in_progress","latest_attempt":"ATTEMPT-001"}]
        });
        let attempts = json!({"TASK-001":{
            "status":"in_progress","task_id":"TASK-001","attempt_id":"ATTEMPT-001",
            "execute_instructions_sha256":fingerprint
        }});
        let active = decide_execution(&index, &attempts);
        assert_eq!(
            (active.status.as_str(), active.next_action.as_str()),
            ("execution_in_progress", "continue_execution")
        );
        assert!(!active.requires_user_confirmation);

        let mut drifted = attempts;
        drifted["TASK-001"]["execute_instructions_sha256"] = json!("d".repeat(64));
        let locked = decide_execution(&index, &drifted);
        assert_eq!(locked.next_action, "inspect_recovery");

        let completed = json!({"overall_status":"completed","lock":null,"tasks":[{
            "id":"TASK-001","status":"completed","latest_attempt":"ATTEMPT-001"
        }]});
        let mut attempts = json!({"TASK-001":{
            "status":"completed","execution_deviations":[{
                "deviation_id":"DEVIATION-001","decision":{"outcome":"approved"},
                "reconciliation_status":"pending"
            }]
        }});
        let pending = decide_execution(&completed, &attempts);
        assert_eq!(
            (pending.status.as_str(), pending.next_action.as_str()),
            ("execution_reconciliation_pending", "review_reconciliation")
        );
        assert_eq!(
            pending.details["pending_deviation_ids"],
            json!(["DEVIATION-001"])
        );
        attempts["TASK-001"]["_reconciliation_resolved_ids"] = json!(["DEVIATION-001"]);
        let resolved = decide_execution(&completed, &attempts);
        assert_eq!(
            (resolved.status.as_str(), resolved.next_action.as_str()),
            ("execution_completed", "review_completion")
        );
    }
}
