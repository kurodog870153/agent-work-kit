//! Public failure shape shared by every Work layer above the model.

/// Stable reason codes of the public error contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    InvalidRequest,
    InvalidRecord,
    NotFound,
    AlreadyExists,
    VersionConflict,
    ApprovalMismatch,
    RuleViolation,
    Locked,
    RecoveryRequired,
    StorageFailure,
}

impl Reason {
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::InvalidRecord => "invalid_record",
            Self::NotFound => "not_found",
            Self::AlreadyExists => "already_exists",
            Self::VersionConflict => "version_conflict",
            Self::ApprovalMismatch => "approval_mismatch",
            Self::RuleViolation => "rule_violation",
            Self::Locked => "locked",
            Self::RecoveryRequired => "recovery_required",
            Self::StorageFailure => "storage_failure",
        }
    }
}

/// Failure carried from lower layers to the CLI error mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkError {
    pub reason: Reason,
    pub message: String,
}

impl WorkError {
    pub fn new(reason: Reason, message: impl Into<String>) -> Self {
        Self {
            reason,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for WorkError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.reason.code(), self.message)
    }
}

impl std::error::Error for WorkError {}

impl From<crate::ModelError> for WorkError {
    fn from(error: crate::ModelError) -> Self {
        Self::new(Reason::InvalidRequest, error.to_string())
    }
}
