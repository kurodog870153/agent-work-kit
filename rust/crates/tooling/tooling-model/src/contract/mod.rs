use host_environment::ExecutionEnvironment;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::common::CliStatus;

pub fn request_example(name: &str) -> Option<&'static str> {
    match name {
        "policy-load-first" => Some(include_str!(
            "../../resources/contract/examples/policy-load-first.json"
        )),
        "policy-load-reload" => Some(include_str!(
            "../../resources/contract/examples/policy-load-reload.json"
        )),
        "operation-evaluate" => Some(include_str!(
            "../../resources/contract/examples/operation-evaluate.json"
        )),
        "policy-load-invalid-missing-previous" => Some(include_str!(
            "../../resources/contract/examples/policy-load-invalid-missing-previous.json"
        )),
        "operation-evaluate-invalid-missing-target" => Some(include_str!(
            "../../resources/contract/examples/operation-evaluate-invalid-missing-target.json"
        )),
        _ => None,
    }
}

pub fn response_example(name: &str) -> Option<&'static str> {
    match name {
        "policy-discover-response" => Some(include_str!(
            "../../resources/contract/examples/policy-discover-response.json"
        )),
        "policy-load-response" => Some(include_str!(
            "../../resources/contract/examples/policy-load-response.json"
        )),
        "operation-evaluate-response" => Some(include_str!(
            "../../resources/contract/examples/operation-evaluate-response.json"
        )),
        "error-response" => Some(include_str!(
            "../../resources/contract/examples/error-response.json"
        )),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CliSchema {
    #[serde(rename = "tooling-cli-result")]
    ToolingCliResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CliEnvelope {
    pub schema: CliSchema,
    pub status: CliStatus,
    pub reason_code: String,
    pub message: String,
    pub data: Value,
    pub execution: ExecutionEnvironment,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cli_envelope_requires_current_schema_status_and_execution() {
        let current = json!({"schema":"tooling-cli-result","status":"success",
            "reason_code":"ok","message":"Done","data":{},
            "execution":{"host":{"mode":"inherited","os":"windows",
                "architecture":"x86_64","rust_workspace_version":"2.1.2"}}});
        let model: CliEnvelope = serde_json::from_value(current.clone()).unwrap();
        assert_eq!(serde_json::to_value(model).unwrap(), current);

        let mut invalid = current.clone();
        invalid["status"] = json!("unknown");
        assert!(serde_json::from_value::<CliEnvelope>(invalid).is_err());
        let mut invalid = current.clone();
        invalid["schema"] = json!("work-cli-result");
        assert!(serde_json::from_value::<CliEnvelope>(invalid).is_err());
        let mut invalid = current;
        let old_version = invalid["execution"]["host"]
            .as_object_mut()
            .unwrap()
            .remove("rust_workspace_version")
            .unwrap();
        invalid["execution"]["host"]["work_version"] = old_version;
        assert!(serde_json::from_value::<CliEnvelope>(invalid).is_err());
    }

    #[test]
    fn embedded_response_examples_follow_the_public_envelope() {
        for name in [
            "policy-discover-response",
            "policy-load-response",
            "operation-evaluate-response",
            "error-response",
        ] {
            let raw = response_example(name).unwrap();
            let _: CliEnvelope = serde_json::from_str(raw).unwrap();
        }
    }
}
