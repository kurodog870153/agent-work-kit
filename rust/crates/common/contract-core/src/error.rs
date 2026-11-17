//! Errors produced by registry construction, queries and scaffolding.

use std::fmt;

/// Structural or query failure. Reason codes are stable public contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractError {
    reason: &'static str,
    message: String,
}

impl ContractError {
    pub fn new(reason: &'static str, message: impl Into<String>) -> Self {
        Self {
            reason,
            message: message.into(),
        }
    }

    pub fn reason(&self) -> &'static str {
        self.reason
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.reason, self.message)
    }
}

impl std::error::Error for ContractError {}

pub type ContractResult<T> = Result<T, ContractError>;
