//! Use cases the Work CLI exposes. Each delegates to exactly one Feature use case.

use work_feature::RecordStore;
use work_model::{
    CORRECTION_LIST_RESPONSE, CORRECTION_PREVIEW_RESPONSE, CorrectionApplyRequest,
    CorrectionDraftRequest, CorrectionListData, CorrectionPreviewData, DISCUSSION_RESPONSE,
    DiscussionCreateRequest, DiscussionData, DiscussionRoundRequest, EXECUTION_RESPONSE,
    ExecutionData, ExecutionRecordRequest, HISTORY_RESPONSE, HistoryData, PROGRESS_RESPONSE,
    ProgressData, REQUIREMENT_ACCEPTANCE_RESPONSE, RecordId, RecordKind, RecordRef,
    RequirementAcceptanceData, RequirementAcceptanceRecordRequest, SOURCE_RESPONSE,
    STATUS_RESPONSE, SourceCreateRequest, SourceData, StatusData, TASK_ACCEPTANCE_RESPONSE,
    TASK_PREVIEW_RESPONSE, TASK_RESPONSE, TRACE_RESPONSE, TaskAcceptanceData,
    TaskAcceptanceRecordRequest, TaskApplyRequest, TaskCompleteRequest, TaskData, TaskPreviewData,
    TaskPreviewRequest, TraceData, WRITE_RESPONSE, WorkError, WriteData,
};

fn written(reference: RecordRef) -> WriteData {
    WriteData {
        schema: WRITE_RESPONSE,
        kind: reference.kind,
        id: reference.id,
        version: reference.version,
    }
}

pub fn source_create(
    store: &impl RecordStore,
    request: SourceCreateRequest,
) -> Result<WriteData, WorkError> {
    work_feature::create_source(store, request).map(written)
}

pub fn source_show(
    store: &impl RecordStore,
    id: &RecordId,
    version: Option<u32>,
) -> Result<SourceData, WorkError> {
    let (record, referenced_by, corrections) = work_feature::show_source(store, id, version)?;
    Ok(SourceData {
        schema: SOURCE_RESPONSE,
        record,
        referenced_by,
        corrections,
    })
}

pub fn discussion_create(
    store: &impl RecordStore,
    request: DiscussionCreateRequest,
) -> Result<WriteData, WorkError> {
    work_feature::create_discussion(store, request).map(written)
}

pub fn discussion_round(
    store: &impl RecordStore,
    request: DiscussionRoundRequest,
) -> Result<WriteData, WorkError> {
    work_feature::discussion_round(store, request).map(written)
}

pub fn discussion_show(
    store: &impl RecordStore,
    id: &RecordId,
    version: Option<u32>,
) -> Result<DiscussionData, WorkError> {
    let (record, latest_version, corrections) = work_feature::show_discussion(store, id, version)?;
    Ok(DiscussionData {
        schema: DISCUSSION_RESPONSE,
        record,
        latest_version,
        corrections,
    })
}

pub fn discussion_history(
    store: &impl RecordStore,
    id: &RecordId,
) -> Result<HistoryData, WorkError> {
    Ok(HistoryData {
        schema: HISTORY_RESPONSE,
        id: id.clone(),
        versions: work_feature::discussion_history(store, id)?,
    })
}

pub fn discussion_progress(
    store: &impl RecordStore,
    id: &RecordId,
) -> Result<ProgressData, WorkError> {
    let record = work_feature::show_progress(store, id)?;
    Ok(ProgressData {
        schema: PROGRESS_RESPONSE,
        id: record.id,
        version: record.version,
        status: record.status,
        questions: record.questions.len(),
        answered: record.answers.len(),
        outstanding_question: record
            .questions
            .iter()
            .find(|question| {
                !record
                    .answers
                    .iter()
                    .any(|answer| answer.question_id == question.id)
            })
            .cloned(),
        open_pending: record
            .pending
            .into_iter()
            .filter(|item| !item.resolved)
            .collect(),
        decisions: record.decisions,
    })
}

pub fn status(store: &impl RecordStore) -> Result<StatusData, WorkError> {
    Ok(StatusData {
        schema: STATUS_RESPONSE,
        records: work_feature::status(store)?,
    })
}

pub fn correction_preview(
    store: &impl RecordStore,
    draft: CorrectionDraftRequest,
) -> Result<CorrectionPreviewData, WorkError> {
    let (candidate, reviewed_target_version) = work_feature::correction_preview(store, draft)?;
    Ok(CorrectionPreviewData {
        schema: CORRECTION_PREVIEW_RESPONSE,
        candidate,
        reviewed_target_version,
    })
}

pub fn correction_apply(
    store: &impl RecordStore,
    request: CorrectionApplyRequest,
) -> Result<WriteData, WorkError> {
    work_feature::correction_apply(store, request).map(written)
}

pub fn correction_list(
    store: &impl RecordStore,
    kind: RecordKind,
    id: &RecordId,
) -> Result<CorrectionListData, WorkError> {
    Ok(CorrectionListData {
        schema: CORRECTION_LIST_RESPONSE,
        corrections: work_feature::corrections_for(store, kind, id)?,
    })
}

pub fn task_preview(
    store: &impl RecordStore,
    request: TaskPreviewRequest,
) -> Result<TaskPreviewData, WorkError> {
    let (candidate, reviewed_dependencies) = work_feature::task_preview(store, request)?;
    Ok(TaskPreviewData {
        schema: TASK_PREVIEW_RESPONSE,
        candidate,
        reviewed_dependencies,
    })
}

pub fn task_apply(
    store: &impl RecordStore,
    request: TaskApplyRequest,
) -> Result<WriteData, WorkError> {
    work_feature::task_apply(store, request).map(written)
}

pub fn task_complete(
    store: &impl RecordStore,
    request: TaskCompleteRequest,
) -> Result<WriteData, WorkError> {
    work_feature::complete_task(store, request).map(written)
}

pub fn task_show(
    store: &impl RecordStore,
    id: &RecordId,
    version: Option<u32>,
) -> Result<TaskData, WorkError> {
    let view = work_feature::show_task(store, id, version)?;
    Ok(TaskData {
        schema: TASK_RESPONSE,
        record: view.record,
        latest_version: view.latest_version,
        criteria: view.criteria,
        completable: view.completable,
        executions: view.executions,
        acceptance: view.acceptance,
        corrections: view.corrections,
    })
}

pub fn execution_record(
    store: &impl RecordStore,
    request: ExecutionRecordRequest,
) -> Result<WriteData, WorkError> {
    work_feature::record_execution(store, request).map(written)
}

pub fn execution_show(
    store: &impl RecordStore,
    id: &RecordId,
    version: Option<u32>,
) -> Result<ExecutionData, WorkError> {
    let (record, latest_version, corrections) = work_feature::show_execution(store, id, version)?;
    Ok(ExecutionData {
        schema: EXECUTION_RESPONSE,
        record,
        latest_version,
        corrections,
    })
}

pub fn task_acceptance_record(
    store: &impl RecordStore,
    request: TaskAcceptanceRecordRequest,
) -> Result<WriteData, WorkError> {
    work_feature::record_task_acceptance(store, request).map(written)
}

pub fn task_acceptance_show(
    store: &impl RecordStore,
    id: &RecordId,
    version: Option<u32>,
) -> Result<TaskAcceptanceData, WorkError> {
    let view = work_feature::show_task_acceptance(store, id, version)?;
    Ok(TaskAcceptanceData {
        schema: TASK_ACCEPTANCE_RESPONSE,
        record: view.record,
        latest_version: view.latest_version,
        criteria: view.criteria,
        corrections: view.corrections,
    })
}

pub fn requirement_acceptance_record(
    store: &impl RecordStore,
    request: RequirementAcceptanceRecordRequest,
) -> Result<WriteData, WorkError> {
    work_feature::record_requirement_acceptance(store, request).map(written)
}

pub fn requirement_acceptance_show(
    store: &impl RecordStore,
    id: &RecordId,
    version: Option<u32>,
) -> Result<RequirementAcceptanceData, WorkError> {
    let view = work_feature::show_requirement_acceptance(store, id, version)?;
    Ok(RequirementAcceptanceData {
        schema: REQUIREMENT_ACCEPTANCE_RESPONSE,
        record: view.record,
        latest_version: view.latest_version,
        state: view.state,
        corrections: view.corrections,
    })
}

pub fn trace(
    store: &impl RecordStore,
    kind: RecordKind,
    id: &RecordId,
) -> Result<TraceData, WorkError> {
    let view = work_feature::trace(store, kind, id)?;
    Ok(TraceData {
        schema: TRACE_RESPONSE,
        root: view.root,
        sources: view.sources,
        discussions: view.discussions,
        tasks: view.tasks,
        requirement_acceptances: view.requirement_acceptances,
    })
}
