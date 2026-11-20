//! Use cases over TASK, Execution/Attempt, the two Acceptance layers and the
//! traceability query that links them to Discussion and Source.

use std::collections::BTreeSet;

use work_model::{
    CorrectionRecord, CriterionState, EXECUTION_RECORD_SCHEMA, ExecutionRecord,
    ExecutionRecordRequest, ExecutionTrace, REQUIREMENT_ACCEPTANCE_RECORD_SCHEMA, Reason, RecordId,
    RecordKind, RecordRef, RequirementAcceptanceRecord, RequirementAcceptanceRecordRequest,
    RequirementState, TASK_ACCEPTANCE_RECORD_SCHEMA, TASK_APPLY_SCHEMA, TASK_COMPLETE_SCHEMA,
    TASK_CREATE_SCHEMA, TASK_PREVIEW_SCHEMA, TASK_REVISE_SCHEMA, TaskAcceptanceRecord,
    TaskAcceptanceRecordRequest, TaskApplyRequest, TaskCompleteRequest, TaskDraft,
    TaskPreviewRequest, TaskRecord, TaskStatus, TaskTrace, WorkError,
};
use work_operations::{
    add_attempt, append_requirement_acceptance, append_task_acceptance, complete_task as complete,
    completion_blockers, criterion_states, encode, new_execution, new_requirement_acceptance,
    new_task, new_task_acceptance, requirement_state, revise_task as revise,
};

use crate::port::{Precondition, RecordStore, StoredRecord, WritePlan};
use crate::usecase::{
    check_schema, corrections_for, latest, not_found, read_discussion, reference, require_sources,
};

fn mismatch() -> WorkError {
    WorkError::new(Reason::InvalidRecord, "record kind mismatch")
}

fn read_task(
    store: &impl RecordStore,
    id: &RecordId,
    version: u32,
) -> Result<TaskRecord, WorkError> {
    match store.read(RecordKind::Task, id, version)? {
        StoredRecord::Task(record) => Ok(record),
        _ => Err(mismatch()),
    }
}

fn read_execution(
    store: &impl RecordStore,
    id: &RecordId,
    version: u32,
) -> Result<ExecutionRecord, WorkError> {
    match store.read(RecordKind::Execution, id, version)? {
        StoredRecord::Execution(record) => Ok(record),
        _ => Err(mismatch()),
    }
}

fn read_task_acceptance(
    store: &impl RecordStore,
    id: &RecordId,
    version: u32,
) -> Result<TaskAcceptanceRecord, WorkError> {
    match store.read(RecordKind::TaskAcceptance, id, version)? {
        StoredRecord::TaskAcceptance(record) => Ok(record),
        _ => Err(mismatch()),
    }
}

fn read_requirement_acceptance(
    store: &impl RecordStore,
    id: &RecordId,
    version: u32,
) -> Result<RequirementAcceptanceRecord, WorkError> {
    match store.read(RecordKind::RequirementAcceptance, id, version)? {
        StoredRecord::RequirementAcceptance(record) => Ok(record),
        _ => Err(mismatch()),
    }
}

/// Newest version of every record of one kind, in identifier order.
fn latest_all(store: &impl RecordStore, kind: RecordKind) -> Result<Vec<StoredRecord>, WorkError> {
    store
        .ids(kind)?
        .iter()
        .map(|id| store.read(kind, id, latest(store, kind, id)?))
        .collect()
}

fn all_tasks(store: &impl RecordStore) -> Result<Vec<TaskRecord>, WorkError> {
    Ok(latest_all(store, RecordKind::Task)?
        .into_iter()
        .filter_map(|record| match record {
            StoredRecord::Task(task) => Some(task),
            _ => None,
        })
        .collect())
}

fn task_history(store: &impl RecordStore, id: &RecordId) -> Result<Vec<TaskRecord>, WorkError> {
    let newest = latest(store, RecordKind::Task, id)?;
    (1..=newest)
        .map(|version| read_task(store, id, version))
        .collect()
}

fn acceptance_of_task(
    store: &impl RecordStore,
    task_id: &RecordId,
) -> Result<Option<TaskAcceptanceRecord>, WorkError> {
    Ok(latest_all(store, RecordKind::TaskAcceptance)?
        .into_iter()
        .find_map(|record| match record {
            StoredRecord::TaskAcceptance(item) if &item.task_id == task_id => Some(item),
            _ => None,
        }))
}

fn acceptance_of_discussion(
    store: &impl RecordStore,
    discussion_id: &RecordId,
) -> Result<Option<RequirementAcceptanceRecord>, WorkError> {
    Ok(latest_all(store, RecordKind::RequirementAcceptance)?
        .into_iter()
        .find_map(|record| match record {
            StoredRecord::RequirementAcceptance(item) if &item.discussion_id == discussion_id => {
                Some(item)
            }
            _ => None,
        }))
}

fn executions_of(
    store: &impl RecordStore,
    task_id: &RecordId,
) -> Result<Vec<ExecutionRecord>, WorkError> {
    Ok(latest_all(store, RecordKind::Execution)?
        .into_iter()
        .filter_map(|record| match record {
            StoredRecord::Execution(item) if &item.task_id == task_id => Some(item),
            _ => None,
        })
        .collect())
}

fn discussion_preconditions(
    store: &impl RecordStore,
    ids: &[RecordId],
) -> Result<Vec<Precondition>, WorkError> {
    ids.iter()
        .map(|id| {
            Ok(Precondition {
                kind: RecordKind::Discussion,
                id: id.clone(),
                latest: Some(latest(store, RecordKind::Discussion, id)?),
            })
        })
        .collect()
}

fn source_preconditions(ids: &[RecordId]) -> Vec<Precondition> {
    ids.iter()
        .map(|id| Precondition {
            kind: RecordKind::Source,
            id: id.clone(),
            latest: Some(1),
        })
        .collect()
}

fn only(kind: RecordKind, id: &RecordId, latest: Option<u32>) -> Precondition {
    Precondition {
        kind,
        id: id.clone(),
        latest,
    }
}

/// Criterion states of `version`, ignoring conclusions judged on later versions.
fn states_at(
    history: &[TaskRecord],
    version: u32,
    acceptance: Option<&TaskAcceptanceRecord>,
) -> Vec<CriterionState> {
    let end = (version as usize).min(history.len());
    let visible = acceptance.map(|record| {
        let mut record = record.clone();
        record
            .conclusions
            .retain(|item| item.task_version <= version);
        record
    });
    criterion_states(&history[..end], visible.as_ref())
}

/// Construct the exact reviewed TASK record plus every source-of-truth version
/// it depends upon. The resulting write plan is not applied until approved.
fn task_candidate(
    store: &impl RecordStore,
    draft: TaskDraft,
) -> Result<(StoredRecord, Vec<Precondition>), WorkError> {
    let (record, mut preconditions) = match draft {
        TaskDraft::Create(request) => {
            check_schema(&request.schema, TASK_CREATE_SCHEMA)?;
            if store
                .latest_version(RecordKind::Task, &request.id)?
                .is_some()
            {
                return Err(WorkError::new(Reason::AlreadyExists, "TASK already exists"));
            }
            let record = new_task(request)?;
            let conditions = vec![only(RecordKind::Task, &record.id, None)];
            (record, conditions)
        }
        TaskDraft::Revise(request) => {
            check_schema(&request.schema, TASK_REVISE_SCHEMA)?;
            let newest = latest(store, RecordKind::Task, &request.id)?;
            let base = read_task(store, &request.id, newest)?;
            let record = revise(&base, request)?;
            let mut conditions = vec![only(RecordKind::Task, &record.id, Some(newest))];
            if let Some(acceptance) = acceptance_of_task(store, &record.id)? {
                conditions.push(only(
                    RecordKind::TaskAcceptance,
                    &acceptance.id,
                    Some(acceptance.version),
                ));
            }
            (record, conditions)
        }
    };
    // Bind the full resulting set, not only links added by the revision.
    // Versions are rechecked under the store's exclusive commit lock.
    require_sources(store, &record.source_ids)?;
    preconditions.extend(discussion_preconditions(store, &record.discussion_ids)?);
    preconditions.extend(source_preconditions(&record.source_ids));
    Ok((StoredRecord::Task(record), preconditions))
}

fn dependencies(preconditions: &[Precondition]) -> Vec<RecordRef> {
    preconditions
        .iter()
        .filter_map(|item| {
            item.latest.map(|version| RecordRef {
                kind: item.kind,
                id: item.id.clone(),
                version,
            })
        })
        .collect()
}

/// Preview computes a human-reviewable candidate without publishing a TASK.
pub fn task_preview(
    store: &impl RecordStore,
    request: TaskPreviewRequest,
) -> Result<(String, Vec<RecordRef>), WorkError> {
    check_schema(&request.schema, TASK_PREVIEW_SCHEMA)?;
    let (record, conditions) = task_candidate(store, request.draft)?;
    let StoredRecord::Task(record) = record else {
        unreachable!()
    };
    Ok((encode(&record)?, dependencies(&conditions)))
}

/// Apply exactly the approved candidate, with every reviewed dependency still
/// at its reviewed version. Only this use case can publish a creation/revision.
pub fn task_apply(
    store: &impl RecordStore,
    request: TaskApplyRequest,
) -> Result<RecordRef, WorkError> {
    check_schema(&request.schema, TASK_APPLY_SCHEMA)?;
    let (record, conditions) = task_candidate(store, request.draft)?;
    let StoredRecord::Task(candidate) = &record else {
        unreachable!()
    };
    if encode(candidate)? != request.approved_candidate {
        return Err(WorkError::new(
            Reason::ApprovalMismatch,
            "the approved TASK candidate differs from the candidate to apply",
        ));
    }
    if dependencies(&conditions) != request.reviewed_dependencies {
        return Err(WorkError::new(
            Reason::VersionConflict,
            "TASK dependencies changed since preview",
        ));
    }
    store.commit(&WritePlan {
        preconditions: conditions,
        writes: vec![record.clone()],
    })?;
    Ok(reference(&record))
}

/// Complete a TASK. Its required criteria must all hold a current passing
/// conclusion; nothing else (no Execution result, no Requirement) can stand in.
pub fn complete_task(
    store: &impl RecordStore,
    request: TaskCompleteRequest,
) -> Result<RecordRef, WorkError> {
    check_schema(&request.schema, TASK_COMPLETE_SCHEMA)?;
    let id = request.id;
    let newest = latest(store, RecordKind::Task, &id)?;
    let history = task_history(store, &id)?;
    let acceptance = acceptance_of_task(store, &id)?;
    let states = states_at(&history, newest, acceptance.as_ref());
    let next = StoredRecord::Task(complete(
        &history[history.len() - 1],
        request.expected_version,
        &states,
    )?);
    let mut preconditions = vec![only(RecordKind::Task, &id, Some(newest))];
    if let Some(record) = &acceptance {
        preconditions.push(only(
            RecordKind::TaskAcceptance,
            &record.id,
            Some(record.version),
        ));
    }
    store.commit(&WritePlan {
        preconditions,
        writes: vec![next.clone()],
    })?;
    Ok(reference(&next))
}

pub struct TaskView {
    pub record: TaskRecord,
    pub latest_version: u32,
    pub criteria: Vec<CriterionState>,
    pub completable: bool,
    pub executions: Vec<RecordId>,
    pub acceptance: Option<RecordId>,
    pub corrections: Vec<CorrectionRecord>,
}

pub fn show_task(
    store: &impl RecordStore,
    id: &RecordId,
    version: Option<u32>,
) -> Result<TaskView, WorkError> {
    let newest = latest(store, RecordKind::Task, id)?;
    let shown = version.unwrap_or(newest);
    let history = task_history(store, id)?;
    let record = history
        .get((shown as usize).wrapping_sub(1))
        .cloned()
        .ok_or_else(|| not_found(RecordKind::Task, id))?;
    let acceptance = acceptance_of_task(store, id)?;
    let criteria = states_at(&history, shown, acceptance.as_ref());
    let completable = shown == newest
        && record.status == TaskStatus::Open
        && completion_blockers(&criteria).is_empty();
    Ok(TaskView {
        record,
        latest_version: newest,
        criteria,
        completable,
        executions: executions_of(store, id)?
            .into_iter()
            .map(|item| item.id)
            .collect(),
        acceptance: acceptance.map(|item| item.id),
        corrections: corrections_for(store, RecordKind::Task, id)?,
    })
}

pub fn record_execution(
    store: &impl RecordStore,
    request: ExecutionRecordRequest,
) -> Result<RecordRef, WorkError> {
    check_schema(&request.schema, EXECUTION_RECORD_SCHEMA)?;
    let task_newest = latest(store, RecordKind::Task, &request.task_id)?;
    let mut preconditions = vec![only(RecordKind::Task, &request.task_id, Some(task_newest))];
    let record = match request.expected_version {
        None => {
            preconditions.push(only(RecordKind::Execution, &request.id, None));
            new_execution(request, task_newest)?
        }
        Some(expected) => {
            let newest = latest(store, RecordKind::Execution, &request.id)?;
            preconditions.push(only(RecordKind::Execution, &request.id, Some(newest)));
            let base = read_execution(store, &request.id, newest)?;
            add_attempt(&base, request, expected, task_newest)?
        }
    };
    let stored = StoredRecord::Execution(record);
    store.commit(&WritePlan {
        preconditions,
        writes: vec![stored.clone()],
    })?;
    Ok(reference(&stored))
}

pub fn show_execution(
    store: &impl RecordStore,
    id: &RecordId,
    version: Option<u32>,
) -> Result<(ExecutionRecord, u32, Vec<CorrectionRecord>), WorkError> {
    let newest = latest(store, RecordKind::Execution, id)?;
    let record = read_execution(store, id, version.unwrap_or(newest))?;
    Ok((
        record,
        newest,
        corrections_for(store, RecordKind::Execution, id)?,
    ))
}

pub fn record_task_acceptance(
    store: &impl RecordStore,
    request: TaskAcceptanceRecordRequest,
) -> Result<RecordRef, WorkError> {
    check_schema(&request.schema, TASK_ACCEPTANCE_RECORD_SCHEMA)?;
    let task_newest = latest(store, RecordKind::Task, &request.task_id)?;
    let task = read_task(store, &request.task_id, task_newest)?;
    let mut preconditions = vec![only(RecordKind::Task, &request.task_id, Some(task_newest))];
    let record = match request.expected_version {
        None => {
            if let Some(existing) = acceptance_of_task(store, &request.task_id)? {
                return Err(WorkError::new(
                    Reason::AlreadyExists,
                    format!(
                        "TASK {} already has acceptance {}; append with expected_version",
                        request.task_id.as_str(),
                        existing.id.as_str()
                    ),
                ));
            }
            preconditions.push(only(RecordKind::TaskAcceptance, &request.id, None));
            new_task_acceptance(request, &task)?
        }
        Some(expected) => {
            let newest = latest(store, RecordKind::TaskAcceptance, &request.id)?;
            preconditions.push(only(RecordKind::TaskAcceptance, &request.id, Some(newest)));
            let base = read_task_acceptance(store, &request.id, newest)?;
            append_task_acceptance(&base, request, expected, &task)?
        }
    };
    let stored = StoredRecord::TaskAcceptance(record);
    store.commit(&WritePlan {
        preconditions,
        writes: vec![stored.clone()],
    })?;
    Ok(reference(&stored))
}

pub struct TaskAcceptanceView {
    pub record: TaskAcceptanceRecord,
    pub latest_version: u32,
    pub criteria: Vec<CriterionState>,
    pub corrections: Vec<CorrectionRecord>,
}

pub fn show_task_acceptance(
    store: &impl RecordStore,
    id: &RecordId,
    version: Option<u32>,
) -> Result<TaskAcceptanceView, WorkError> {
    let newest = latest(store, RecordKind::TaskAcceptance, id)?;
    let record = read_task_acceptance(store, id, version.unwrap_or(newest))?;
    let newest_record = read_task_acceptance(store, id, newest)?;
    let history = task_history(store, &record.task_id)?;
    Ok(TaskAcceptanceView {
        criteria: states_at(&history, history.len() as u32, Some(&newest_record)),
        latest_version: newest,
        corrections: corrections_for(store, RecordKind::TaskAcceptance, id)?,
        record,
    })
}

fn check_task_refs(
    store: &impl RecordStore,
    request: &RequirementAcceptanceRecordRequest,
) -> Result<Vec<Precondition>, WorkError> {
    let mut preconditions = Vec::new();
    for item in &request.task_refs {
        if item.kind != RecordKind::Task {
            return Err(WorkError::new(
                Reason::RuleViolation,
                "task_refs may only reference TASK versions",
            ));
        }
        let newest = latest(store, RecordKind::Task, &item.id)?;
        if item.version == 0 || item.version > newest {
            return Err(WorkError::new(
                Reason::NotFound,
                format!("task {} has no version {}", item.id.as_str(), item.version),
            ));
        }
        if !read_task(store, &item.id, newest)?
            .discussion_ids
            .contains(&request.discussion_id)
        {
            return Err(WorkError::new(
                Reason::RuleViolation,
                format!(
                    "task {} is not linked to Discussion {}",
                    item.id.as_str(),
                    request.discussion_id.as_str()
                ),
            ));
        }
        preconditions.push(only(RecordKind::Task, &item.id, Some(newest)));
    }
    let mut seen = BTreeSet::new();
    preconditions.retain(|item| seen.insert(item.id.clone()));
    Ok(preconditions)
}

pub fn record_requirement_acceptance(
    store: &impl RecordStore,
    request: RequirementAcceptanceRecordRequest,
) -> Result<RecordRef, WorkError> {
    check_schema(&request.schema, REQUIREMENT_ACCEPTANCE_RECORD_SCHEMA)?;
    let discussion_newest = latest(store, RecordKind::Discussion, &request.discussion_id)?;
    read_discussion(store, &request.discussion_id, discussion_newest)?;
    let mut preconditions = vec![only(
        RecordKind::Discussion,
        &request.discussion_id,
        Some(discussion_newest),
    )];
    preconditions.extend(check_task_refs(store, &request)?);
    let record = match request.expected_version {
        None => {
            if let Some(existing) = acceptance_of_discussion(store, &request.discussion_id)? {
                return Err(WorkError::new(
                    Reason::AlreadyExists,
                    format!(
                        "Discussion {} already has acceptance {}; append with expected_version",
                        request.discussion_id.as_str(),
                        existing.id.as_str()
                    ),
                ));
            }
            preconditions.push(only(RecordKind::RequirementAcceptance, &request.id, None));
            new_requirement_acceptance(request, discussion_newest)?
        }
        Some(expected) => {
            let newest = latest(store, RecordKind::RequirementAcceptance, &request.id)?;
            preconditions.push(only(
                RecordKind::RequirementAcceptance,
                &request.id,
                Some(newest),
            ));
            let base = read_requirement_acceptance(store, &request.id, newest)?;
            append_requirement_acceptance(&base, request, expected, discussion_newest)?
        }
    };
    let stored = StoredRecord::RequirementAcceptance(record);
    store.commit(&WritePlan {
        preconditions,
        writes: vec![stored.clone()],
    })?;
    Ok(reference(&stored))
}

pub struct RequirementAcceptanceView {
    pub record: RequirementAcceptanceRecord,
    pub latest_version: u32,
    pub state: RequirementState,
    pub corrections: Vec<CorrectionRecord>,
}

pub fn show_requirement_acceptance(
    store: &impl RecordStore,
    id: &RecordId,
    version: Option<u32>,
) -> Result<RequirementAcceptanceView, WorkError> {
    let newest = latest(store, RecordKind::RequirementAcceptance, id)?;
    let record = read_requirement_acceptance(store, id, version.unwrap_or(newest))?;
    let newest_record = read_requirement_acceptance(store, id, newest)?;
    let mut histories = Vec::new();
    for task in all_tasks(store)? {
        if task.discussion_ids.contains(&newest_record.discussion_id) {
            histories.push(task_history(store, &task.id)?);
        }
    }
    Ok(RequirementAcceptanceView {
        state: requirement_state(Some(&newest_record), &histories),
        latest_version: newest,
        corrections: corrections_for(store, RecordKind::RequirementAcceptance, id)?,
        record,
    })
}

fn sorted(set: BTreeSet<RecordId>) -> Vec<RecordId> {
    set.into_iter().collect()
}

/// Links around one record: Source → Discussion → TASK → Execution →
/// Acceptance, derived only from stored records.
/// Internal trace result; the public JSON envelope is assembled by Flow.
pub struct TraceView {
    pub root: RecordRef,
    pub sources: Vec<RecordId>,
    pub discussions: Vec<RecordId>,
    pub tasks: Vec<TaskTrace>,
    pub requirement_acceptances: Vec<RecordId>,
}

pub fn trace(
    store: &impl RecordStore,
    kind: RecordKind,
    id: &RecordId,
) -> Result<TraceView, WorkError> {
    let newest = latest(store, kind, id)?;
    let tasks = all_tasks(store)?;
    let mut discussions = BTreeSet::new();
    let mut sources = BTreeSet::new();
    let mut linked: Vec<&TaskRecord> = Vec::new();
    let discussion_sources = |discussion: &RecordId| -> Result<Vec<RecordId>, WorkError> {
        let newest = latest(store, RecordKind::Discussion, discussion)?;
        Ok(read_discussion(store, discussion, newest)?.source_ids)
    };

    // Resolve the record to a TASK, a Discussion or a Source anchor.
    let (anchor_kind, anchor_id) = match kind {
        RecordKind::Execution => (RecordKind::Task, read_execution(store, id, newest)?.task_id),
        RecordKind::TaskAcceptance => (
            RecordKind::Task,
            read_task_acceptance(store, id, newest)?.task_id,
        ),
        RecordKind::RequirementAcceptance => (
            RecordKind::Discussion,
            read_requirement_acceptance(store, id, newest)?.discussion_id,
        ),
        RecordKind::Correction => {
            return Err(WorkError::new(
                Reason::InvalidRequest,
                "trace starts from a Source, Discussion, TASK, Execution or Acceptance",
            ));
        }
        other => (other, id.clone()),
    };

    match anchor_kind {
        RecordKind::Task => {
            let task = tasks
                .iter()
                .find(|item| item.id == anchor_id)
                .ok_or_else(|| not_found(RecordKind::Task, &anchor_id))?;
            linked.push(task);
        }
        RecordKind::Discussion => {
            discussions.insert(anchor_id.clone());
            linked.extend(
                tasks
                    .iter()
                    .filter(|item| item.discussion_ids.contains(&anchor_id)),
            );
        }
        _ => {
            sources.insert(anchor_id.clone());
            for discussion in latest_all(store, RecordKind::Discussion)? {
                if let StoredRecord::Discussion(item) = discussion
                    && item.source_ids.contains(&anchor_id)
                {
                    discussions.insert(item.id);
                }
            }
            linked.extend(tasks.iter().filter(|item| {
                item.source_ids.contains(&anchor_id)
                    || item.discussion_ids.iter().any(|d| discussions.contains(d))
            }));
        }
    }
    for task in &linked {
        discussions.extend(task.discussion_ids.iter().cloned());
        sources.extend(task.source_ids.iter().cloned());
    }
    for discussion in discussions.clone() {
        sources.extend(discussion_sources(&discussion)?);
    }

    let mut task_traces = Vec::new();
    for task in &linked {
        let history = task_history(store, &task.id)?;
        let acceptance = acceptance_of_task(store, &task.id)?;
        task_traces.push(TaskTrace {
            id: task.id.clone(),
            version: task.version,
            status: task.status,
            discussions: task.discussion_ids.clone(),
            sources: task.source_ids.clone(),
            executions: executions_of(store, &task.id)?
                .into_iter()
                .map(|item| ExecutionTrace {
                    id: item.id,
                    attempts: item.attempts.len(),
                })
                .collect(),
            criteria: states_at(&history, task.version, acceptance.as_ref()),
            acceptance: acceptance.map(|item| item.id),
        });
    }
    let mut requirement_acceptances = BTreeSet::new();
    for record in latest_all(store, RecordKind::RequirementAcceptance)? {
        if let StoredRecord::RequirementAcceptance(item) = record
            && discussions.contains(&item.discussion_id)
        {
            requirement_acceptances.insert(item.id);
        }
    }
    Ok(TraceView {
        root: RecordRef {
            kind,
            id: id.clone(),
            version: newest,
        },
        sources: sorted(sources),
        discussions: sorted(discussions),
        tasks: task_traces,
        requirement_acceptances: sorted(requirement_acceptances),
    })
}

#[cfg(test)]
mod tests;
