//! Host-reported project file effects bound to a Work-owned prepared file set.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::schema::PublicSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostFileStatus {
    CompletedAtomic,
    Partial,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostFileReport {
    pub schema: PublicSchema,
    pub task_id: String,
    pub attempt_id: String,
    pub transaction_identity: String,
    pub approval_sha256: String,
    pub result_origin: super::host_report::HostResultOrigin,
    pub result_reference: String,
    pub result_sha256: String,
    pub status: HostFileStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostFilePathEvidence {
    pub before_sha256: Option<String>,
    pub after_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostFileEvidence {
    pub schema: String,
    pub task_id: String,
    pub attempt_id: String,
    pub transaction_identity: String,
    pub status: HostFileStatus,
    pub source_reference: String,
    pub atomicity_statement: String,
    pub paths: BTreeMap<String, HostFilePathEvidence>,
}

fn hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

impl HostFileReport {
    pub fn valid_shape(&self) -> bool {
        self.schema == PublicSchema::WorkHostFileReport
            && !self.task_id.trim().is_empty()
            && !self.attempt_id.trim().is_empty()
            && !self.transaction_identity.trim().is_empty()
            && !self.result_reference.trim().is_empty()
            && hash(&self.approval_sha256)
            && hash(&self.result_sha256)
    }
}

impl HostFileEvidence {
    pub fn valid_shape(&self) -> bool {
        self.schema == "work-host-file-evidence"
            && !self.task_id.trim().is_empty()
            && !self.attempt_id.trim().is_empty()
            && !self.transaction_identity.trim().is_empty()
            && !self.source_reference.trim().is_empty()
            && (self.status != HostFileStatus::CompletedAtomic
                || (!self.atomicity_statement.trim().is_empty() && !self.paths.is_empty()))
            && self.paths.values().all(|path| {
                path.before_sha256.as_deref().is_none_or(hash)
                    && path.after_sha256.as_deref().is_none_or(hash)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_file_result_requires_explicit_source_and_complete_hashes() {
        let report: HostFileReport = serde_json::from_value(serde_json::json!({
            "schema":"work-host-file-report","task_id":"TASK-001","attempt_id":"ATTEMPT-001",
            "transaction_identity":"tx-1","approval_sha256":"a".repeat(64),
            "result_origin":"host_reported","result_reference":"host-evidence/file.json",
            "result_sha256":"b".repeat(64),"status":"completed_atomic"
        }))
        .unwrap();
        assert!(report.valid_shape());
        let mut evidence: HostFileEvidence = serde_json::from_value(serde_json::json!({
            "schema":"work-host-file-evidence","task_id":"TASK-001","attempt_id":"ATTEMPT-001",
            "transaction_identity":"tx-1","status":"completed_atomic",
            "source_reference":"Host edit call 1","atomicity_statement":"The complete set was committed atomically",
            "paths":{"source.txt":{"before_sha256":"a".repeat(64),"after_sha256":"b".repeat(64)}}
        })).unwrap();
        assert!(evidence.valid_shape());
        evidence.atomicity_statement.clear();
        assert!(!evidence.valid_shape());
    }
}
