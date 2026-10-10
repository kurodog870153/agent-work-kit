//! Machine-verifiable Automated VAL conditions. Command references are formal IDs,
//! candidate keys, or semantic positions according to the containing contract.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum VerificationCondition<C = String> {
    All {
        conditions: Vec<Self>,
    },
    Any {
        conditions: Vec<Self>,
    },
    Not {
        condition: Box<Self>,
    },
    ExitCode {
        command: C,
        equals: i64,
    },
    StdoutEquals {
        command: C,
        text: String,
    },
    StdoutContains {
        command: C,
        text: String,
    },
    StderrEquals {
        command: C,
        text: String,
    },
    StderrContains {
        command: C,
        text: String,
    },
    StdoutSha256 {
        command: C,
        sha256: String,
    },
    StderrSha256 {
        command: C,
        sha256: String,
    },
    ReportSha256 {
        command: C,
        path: String,
        sha256: String,
    },
    JsonReportEquals {
        command: C,
        path: String,
        pointer: String,
        equals: serde_json::Value,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn verification_contract_is_strict_and_preserves_legacy_validations() {
        let legacy = json!({"id":"VAL-001","kind":"automated",
            "command_ids":["CMD-001"],"pass_condition":"Exit zero."});
        let parsed: super::super::item::TaskValidation =
            serde_json::from_value(legacy.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), legacy);
        let condition = json!({"kind":"not","condition":{"kind":"exit_code",
            "command":"CMD-001","equals":1}});
        let parsed: VerificationCondition = serde_json::from_value(condition.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), condition);
        let mut unknown = condition;
        unknown["condition"]["passed"] = json!(true);
        assert!(serde_json::from_value::<VerificationCondition>(unknown).is_err());
        assert!(
            serde_json::from_value::<VerificationCondition>(json!({
            "kind":"ai_passed","command":"CMD-001"}))
            .is_err()
        );
    }
}
