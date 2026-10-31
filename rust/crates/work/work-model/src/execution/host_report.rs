//! Host-reported execution facts; formal authorization is checked by the writer.

use serde::{Deserialize, Serialize};

use crate::schema::PublicSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostResultStatus {
    Exited,
    Rejected,
    Unknown,
    Partial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostResultOrigin {
    HostReported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostExecutionReport {
    pub schema: PublicSchema,
    pub task_id: String,
    pub attempt_id: String,
    pub record_id: String,
    pub task_collection_sha256: String,
    pub authorization_sha256: String,
    pub file_version: String,
    pub command_sha256: String,
    pub result_origin: HostResultOrigin,
    pub result_reference: String,
    pub result_sha256: String,
    pub status: HostResultStatus,
    pub exit_code: Option<i64>,
    pub output_complete: bool,
}

impl HostExecutionReport {
    pub fn valid_shape(&self) -> bool {
        self.schema == PublicSchema::WorkHostExecutionReport
            && [
                &self.task_id,
                &self.attempt_id,
                &self.record_id,
                &self.file_version,
                &self.result_reference,
            ]
            .iter()
            .all(|value| !value.trim().is_empty())
            && [
                &self.task_collection_sha256,
                &self.authorization_sha256,
                &self.command_sha256,
                &self.result_sha256,
            ]
            .iter()
            .all(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
            && match self.status {
                HostResultStatus::Exited => self.exit_code.is_some() && self.output_complete,
                HostResultStatus::Rejected
                | HostResultStatus::Unknown
                | HostResultStatus::Partial => self.exit_code.is_none(),
            }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> HostExecutionReport {
        HostExecutionReport {
            schema: PublicSchema::WorkHostExecutionReport,
            task_id: "TASK-001".into(),
            attempt_id: "ATTEMPT-001".into(),
            record_id: "CMD-001".into(),
            task_collection_sha256: "a".repeat(64),
            authorization_sha256: "b".repeat(64),
            file_version: "version-1".into(),
            command_sha256: "c".repeat(64),
            result_origin: HostResultOrigin::HostReported,
            result_reference: "host-command-1".into(),
            result_sha256: "d".repeat(64),
            status: HostResultStatus::Exited,
            exit_code: Some(0),
            output_complete: true,
        }
    }

    #[test]
    fn completed_host_result_requires_known_exit_and_complete_output() {
        assert!(report().valid_shape());
        let mut missing_exit = report();
        missing_exit.exit_code = None;
        assert!(!missing_exit.valid_shape());
        let mut incomplete = report();
        incomplete.output_complete = false;
        assert!(!incomplete.valid_shape());
    }

    #[test]
    fn unknown_and_partial_results_cannot_claim_an_exit() {
        for status in [
            HostResultStatus::Rejected,
            HostResultStatus::Unknown,
            HostResultStatus::Partial,
        ] {
            let mut value = report();
            value.status = status;
            assert!(!value.valid_shape());
            value.exit_code = None;
            assert!(value.valid_shape());
        }
    }

    #[test]
    fn malformed_identity_and_unexpected_fields_are_rejected() {
        let mut value = serde_json::to_value(report()).unwrap();
        value["authorization_sha256"] = "not-a-hash".into();
        assert!(
            !serde_json::from_value::<HostExecutionReport>(value.clone())
                .unwrap()
                .valid_shape()
        );
        value["unexpected"] = true.into();
        assert!(serde_json::from_value::<HostExecutionReport>(value).is_err());
    }
}
