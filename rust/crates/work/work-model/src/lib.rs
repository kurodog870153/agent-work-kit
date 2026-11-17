//! Work domain model: formal record types, public request/response shapes and
//! their basic invariants.
//!
//! The model owns data shapes only. Cross-field rules belong to
//! `work-operations`, business decisions to `work-feature`.

use serde::{Deserialize, Serialize};

mod description;
mod error;
mod example;
mod record;
mod request;
mod response;
mod task;

pub use description::{DOMAIN, contract_records};
pub use error::{Reason, WorkError};
pub use example::contract_examples;
pub use record::{
    Answer, CORRECTION_SCHEMA, CorrectionRecord, DISCUSSION_SCHEMA, Decision, DiscussionRecord,
    DiscussionStatus, Pending, Question, QuestionOption, RecordKind, RecordRef, SOURCE_SCHEMA,
    SourceContent, SourceRecord,
};
pub use request::{
    APPLY_REQUEST_SCHEMA, CorrectionApplyRequest, CorrectionDraftRequest, DISCUSSION_CREATE_SCHEMA,
    DISCUSSION_ROUND_SCHEMA, DRAFT_REQUEST_SCHEMA, DiscussionCreateRequest, DiscussionRoundRequest,
    NewDecision, NewPending, SOURCE_CREATE_SCHEMA, SourceCreateRequest,
};
pub use response::{
    CORRECTION_LIST_RESPONSE, CORRECTION_PREVIEW_RESPONSE, CorrectionListData,
    CorrectionPreviewData, DISCUSSION_RESPONSE, DiscussionData, Envelope, ErrorData,
    HISTORY_RESPONSE, HistoryData, PROGRESS_RESPONSE, ProgressData, RECOVER_RESPONSE,
    RecordSummary, RecoverData, SOURCE_RESPONSE, STATUS_RESPONSE, SourceData, StatusData,
    VersionSummary, WRITE_RESPONSE, WriteData,
};
pub use task::{
    Attempt, AttemptInput, AttemptResult, ConclusionInput, Criterion, CriterionState,
    CriterionStatus, EXECUTION_RECORD_SCHEMA, EXECUTION_RESPONSE, EXECUTION_SCHEMA, ExecutionData,
    ExecutionRecord, ExecutionRecordRequest, ExecutionTrace, REQUIREMENT_ACCEPTANCE_RECORD_SCHEMA,
    REQUIREMENT_ACCEPTANCE_RESPONSE, REQUIREMENT_ACCEPTANCE_SCHEMA, RequirementAcceptanceData,
    RequirementAcceptanceRecord, RequirementAcceptanceRecordRequest, RequirementConclusion,
    RequirementState, Revision, TASK_ACCEPTANCE_RECORD_SCHEMA, TASK_ACCEPTANCE_RESPONSE,
    TASK_ACCEPTANCE_SCHEMA, TASK_APPLY_SCHEMA, TASK_COMPLETE_SCHEMA, TASK_CREATE_SCHEMA,
    TASK_PREVIEW_RESPONSE, TASK_PREVIEW_SCHEMA, TASK_RESPONSE, TASK_REVISE_SCHEMA, TASK_SCHEMA,
    TRACE_RESPONSE, TaskAcceptanceData, TaskAcceptanceRecord, TaskAcceptanceRecordRequest,
    TaskApplyRequest, TaskCompleteRequest, TaskConclusion, TaskCreateRequest, TaskData, TaskDraft,
    TaskPreviewData, TaskPreviewRequest, TaskRecord, TaskReviseRequest, TaskStatus, TaskTrace,
    TraceData, Verdict,
};

/// Stored form of one published version: the record and the integrity digest
/// of its canonical text. The model fixes the shape only; the digest algorithm
/// is chosen, computed and verified by the infrastructure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordEnvelope<T> {
    pub digest: String,
    pub record: T,
}

/// Stable identity of a formal record.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RecordId(String);

impl RecordId {
    /// Accept a non-empty identifier of stable path-safe characters.
    pub fn parse(value: &str) -> Result<Self, ModelError> {
        if value.is_empty() {
            return Err(ModelError::EmptyId);
        }
        if !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        {
            return Err(ModelError::InvalidId(value.to_owned()));
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for RecordId {
    type Error = ModelError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<RecordId> for String {
    fn from(value: RecordId) -> Self {
        value.0
    }
}

/// Type-level failure of a formal record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelError {
    EmptyId,
    InvalidId(String),
    EmptyText(&'static str),
}

impl std::fmt::Display for ModelError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyId => formatter.write_str("record ID must not be empty"),
            Self::InvalidId(value) => write!(formatter, "invalid record ID: {value}"),
            Self::EmptyText(field) => write!(formatter, "required text is empty: {field}"),
        }
    }
}

impl std::error::Error for ModelError {}

/// Text that must be present when a record is created.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RequiredText(String);

impl RequiredText {
    pub fn parse(field: &'static str, value: &str) -> Result<Self, ModelError> {
        if value.trim().is_empty() {
            return Err(ModelError::EmptyText(field));
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Origin of a formal record: who produced it and where it came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Origin {
    pub source: OriginSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
}

/// Declared origin kinds. Acceptance conclusions must name their source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OriginSource {
    User,
    Host,
    Ai,
    External,
}

#[cfg(test)]
mod tests;
