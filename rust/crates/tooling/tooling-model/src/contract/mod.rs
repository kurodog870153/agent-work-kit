use host_environment::ExecutionEnvironment;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::common::CliStatus;

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
}
