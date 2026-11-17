//! Caller-constructible request shapes of the Work CLI.

use serde::{Deserialize, Serialize};

use crate::{Answer, Origin, Question, RecordId, RecordRef, SourceContent};

pub const SOURCE_CREATE_SCHEMA: &str = "work-source-create-request";
pub const DISCUSSION_CREATE_SCHEMA: &str = "work-discussion-create-request";
pub const DISCUSSION_ROUND_SCHEMA: &str = "work-discussion-round-request";
pub const DRAFT_REQUEST_SCHEMA: &str = "work-correction-draft-request";
pub const APPLY_REQUEST_SCHEMA: &str = "work-correction-apply-request";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceCreateRequest {
    pub schema: String,
    pub id: RecordId,
    pub title: String,
    pub locator: String,
    /// Omit content only when the CLI receives --from-file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<SourceContent>,
    pub origin: Origin,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscussionCreateRequest {
    pub schema: String,
    pub id: RecordId,
    pub original_request: String,
    #[serde(default)]
    pub source_ids: Vec<RecordId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewDecision {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub question_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewPending {
    pub id: String,
    pub text: String,
}

/// One round of an existing Discussion, applied against a reviewed version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscussionRoundRequest {
    pub schema: String,
    pub id: RecordId,
    pub expected_version: u32,
    #[serde(default)]
    pub answer: Option<Answer>,
    #[serde(default)]
    pub decisions: Vec<NewDecision>,
    #[serde(default)]
    pub add_pending: Vec<NewPending>,
    #[serde(default)]
    pub resolve_pending: Vec<String>,
    #[serde(default)]
    pub link_sources: Vec<RecordId>,
    #[serde(default)]
    pub ask: Option<Question>,
    #[serde(default)]
    pub close: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorrectionDraftRequest {
    pub schema: String,
    pub id: RecordId,
    pub target: RecordRef,
    pub statement: String,
    pub reason: String,
    pub origin: Origin,
}

/// Apply a reviewed correction candidate; binds approval to reviewed content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorrectionApplyRequest {
    pub schema: String,
    pub draft: CorrectionDraftRequest,
    pub approved_candidate: String,
    pub reviewed_target_version: u32,
}
