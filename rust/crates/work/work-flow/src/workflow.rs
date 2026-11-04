//! Formal workflow status from verified artifact snapshots.

use serde_json::Value;
use work_feature::error::WorkError;
use work_feature::workflow::{WorkflowSnapshot, execution_state, pre_execution_state};

pub fn status(requirement_id: &str, snapshot: WorkflowSnapshot) -> Result<Value, WorkError> {
    let WorkflowSnapshot {
        artifacts,
        source,
        discussion,
        task,
        index,
        latest_attempts,
    } = snapshot;
    if let Some(state) = pre_execution_state(
        requirement_id,
        &artifacts,
        source.as_ref(),
        discussion.as_ref(),
        task.as_ref(),
        index.is_some(),
    )? {
        return Ok(state);
    }
    execution_state(
        requirement_id,
        &artifacts,
        index
            .as_ref()
            .expect("pre-execution state handled missing index"),
        &latest_attempts,
    )
}
