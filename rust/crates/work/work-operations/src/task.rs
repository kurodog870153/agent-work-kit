//! Rules for TASK, Execution/Attempt and the two Acceptance layers.
//!
//! Acceptance conclusions are reported by the AI/Host or a named user; these
//! rules only check shape, references and the completion condition, never
//! whether a conclusion is true.

use std::collections::{BTreeMap, BTreeSet};

use work_model::{
    Attempt, ConclusionInput, Criterion, CriterionState, CriterionStatus, EXECUTION_SCHEMA,
    ExecutionRecord, ExecutionRecordRequest, REQUIREMENT_ACCEPTANCE_SCHEMA, Reason, RecordId,
    RequiredText, RequirementAcceptanceRecord, RequirementAcceptanceRecordRequest,
    RequirementConclusion, RequirementState, Revision, TASK_ACCEPTANCE_SCHEMA, TASK_SCHEMA,
    TaskAcceptanceRecord, TaskAcceptanceRecordRequest, TaskConclusion, TaskCreateRequest,
    TaskRecord, TaskReviseRequest, TaskStatus, Verdict, WorkError,
};

fn violation(message: impl Into<String>) -> WorkError {
    WorkError::new(Reason::RuleViolation, message)
}

fn require(field: &'static str, value: &str) -> Result<(), WorkError> {
    RequiredText::parse(field, value)
        .map(|_| ())
        .map_err(WorkError::from)
}

fn require_all(field: &'static str, values: &[String]) -> Result<(), WorkError> {
    values.iter().try_for_each(|value| require(field, value))
}

fn require_unique_ids(field: &str, ids: &[RecordId]) -> Result<(), WorkError> {
    let mut seen = BTreeSet::new();
    if ids.iter().all(|id| seen.insert(id)) {
        Ok(())
    } else {
        Err(violation(format!("duplicate {field} link")))
    }
}

fn check_criteria(criteria: &[Criterion]) -> Result<(), WorkError> {
    if criteria.is_empty() {
        return Err(violation("a TASK needs at least one acceptance criterion"));
    }
    let mut seen = BTreeSet::new();
    for criterion in criteria {
        require("criterion.id", &criterion.id)?;
        require("criterion.text", &criterion.text)?;
        if !seen.insert(criterion.id.as_str()) {
            return Err(violation(format!("duplicate criterion: {}", criterion.id)));
        }
    }
    Ok(())
}

/// Build version 1 of a TASK.
pub fn new_task(request: TaskCreateRequest) -> Result<TaskRecord, WorkError> {
    require("title", &request.title)?;
    require("goal", &request.goal)?;
    if request.requirements.is_empty() {
        return Err(violation("a TASK needs at least one requirement"));
    }
    require_all("requirement", &request.requirements)?;
    require_all("constraint", &request.constraints)?;
    require_all("decision", &request.decisions)?;
    require_all("note", &request.notes)?;
    check_criteria(&request.criteria)?;
    require_unique_ids("discussion", &request.discussion_ids)?;
    require_unique_ids("source", &request.source_ids)?;
    Ok(TaskRecord {
        schema: TASK_SCHEMA.to_owned(),
        id: request.id,
        version: 1,
        title: request.title,
        goal: request.goal,
        requirements: request.requirements,
        constraints: request.constraints,
        decisions: request.decisions,
        criteria: request.criteria,
        discussion_ids: request.discussion_ids,
        source_ids: request.source_ids,
        notes: request.notes,
        status: TaskStatus::Open,
        revision: None,
    })
}

fn version_conflict(expected: u32, actual: u32) -> WorkError {
    WorkError::new(
        Reason::VersionConflict,
        format!("expected version {expected} but latest is {actual}"),
    )
}

/// Produce the next TASK version from a revision. The TASK is reopened, the
/// criteria that changed or were named for reconfirmation lose their earlier
/// conclusions, and everything else keeps its standing.
pub fn revise_task(base: &TaskRecord, request: TaskReviseRequest) -> Result<TaskRecord, WorkError> {
    if request.expected_version != base.version {
        return Err(version_conflict(request.expected_version, base.version));
    }
    require("reason", &request.reason)?;
    let mut next = base.clone();
    next.version = base.version + 1;
    if let Some(value) = request.title {
        require("title", &value)?;
        next.title = value;
    }
    if let Some(value) = request.goal {
        require("goal", &value)?;
        next.goal = value;
    }
    if let Some(values) = request.requirements {
        if values.is_empty() {
            return Err(violation("a TASK needs at least one requirement"));
        }
        require_all("requirement", &values)?;
        next.requirements = values;
    }
    if let Some(values) = request.constraints {
        require_all("constraint", &values)?;
        next.constraints = values;
    }
    if let Some(values) = request.decisions {
        require_all("decision", &values)?;
        next.decisions = values;
    }
    if let Some(values) = request.notes {
        require_all("note", &values)?;
        next.notes = values;
    }
    if let Some(values) = request.criteria {
        check_criteria(&values)?;
        next.criteria = values;
    }
    for id in request.link_discussions {
        if next.discussion_ids.contains(&id) {
            return Err(violation(format!(
                "discussion already linked: {}",
                id.as_str()
            )));
        }
        next.discussion_ids.push(id);
    }
    for id in request.link_sources {
        if next.source_ids.contains(&id) {
            return Err(violation(format!("source already linked: {}", id.as_str())));
        }
        next.source_ids.push(id);
    }

    let mut affected: BTreeSet<String> = next
        .criteria
        .iter()
        .filter(|criterion| !base.criteria.contains(criterion))
        .map(|criterion| criterion.id.clone())
        .collect();
    for id in request.reconfirm_criteria {
        if !next.criteria.iter().any(|criterion| criterion.id == id) {
            return Err(violation(format!("unknown criterion to reconfirm: {id}")));
        }
        affected.insert(id);
    }

    let mut unchanged = next.clone();
    unchanged.version = base.version;
    unchanged.status = base.status;
    unchanged.revision = base.revision.clone();
    if unchanged == *base && affected.is_empty() && !request.reconfirm_requirement {
        return Err(WorkError::new(
            Reason::InvalidRequest,
            "the revision changes nothing",
        ));
    }
    next.status = TaskStatus::Open;
    next.revision = Some(Revision {
        reason: request.reason,
        affected_criteria: affected.into_iter().collect(),
        reconfirm_requirement: request.reconfirm_requirement,
    });
    Ok(next)
}

/// State of every criterion of the newest TASK version.
///
/// A conclusion counts only when it was judged on a version at or after the
/// last version that introduced, changed or reconfirmed its criterion.
pub fn criterion_states(
    history: &[TaskRecord],
    acceptance: Option<&TaskAcceptanceRecord>,
) -> Vec<CriterionState> {
    let Some(latest) = history.last() else {
        return Vec::new();
    };
    let mut since: BTreeMap<&str, u32> = BTreeMap::new();
    for (index, task) in history.iter().enumerate() {
        let previous = index.checked_sub(1).map(|before| &history[before]);
        for criterion in &task.criteria {
            let unchanged = previous
                .and_then(|before| before.criteria.iter().find(|item| item.id == criterion.id))
                .is_some_and(|item| item == criterion);
            let reconfirmed = task
                .revision
                .as_ref()
                .is_some_and(|revision| revision.affected_criteria.contains(&criterion.id));
            if !unchanged || reconfirmed {
                since.insert(&criterion.id, task.version);
            }
        }
    }
    latest
        .criteria
        .iter()
        .map(|criterion| {
            let own: Vec<&TaskConclusion> = acceptance
                .map(|record| {
                    record
                        .conclusions
                        .iter()
                        .filter(|item| item.criterion_id == criterion.id)
                        .collect()
                })
                .unwrap_or_default();
            let floor = since.get(criterion.id.as_str()).copied().unwrap_or(1);
            let status = match own.iter().rev().find(|item| item.task_version >= floor) {
                Some(item) if item.verdict == Verdict::Passed => CriterionStatus::Passed,
                Some(_) => CriterionStatus::Failed,
                None if own.is_empty() => CriterionStatus::Missing,
                None => CriterionStatus::NeedsReconfirm,
            };
            CriterionState {
                criterion_id: criterion.id.clone(),
                required: criterion.required,
                status,
            }
        })
        .collect()
}

/// A TASK may complete only when it is open and every required criterion has a
/// current passing conclusion.
pub fn completion_blockers(states: &[CriterionState]) -> Vec<String> {
    states
        .iter()
        .filter(|state| state.required && state.status != CriterionStatus::Passed)
        .map(|state| format!("{} is {:?}", state.criterion_id, state.status))
        .collect()
}

pub fn complete_task(
    base: &TaskRecord,
    expected_version: u32,
    states: &[CriterionState],
) -> Result<TaskRecord, WorkError> {
    if expected_version != base.version {
        return Err(version_conflict(expected_version, base.version));
    }
    if base.status == TaskStatus::Completed {
        return Err(violation("the TASK is already completed"));
    }
    let blockers = completion_blockers(states);
    if !blockers.is_empty() {
        return Err(violation(format!(
            "required acceptance is not passed: {}",
            blockers.join(", ")
        )));
    }
    let mut next = base.clone();
    next.version = base.version + 1;
    next.status = TaskStatus::Completed;
    next.revision = None;
    Ok(next)
}

fn attempt_of(request: ExecutionRecordRequest, task_version: u32) -> Result<Attempt, WorkError> {
    let input = request.attempt;
    require("attempt.id", &input.id)?;
    require_all("change", &input.changes)?;
    require_all("anomaly", &input.anomalies)?;
    require_all("verification", &input.verification)?;
    require_all("evidence", &input.evidence)?;
    Ok(Attempt {
        id: input.id,
        task_version,
        result: input.result,
        changes: input.changes,
        anomalies: input.anomalies,
        verification: input.verification,
        evidence: input.evidence,
    })
}

/// First version of an Execution, holding its first attempt.
pub fn new_execution(
    request: ExecutionRecordRequest,
    task_version: u32,
) -> Result<ExecutionRecord, WorkError> {
    let (id, task_id) = (request.id.clone(), request.task_id.clone());
    Ok(ExecutionRecord {
        schema: EXECUTION_SCHEMA.to_owned(),
        id,
        version: 1,
        task_id,
        attempts: vec![attempt_of(request, task_version)?],
    })
}

/// Next Execution version with one more attempt; earlier attempts stay as is.
pub fn add_attempt(
    base: &ExecutionRecord,
    request: ExecutionRecordRequest,
    expected_version: u32,
    task_version: u32,
) -> Result<ExecutionRecord, WorkError> {
    if expected_version != base.version {
        return Err(version_conflict(expected_version, base.version));
    }
    if request.task_id != base.task_id {
        return Err(violation("an Execution belongs to one TASK"));
    }
    let attempt = attempt_of(request, task_version)?;
    if base.attempts.iter().any(|item| item.id == attempt.id) {
        return Err(violation(format!("duplicate attempt: {}", attempt.id)));
    }
    let mut next = base.clone();
    next.version = base.version + 1;
    next.attempts.push(attempt);
    Ok(next)
}

fn conclusions_of(
    task: &TaskRecord,
    task_version: u32,
    inputs: Vec<ConclusionInput>,
) -> Result<Vec<TaskConclusion>, WorkError> {
    if inputs.is_empty() {
        return Err(violation("at least one conclusion is required"));
    }
    let mut seen = BTreeSet::new();
    let mut conclusions = Vec::new();
    for input in inputs {
        if !task
            .criteria
            .iter()
            .any(|criterion| criterion.id == input.criterion_id)
        {
            return Err(violation(format!(
                "unknown criterion: {}",
                input.criterion_id
            )));
        }
        if !seen.insert(input.criterion_id.clone()) {
            return Err(violation(format!(
                "criterion concluded twice: {}",
                input.criterion_id
            )));
        }
        require("evidence", &input.evidence)?;
        conclusions.push(TaskConclusion {
            criterion_id: input.criterion_id,
            task_version,
            verdict: input.verdict,
            evidence: input.evidence,
            origin: input.origin,
        });
    }
    Ok(conclusions)
}

fn judged_on_latest(task: &TaskRecord, task_version: u32) -> Result<(), WorkError> {
    if task_version == task.version {
        Ok(())
    } else {
        Err(WorkError::new(
            Reason::VersionConflict,
            format!(
                "conclusions were judged on TASK version {task_version} but latest is {}",
                task.version
            ),
        ))
    }
}

pub fn new_task_acceptance(
    request: TaskAcceptanceRecordRequest,
    task: &TaskRecord,
) -> Result<TaskAcceptanceRecord, WorkError> {
    judged_on_latest(task, request.task_version)?;
    Ok(TaskAcceptanceRecord {
        schema: TASK_ACCEPTANCE_SCHEMA.to_owned(),
        id: request.id,
        version: 1,
        task_id: request.task_id,
        conclusions: conclusions_of(task, request.task_version, request.conclusions)?,
    })
}

pub fn append_task_acceptance(
    base: &TaskAcceptanceRecord,
    request: TaskAcceptanceRecordRequest,
    expected_version: u32,
    task: &TaskRecord,
) -> Result<TaskAcceptanceRecord, WorkError> {
    if expected_version != base.version {
        return Err(version_conflict(expected_version, base.version));
    }
    if request.task_id != base.task_id {
        return Err(violation("an acceptance belongs to one TASK"));
    }
    judged_on_latest(task, request.task_version)?;
    let mut next = base.clone();
    next.version = base.version + 1;
    next.conclusions.extend(conclusions_of(
        task,
        request.task_version,
        request.conclusions,
    )?);
    Ok(next)
}

fn requirement_conclusion(
    request: RequirementAcceptanceRecordRequest,
    discussion_version: u32,
) -> Result<RequirementConclusion, WorkError> {
    require("evidence", &request.evidence)?;
    require_task_refs_unique(&request)?;
    Ok(RequirementConclusion {
        discussion_version,
        verdict: request.verdict,
        evidence: request.evidence,
        origin: request.origin,
        task_refs: request.task_refs,
    })
}

fn require_task_refs_unique(request: &RequirementAcceptanceRecordRequest) -> Result<(), WorkError> {
    let mut seen = BTreeSet::new();
    if request
        .task_refs
        .iter()
        .all(|item| seen.insert((item.id.clone(), item.version)))
    {
        Ok(())
    } else {
        Err(violation("duplicate TASK reference"))
    }
}

pub fn new_requirement_acceptance(
    request: RequirementAcceptanceRecordRequest,
    discussion_version: u32,
) -> Result<RequirementAcceptanceRecord, WorkError> {
    let (id, discussion_id) = (request.id.clone(), request.discussion_id.clone());
    Ok(RequirementAcceptanceRecord {
        schema: REQUIREMENT_ACCEPTANCE_SCHEMA.to_owned(),
        id,
        version: 1,
        discussion_id,
        conclusions: vec![requirement_conclusion(request, discussion_version)?],
    })
}

pub fn append_requirement_acceptance(
    base: &RequirementAcceptanceRecord,
    request: RequirementAcceptanceRecordRequest,
    expected_version: u32,
    discussion_version: u32,
) -> Result<RequirementAcceptanceRecord, WorkError> {
    if expected_version != base.version {
        return Err(version_conflict(expected_version, base.version));
    }
    if request.discussion_id != base.discussion_id {
        return Err(violation("an acceptance belongs to one Discussion"));
    }
    let mut next = base.clone();
    next.version = base.version + 1;
    next.conclusions
        .push(requirement_conclusion(request, discussion_version)?);
    Ok(next)
}

/// Requirement state from its own conclusions only. TASK completion never
/// passes it; a TASK revision may ask for it to be confirmed again.
pub fn requirement_state(
    record: Option<&RequirementAcceptanceRecord>,
    linked_tasks: &[Vec<TaskRecord>],
) -> RequirementState {
    let Some(newest) = record.and_then(|item| item.conclusions.last()) else {
        return RequirementState {
            verdict: None,
            needs_reconfirm: false,
        };
    };
    let needs_reconfirm = linked_tasks.iter().any(|history| {
        history.iter().any(|task| {
            task.revision
                .as_ref()
                .is_some_and(|revision| revision.reconfirm_requirement)
                && !newest
                    .task_refs
                    .iter()
                    .any(|item| item.id == task.id && item.version >= task.version)
        })
    });
    RequirementState {
        verdict: Some(newest.verdict),
        needs_reconfirm,
    }
}

#[cfg(test)]
mod tests;
