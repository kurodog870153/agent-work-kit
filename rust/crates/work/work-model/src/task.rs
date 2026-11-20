//! TASK, Execution/Attempt and the two Acceptance layers: record shapes, the
//! caller-constructible requests and the typed responses.

use serde::{Deserialize, Serialize};

use crate::{CorrectionRecord, Origin, RecordId, RecordRef};

pub const TASK_SCHEMA: &str = "work-task";
pub const EXECUTION_SCHEMA: &str = "work-execution";
pub const TASK_ACCEPTANCE_SCHEMA: &str = "work-task-acceptance";
pub const REQUIREMENT_ACCEPTANCE_SCHEMA: &str = "work-requirement-acceptance";

pub const TASK_CREATE_SCHEMA: &str = "work-task-create-request";
pub const TASK_REVISE_SCHEMA: &str = "work-task-revise-request";
pub const TASK_PREVIEW_SCHEMA: &str = "work-task-preview-request";
pub const TASK_APPLY_SCHEMA: &str = "work-task-apply-request";
pub const TASK_PREVIEW_RESPONSE: &str = "work-task-preview-response";
pub const TASK_COMPLETE_SCHEMA: &str = "work-task-complete-request";
pub const EXECUTION_RECORD_SCHEMA: &str = "work-execution-record-request";
pub const TASK_ACCEPTANCE_RECORD_SCHEMA: &str = "work-task-acceptance-record-request";
pub const REQUIREMENT_ACCEPTANCE_RECORD_SCHEMA: &str = "work-requirement-acceptance-record-request";

pub const TASK_RESPONSE: &str = "work-task-response";
pub const EXECUTION_RESPONSE: &str = "work-execution-response";
pub const TASK_ACCEPTANCE_RESPONSE: &str = "work-task-acceptance-response";
pub const REQUIREMENT_ACCEPTANCE_RESPONSE: &str = "work-requirement-acceptance-response";
pub const TRACE_RESPONSE: &str = "work-trace-response";

const fn yes() -> bool {
    true
}

/// One acceptance condition of a TASK. Only required criteria gate completion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Criterion {
    pub id: String,
    pub text: String,
    #[serde(default = "yes")]
    pub required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Open,
    Completed,
}

/// Why a TASK version exists and which acceptance it put back in question.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Revision {
    pub reason: String,
    /// Criteria whose earlier conclusions no longer count from this version on.
    pub affected_criteria: Vec<String>,
    /// Whether the Requirement Acceptance must be confirmed again.
    pub reconfirm_requirement: bool,
}

/// Snapshot of a TASK. Goal, requirements, constraints, decisions and criteria
/// are the core; everything else is recorded only when needed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskRecord {
    pub schema: String,
    pub id: RecordId,
    pub version: u32,
    pub title: String,
    pub goal: String,
    pub requirements: Vec<String>,
    #[serde(default)]
    pub constraints: Vec<String>,
    #[serde(default)]
    pub decisions: Vec<String>,
    pub criteria: Vec<Criterion>,
    #[serde(default)]
    pub discussion_ids: Vec<RecordId>,
    #[serde(default)]
    pub source_ids: Vec<RecordId>,
    #[serde(default)]
    pub notes: Vec<String>,
    pub status: TaskStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<Revision>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptResult {
    Succeeded,
    Failed,
    Interrupted,
}

/// What the caller reports for one attempt; the store adds the TASK version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptInput {
    pub id: String,
    pub result: AttemptResult,
    #[serde(default)]
    pub changes: Vec<String>,
    #[serde(default)]
    pub anomalies: Vec<String>,
    #[serde(default)]
    pub verification: Vec<String>,
    #[serde(default)]
    pub evidence: Vec<String>,
}

/// One execution attempt; retries are further attempts of the same Execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attempt {
    pub id: String,
    pub task_version: u32,
    pub result: AttemptResult,
    #[serde(default)]
    pub changes: Vec<String>,
    #[serde(default)]
    pub anomalies: Vec<String>,
    #[serde(default)]
    pub verification: Vec<String>,
    #[serde(default)]
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionRecord {
    pub schema: String,
    pub id: RecordId,
    pub version: u32,
    pub task_id: RecordId,
    pub attempts: Vec<Attempt>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Passed,
    Failed,
}

/// Conclusion about one criterion as the caller reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConclusionInput {
    pub criterion_id: String,
    pub verdict: Verdict,
    pub evidence: String,
    pub origin: Origin,
}

/// Stored conclusion; `task_version` is the TASK version it was judged on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskConclusion {
    pub criterion_id: String,
    pub task_version: u32,
    pub verdict: Verdict,
    pub evidence: String,
    pub origin: Origin,
}

/// All conclusions of one TASK, oldest first; later versions only append.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskAcceptanceRecord {
    pub schema: String,
    pub id: RecordId,
    pub version: u32,
    pub task_id: RecordId,
    pub conclusions: Vec<TaskConclusion>,
}

/// Overall conclusion on the requirement a Discussion holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequirementConclusion {
    pub discussion_version: u32,
    pub verdict: Verdict,
    pub evidence: String,
    pub origin: Origin,
    /// TASK versions the conclusion was judged against.
    #[serde(default)]
    pub task_refs: Vec<RecordRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequirementAcceptanceRecord {
    pub schema: String,
    pub id: RecordId,
    pub version: u32,
    pub discussion_id: RecordId,
    pub conclusions: Vec<RequirementConclusion>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskCreateRequest {
    pub schema: String,
    pub id: RecordId,
    pub title: String,
    pub goal: String,
    pub requirements: Vec<String>,
    #[serde(default)]
    pub constraints: Vec<String>,
    #[serde(default)]
    pub decisions: Vec<String>,
    pub criteria: Vec<Criterion>,
    #[serde(default)]
    pub discussion_ids: Vec<RecordId>,
    #[serde(default)]
    pub source_ids: Vec<RecordId>,
    #[serde(default)]
    pub notes: Vec<String>,
}

/// Revise an existing TASK. Omitted fields keep their value; a given list
/// replaces the whole list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskReviseRequest {
    pub schema: String,
    pub id: RecordId,
    pub expected_version: u32,
    pub reason: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub goal: Option<String>,
    #[serde(default)]
    pub requirements: Option<Vec<String>>,
    #[serde(default)]
    pub constraints: Option<Vec<String>>,
    #[serde(default)]
    pub decisions: Option<Vec<String>>,
    #[serde(default)]
    pub criteria: Option<Vec<Criterion>>,
    #[serde(default)]
    pub notes: Option<Vec<String>>,
    #[serde(default)]
    pub link_discussions: Vec<RecordId>,
    #[serde(default)]
    pub link_sources: Vec<RecordId>,
    /// Unchanged criteria whose earlier conclusions must still be redone.
    #[serde(default)]
    pub reconfirm_criteria: Vec<String>,
    #[serde(default)]
    pub reconfirm_requirement: bool,
}

/// A proposed creation or revision, never directly published.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", content = "request", rename_all = "snake_case")]
pub enum TaskDraft {
    Create(TaskCreateRequest),
    Revise(TaskReviseRequest),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskPreviewRequest {
    pub schema: String,
    pub draft: TaskDraft,
}

/// Approval binds the exact reviewed candidate and every dependency version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskApplyRequest {
    pub schema: String,
    pub draft: TaskDraft,
    pub approved_candidate: String,
    pub reviewed_dependencies: Vec<RecordRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskPreviewData {
    pub schema: &'static str,
    pub candidate: String,
    pub reviewed_dependencies: Vec<RecordRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskCompleteRequest {
    pub schema: String,
    pub id: RecordId,
    pub expected_version: u32,
}

/// Create an Execution (`expected_version` absent) or add an attempt to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionRecordRequest {
    pub schema: String,
    pub id: RecordId,
    pub task_id: RecordId,
    #[serde(default)]
    pub expected_version: Option<u32>,
    pub attempt: AttemptInput,
}

/// Create the TASK's acceptance (`expected_version` absent) or append to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskAcceptanceRecordRequest {
    pub schema: String,
    pub id: RecordId,
    pub task_id: RecordId,
    pub task_version: u32,
    #[serde(default)]
    pub expected_version: Option<u32>,
    pub conclusions: Vec<ConclusionInput>,
}

/// Create the requirement's acceptance (`expected_version` absent) or append.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequirementAcceptanceRecordRequest {
    pub schema: String,
    pub id: RecordId,
    pub discussion_id: RecordId,
    #[serde(default)]
    pub expected_version: Option<u32>,
    pub verdict: Verdict,
    pub evidence: String,
    pub origin: Origin,
    #[serde(default)]
    pub task_refs: Vec<RecordRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CriterionStatus {
    Passed,
    Failed,
    /// No conclusion was ever recorded.
    Missing,
    /// Only conclusions older than the latest revision of the criterion exist.
    NeedsReconfirm,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CriterionState {
    pub criterion_id: String,
    pub required: bool,
    pub status: CriterionStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskData {
    pub schema: &'static str,
    pub record: TaskRecord,
    pub latest_version: u32,
    pub criteria: Vec<CriterionState>,
    pub completable: bool,
    pub executions: Vec<RecordId>,
    pub acceptance: Option<RecordId>,
    pub corrections: Vec<CorrectionRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExecutionData {
    pub schema: &'static str,
    pub record: ExecutionRecord,
    pub latest_version: u32,
    pub corrections: Vec<CorrectionRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskAcceptanceData {
    pub schema: &'static str,
    pub record: TaskAcceptanceRecord,
    pub latest_version: u32,
    pub criteria: Vec<CriterionState>,
    pub corrections: Vec<CorrectionRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RequirementState {
    /// Verdict of the newest conclusion, `None` when none was recorded.
    pub verdict: Option<Verdict>,
    /// A TASK revision asked for reconfirmation after that conclusion.
    pub needs_reconfirm: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RequirementAcceptanceData {
    pub schema: &'static str,
    pub record: RequirementAcceptanceRecord,
    pub latest_version: u32,
    pub state: RequirementState,
    pub corrections: Vec<CorrectionRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExecutionTrace {
    pub id: RecordId,
    pub attempts: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskTrace {
    pub id: RecordId,
    pub version: u32,
    pub status: TaskStatus,
    pub discussions: Vec<RecordId>,
    pub sources: Vec<RecordId>,
    pub executions: Vec<ExecutionTrace>,
    pub acceptance: Option<RecordId>,
    pub criteria: Vec<CriterionState>,
}

/// Source → Discussion → TASK → Execution → Acceptance links around one record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TraceData {
    pub schema: &'static str,
    pub root: RecordRef,
    pub sources: Vec<RecordId>,
    pub discussions: Vec<RecordId>,
    pub tasks: Vec<TaskTrace>,
    pub requirement_acceptances: Vec<RecordId>,
}
