//! Pre-execution workflow state assembly.

use serde_json::{Value, json};
use work_operations::workflow::{WorkflowDecision, decide_execution, decide_pre_execution};

use crate::error::{ExitCode, WorkError};

pub struct WorkflowSnapshot {
    pub artifacts: Value,
    pub source: Option<Value>,
    pub discussion: Option<Value>,
    pub task: Option<Value>,
    pub index: Option<Value>,
    pub latest_attempts: Value,
}

pub fn pre_execution_state(
    requirement_id: &str,
    artifacts: &Value,
    source_validation: Option<&Value>,
    discussion: Option<&Value>,
    task_validation: Option<&Value>,
    execution_index_exists: bool,
) -> Result<Option<Value>, WorkError> {
    let decision = decide_pre_execution(
        source_validation,
        discussion,
        task_validation,
        execution_index_exists,
    )
    .map_err(|issue| {
        WorkError::new(
            ExitCode::Contract,
            issue.reason_code,
            issue.message,
            json!({}),
        )
    })?;
    decision
        .map(|decision| render_state(requirement_id, artifacts, decision))
        .transpose()
}

pub fn execution_state(
    requirement_id: &str,
    artifacts: &Value,
    index: &Value,
    latest_attempts: &Value,
) -> Result<Value, WorkError> {
    render_state(
        requirement_id,
        artifacts,
        decide_execution(index, latest_attempts),
    )
}

fn render_state(
    requirement_id: &str,
    artifacts: &Value,
    decision: WorkflowDecision,
) -> Result<Value, WorkError> {
    let target = artifacts[decision.target_artifact]
        .as_str()
        .ok_or_else(|| {
            WorkError::new(
                ExitCode::Contract,
                "workflow_artifact_path_missing",
                "The selected workflow artifact path is missing.",
                json!({"artifact":decision.target_artifact}),
            )
        })?;
    Ok(json!({
        "schema":"work-workflow-state","requirement_id":requirement_id,
        "status":decision.status,"target":target,
        "requires_user_confirmation":decision.requires_user_confirmation,
        "required_checks":decision.required_checks,"artifacts":artifacts,"details":decision.details,
    }))
}
