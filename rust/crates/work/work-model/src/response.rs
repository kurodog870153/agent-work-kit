//! Typed response shapes of the Work CLI.

use serde::Serialize;

use crate::{
    CorrectionRecord, Decision, DiscussionRecord, DiscussionStatus, Pending, Question, RecordId,
    RecordKind, SourceRecord,
};

/// Public CLI success and failure envelope.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Envelope<T> {
    Ok { data: T },
    Error { error: ErrorData },
}

/// Public error fields; the CLI owns conversion from domain and contract errors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ErrorData {
    pub reason: String,
    pub message: String,
}

pub const WRITE_RESPONSE: &str = "work-write-response";
pub const SOURCE_RESPONSE: &str = "work-source-response";
pub const DISCUSSION_RESPONSE: &str = "work-discussion-response";
pub const HISTORY_RESPONSE: &str = "work-history-response";
pub const PROGRESS_RESPONSE: &str = "work-progress-response";
pub const STATUS_RESPONSE: &str = "work-status-response";
pub const CORRECTION_PREVIEW_RESPONSE: &str = "work-correction-preview-response";
pub const CORRECTION_LIST_RESPONSE: &str = "work-correction-list-response";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WriteData {
    pub schema: &'static str,
    pub kind: RecordKind,
    pub id: RecordId,
    pub version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceData {
    pub schema: &'static str,
    pub record: SourceRecord,
    pub referenced_by: Vec<RecordId>,
    pub corrections: Vec<CorrectionRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiscussionData {
    pub schema: &'static str,
    pub record: DiscussionRecord,
    pub latest_version: u32,
    pub corrections: Vec<CorrectionRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VersionSummary {
    pub version: u32,
    pub status: DiscussionStatus,
    pub questions: usize,
    pub answers: usize,
    pub decisions: usize,
    pub open_pending: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HistoryData {
    pub schema: &'static str,
    pub id: RecordId,
    pub versions: Vec<VersionSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProgressData {
    pub schema: &'static str,
    pub id: RecordId,
    pub version: u32,
    pub status: DiscussionStatus,
    pub questions: usize,
    pub answered: usize,
    pub outstanding_question: Option<Question>,
    pub open_pending: Vec<Pending>,
    pub decisions: Vec<Decision>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecordSummary {
    pub kind: RecordKind,
    pub id: RecordId,
    pub latest_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatusData {
    pub schema: &'static str,
    pub records: Vec<RecordSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CorrectionPreviewData {
    pub schema: &'static str,
    pub candidate: String,
    pub reviewed_target_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CorrectionListData {
    pub schema: &'static str,
    pub corrections: Vec<CorrectionRecord>,
}
