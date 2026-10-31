//! Persisted execution artifacts and their fixed data shapes.

pub mod acceptance;
pub mod attempt;
pub mod command;
pub mod correction;
pub mod deviation;
pub mod file_transaction;
pub mod host_file_report;
pub mod host_report;
pub mod index;
pub mod recovery;
pub mod request;
pub mod response;

#[cfg(test)]
mod tests {
    use super::{deviation, host_file_report, host_report, request, response};
    use serde::de::DeserializeOwned;
    use serde_json::Value;

    #[test]
    fn retired_policy_has_no_current_execution_model() {
        let request = serde_json::json!({"schema":"work-command-run-request","timeout_seconds":60,"work_isolation":{}});
        assert!(serde_json::from_value::<request::CommandRunRequest>(request).is_err());
    }

    fn example<T: DeserializeOwned>(registry: &Value, id: &str) {
        let value = registry["items"][id]["description"]["example"].clone();
        serde_json::from_value::<T>(value)
            .unwrap_or_else(|error| panic!("{id} example does not match its model: {error}"));
    }

    #[test]
    fn public_execution_examples_match_model_shapes() {
        let registry: Value = crate::contract::data::registry_value();
        example::<request::AttemptCloseRequest>(&registry, "work-attempt-close-request");
        example::<request::AttemptStartPrepareRequest>(
            &registry,
            "work-attempt-start-prepare-request",
        );
        example::<request::AttemptStartRequest>(&registry, "work-attempt-start-request");
        example::<request::CommandCorrectionRequest>(&registry, "work-command-correction-request");
        example::<request::CommandRunRequest>(&registry, "work-command-run-request");
        example::<host_report::HostExecutionReport>(&registry, "work-host-execution-report");
        example::<host_file_report::HostFileReport>(&registry, "work-host-file-report");
        example::<request::CorrectionCreateRequest>(&registry, "work-correction-create-request");
        example::<request::DeviationSemanticRequest>(
            &registry,
            "work-execution-deviation-semantic-request",
        );
        example::<request::ExecutionRecoveryPrepareRequest>(
            &registry,
            "work-execution-recovery-prepare-request",
        );
        example::<request::ExecutionRecoveryRequest>(&registry, "work-execution-recovery-request");
        example::<request::RecordFinishRequest>(&registry, "work-record-finish-request");
        example::<response::AttemptCloseResponse>(&registry, "work-attempt-close");
        example::<response::AttemptStartPrepareResponse>(&registry, "work-attempt-start-prepare");
        example::<response::AttemptStartRecoveryResponse>(&registry, "work-attempt-start-recovery");
        example::<response::AttemptStartResponse>(&registry, "work-attempt-start");
        example::<response::AttemptValidation>(&registry, "work-attempt-validation");
        example::<response::CommandCorrectionResponse>(&registry, "work-command-correction");
        example::<response::CommandPreview>(&registry, "work-command-preview");
        example::<response::CommandResult>(&registry, "work-command-result");
        example::<response::CorrectionCreateResponse>(&registry, "work-correction-create");
        example::<response::ExecutePreflight>(&registry, "work-execute-preflight");
        example::<response::WorktreeSnapshot>(&registry, "work-execute-worktree-snapshot");
        example::<response::ExecuteWorktree>(&registry, "work-execute-worktree");
        example::<response::DeviationPreview>(&registry, "work-execution-deviation-preview");
        example::<deviation::DeviationProposal>(&registry, "work-execution-deviation-proposal");
        example::<response::DeviationRecordResponse>(&registry, "work-execution-deviation-record");
        example::<response::ExecutionRecoveryPrepare>(&registry, "work-execution-recovery-prepare");
        example::<response::ExecutionRecoveryResponse>(&registry, "work-execution-recovery");
        example::<response::RecordBeginResponse>(&registry, "work-record-begin");
        example::<response::RecordFinishResponse>(&registry, "work-record-finish");
    }
}
