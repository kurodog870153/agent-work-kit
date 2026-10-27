//! Verified acceptance progress is stored separately from immutable Task criteria.
use super::attempt::ValidationOutcome;
use serde::{Deserialize, Serialize};

pub const MAX_USER_CONFIRMATION_BYTES: usize = 1024 * 1024;

/// Verification origins are not interchangeable; only the executor assigns record provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationEvidenceSource {
    Procedure,
    AiReview,
    UserConfirmation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceArtifact {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ReviewReference {
    Artifact {
        path: String,
        sha256: String,
        detail: String,
    },
    Command {
        record_id: String,
        finished_sha256: String,
        detail: String,
    },
    Change {
        transaction_identity: String,
        path: String,
        before_sha256: Option<String>,
        after_sha256: Option<String>,
        detail: String,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiReviewEvidence {
    pub reviewer: String,
    pub rationale: String,
    pub references: Vec<ReviewReference>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserConfirmationOrigin {
    UserMessage,
    UserDocument,
}

/// An actual supplied user statement, scoped to one exact reserved validation and version.
/// This records provenance; it does not claim cryptographic authentication of a same-user peer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserConfirmationDocument {
    pub source: UserConfirmationOrigin,
    pub source_reference: String,
    pub confirmer: String,
    pub task_id: String,
    pub attempt_id: String,
    pub validation_id: String,
    pub record_id: String,
    pub file_version: String,
    pub outcome: ValidationOutcome,
    pub statement: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserConfirmationEvidence {
    pub reference: EvidenceArtifact,
    pub document: UserConfirmationDocument,
    pub raw: Vec<u8>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationContext {
    pub task_id: String,
    pub attempt_id: String,
    pub validation_id: String,
    pub record_id: String,
    pub source: ValidationEvidenceSource,
    pub file_version: String,
    pub confirmer: Option<String>,
    pub criteria: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcceptanceStatus {
    Pending,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptanceEvidence {
    pub source: ValidationEvidenceSource,
    pub file_version: String,
    pub task_id: String,
    pub attempt_id: String,
    /// Actual VAL owner when this Attempt explicitly carries verified evidence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_attempt_id: Option<String>,
    pub validation_id: String,
    pub record_id: String,
    pub outcome: ValidationOutcome,
    pub evidence: String,
    pub task_item_sha256: String,
    pub task_instructions_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptanceProgress {
    pub id: String,
    pub status: AcceptanceStatus,
    pub evidence: Vec<AcceptanceEvidence>,
}
