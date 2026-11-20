//! Formal record shapes. Each stored version is an immutable snapshot.

use serde::{Deserialize, Serialize};

use crate::{Origin, RecordId};

pub const SOURCE_SCHEMA: &str = "work-source";
pub const DISCUSSION_SCHEMA: &str = "work-discussion";
pub const CORRECTION_SCHEMA: &str = "work-correction";

/// Kind of a stored formal record; the code is also the storage directory name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordKind {
    Source,
    Discussion,
    Correction,
    Task,
    Execution,
    TaskAcceptance,
    RequirementAcceptance,
}

impl RecordKind {
    pub const ALL: [Self; 7] = [
        Self::Source,
        Self::Discussion,
        Self::Correction,
        Self::Task,
        Self::Execution,
        Self::TaskAcceptance,
        Self::RequirementAcceptance,
    ];

    pub fn parse(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.code() == code)
    }

    pub const fn code(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Discussion => "discussion",
            Self::Correction => "correction",
            Self::Task => "task",
            Self::Execution => "execution",
            Self::TaskAcceptance => "task_acceptance",
            Self::RequirementAcceptance => "requirement_acceptance",
        }
    }
}

/// Reference to one published version of a record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordRef {
    pub kind: RecordKind,
    pub id: RecordId,
    pub version: u32,
}

/// The exact original bytes of a Source, without normalization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SourceContent {
    Text { text: String },
    Binary { bytes: Vec<u8> },
}

impl SourceContent {
    pub fn bytes(&self) -> &[u8] {
        match self {
            Self::Text { text } => text.as_bytes(),
            Self::Binary { bytes } => bytes,
        }
    }
}

/// Original evidence with the byte length and digest of its raw content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRecord {
    pub schema: String,
    pub id: RecordId,
    pub version: u32,
    pub title: String,
    pub locator: String,
    pub content: SourceContent,
    pub content_length: u64,
    pub content_digest: String,
    pub origin: Origin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscussionStatus {
    Open,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestionOption {
    pub id: String,
    pub text: String,
}

/// One question asked of the user, with numbered options and a recommendation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Question {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub options: Vec<QuestionOption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommendation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Answer to exactly one question: a chosen option, free text, or both.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Answer {
    pub question_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub option_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    pub id: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pending {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub resolved: bool,
}

/// Snapshot of a Discussion after one round.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscussionRecord {
    pub schema: String,
    pub id: RecordId,
    pub version: u32,
    pub original_request: String,
    pub status: DiscussionStatus,
    #[serde(default)]
    pub source_ids: Vec<RecordId>,
    #[serde(default)]
    pub questions: Vec<Question>,
    #[serde(default)]
    pub answers: Vec<Answer>,
    #[serde(default)]
    pub decisions: Vec<Decision>,
    #[serde(default)]
    pub pending: Vec<Pending>,
}

/// Additive fix for a published record; the corrected version is never touched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorrectionRecord {
    pub schema: String,
    pub id: RecordId,
    pub version: u32,
    pub target: RecordRef,
    pub statement: String,
    pub reason: String,
    pub origin: Origin,
}
