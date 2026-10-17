//! Select only reports needed by actual structured validation; tools own their locations.
use crate::execution::ExecutionIssue;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use work_model::execution::command::capture::CommandCapturePolicy;
pub fn policy(task: &Value, command_id: &str) -> Result<CommandCapturePolicy, ExecutionIssue> {
    fn collect(value: &Value, id: &str, paths: &mut BTreeSet<String>) {
        match value["kind"].as_str() {
            Some("all" | "any") => {
                for child in value["conditions"].as_array().into_iter().flatten() {
                    collect(child, id, paths);
                }
            }
            Some("not") => collect(&value["condition"], id, paths),
            Some("report_sha256" | "json_report_equals") if value["command"] == id => {
                if let Some(path) = value["path"].as_str() {
                    paths.insert(path.into());
                }
            }
            _ => {}
        }
    }
    let mut policy = CommandCapturePolicy::default();
    for validation in task["validations"].as_array().into_iter().flatten() {
        if let Some(condition) = validation.get("verification") {
            crate::task::verification::validate(condition, &validation["command_ids"]).map_err(
                |_| ExecutionIssue {
                    reason_code: "command_capture_policy_invalid",
                    message: "Report evidence must come from current structured validation.",
                    details: json!({}),
                },
            )?;
            collect(condition, command_id, &mut policy.report_paths);
        }
    }
    if policy.report_paths.len() > 1024 {
        return Err(ExecutionIssue {
            reason_code: "command_capture_policy_invalid",
            message: "Too many report evidence sources.",
            details: json!({}),
        });
    }
    Ok(policy)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reports_follow_validation_ownership_and_commands_need_no_output_declaration() {
        assert!(
            policy(&json!({"validations":[]}), "CMD-001")
                .unwrap()
                .report_paths
                .is_empty()
        );
        let task = json!({"validations":[{"command_ids":["CMD-001","CMD-002"],"verification":{
            "kind":"all","conditions":[
                {"kind":"json_report_equals","command":"CMD-001","path":"target/report.json","pointer":"/ok","equals":true},
                {"kind":"report_sha256","command":"CMD-002","path":"other/report.txt","sha256":"a".repeat(64)}]}}]});
        assert_eq!(
            policy(&task, "CMD-001").unwrap().report_paths,
            BTreeSet::from(["target/report.json".into()])
        );
        assert_eq!(
            policy(&task, "CMD-002").unwrap().report_paths,
            BTreeSet::from(["other/report.txt".into()])
        );
    }
}
