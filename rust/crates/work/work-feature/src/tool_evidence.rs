//! Prepare tool details for Work's existing record-finish contract.

use serde_json::{Value, json};

use crate::error::{ExitCode, WorkError};

pub struct ToolCommandEvidence<'a> {
    pub tool: &'a str,
    pub operation: &'a str,
    pub policy_sources: &'a [&'a str],
    pub target: &'a str,
    pub actual_result: &'a str,
    pub diff_reference: &'a str,
    pub validation_reference: &'a str,
    pub limitations: &'a str,
    pub modified_files: &'a [&'a str],
    pub exit_code: i32,
}

/// Returns a normal Work request. Owned receipts and VAL proof remain the
/// responsibility of Work's existing record-finish and validation producers.
pub fn command_record_request(evidence: &ToolCommandEvidence<'_>) -> Result<Value, WorkError> {
    let fields = [
        evidence.tool,
        evidence.operation,
        evidence.target,
        evidence.actual_result,
        evidence.diff_reference,
        evidence.validation_reference,
        evidence.limitations,
    ];
    if fields.iter().any(|value| value.trim().is_empty())
        || evidence.policy_sources.is_empty()
        || evidence
            .policy_sources
            .iter()
            .any(|source| source.trim().is_empty())
        || evidence
            .modified_files
            .iter()
            .any(|path| path.trim().is_empty())
    {
        return Err(WorkError::new(
            ExitCode::Contract,
            "tool_evidence_incomplete",
            "Tool operation evidence requires source, target, result and validation references.",
            json!({}),
        ));
    }
    let summary = serde_json::to_string(&json!({
        "tool": evidence.tool,
        "operation": evidence.operation,
        "policy_sources": evidence.policy_sources,
        "target": evidence.target,
        "actual_result": evidence.actual_result,
        "diff_reference": evidence.diff_reference,
        "validation_reference": evidence.validation_reference,
        "limitations": evidence.limitations
    }))
    .expect("tool evidence serializes");
    let mut request = json!({
        "schema":"work-record-finish-request",
        "record":{"exit_code":evidence.exit_code,"result":summary}
    });
    if !evidence.modified_files.is_empty() {
        request["modified_files"] = json!(evidence.modified_files);
    }
    work_operations::execution::requests::validate_record_finish_request(&request).map_err(
        |issue| {
            WorkError::new(
                ExitCode::Contract,
                issue.reason_code,
                issue.message,
                issue.details,
            )
        },
    )?;
    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_evidence_uses_existing_attempt_record_request_without_new_artifact() {
        let evidence = ToolCommandEvidence {
            tool: "host-serena",
            operation: "find-references",
            policy_sources: &[
                "tooling/instructions.md#abc",
                "tooling/families/serena/instructions.md#def",
            ],
            target: "src/main.rs",
            actual_result: "Three references observed",
            diff_reference: "none",
            validation_reference: "VAL-001",
            limitations: "Index checked against current source",
            modified_files: &[],
            exit_code: 0,
        };
        let request = command_record_request(&evidence).unwrap();
        assert_eq!(request["schema"], "work-record-finish-request");
        assert!(request.get("modified_files").is_none());
        assert!(request["record"].get("id").is_none());
        let summary: Value =
            serde_json::from_str(request["record"]["result"].as_str().unwrap()).unwrap();
        assert_eq!(summary["policy_sources"].as_array().unwrap().len(), 2);
        assert_eq!(summary["target"], "src/main.rs");
        assert_eq!(summary["validation_reference"], "VAL-001");
        assert!(
            work_operations::execution::requests::validate_record_finish_request(&request).is_ok()
        );
    }

    #[test]
    fn missing_source_target_or_validation_cannot_be_recorded_as_complete_evidence() {
        let mut evidence = ToolCommandEvidence {
            tool: "host-tool",
            operation: "write",
            policy_sources: &["tooling/instructions.md#abc"],
            target: "src/main.rs",
            actual_result: "Changed file",
            diff_reference: "transaction:123",
            validation_reference: "VAL-001",
            limitations: "none",
            modified_files: &["src/main.rs"],
            exit_code: 0,
        };
        let request = command_record_request(&evidence).unwrap();
        assert_eq!(request["modified_files"], json!(["src/main.rs"]));
        evidence.policy_sources = &[];
        assert_eq!(
            command_record_request(&evidence).unwrap_err().reason_code,
            "tool_evidence_incomplete"
        );
        evidence.policy_sources = &["tooling/instructions.md#abc"];
        evidence.target = "";
        assert_eq!(
            command_record_request(&evidence).unwrap_err().reason_code,
            "tool_evidence_incomplete"
        );
        evidence.target = "src/main.rs";
        evidence.validation_reference = "";
        assert_eq!(
            command_record_request(&evidence).unwrap_err().reason_code,
            "tool_evidence_incomplete"
        );
    }
}
