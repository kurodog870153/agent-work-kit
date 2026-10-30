//! Bounded validation and command-reference mapping for Automated VAL conditions.

use std::collections::BTreeSet;

use serde_json::{Value, json};
use work_model::task::verification::VerificationCondition;

use super::TaskIssue;

fn invalid() -> TaskIssue {
    TaskIssue {
        reason_code: "validation_verification_invalid",
        message: "Verification requires bounded, structured conditions and exact command references.",
        details: json!({}),
    }
}

fn bounded(value: &Value) -> Result<(), TaskIssue> {
    fn visit(value: &Value, depth: usize, count: &mut usize) -> Result<(), TaskIssue> {
        *count += 1;
        if depth > 32 || *count > 1024 {
            return Err(invalid());
        }
        match value {
            Value::Object(values) => {
                for value in values.values() {
                    visit(value, depth + 1, count)?;
                }
            }
            Value::Array(values) => {
                for value in values {
                    visit(value, depth + 1, count)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    visit(value, 0, &mut 0)?;
    if serde_json::to_vec(value).map_err(|_| invalid())?.len() > 1024 * 1024 {
        return Err(invalid());
    }
    Ok(())
}

/// Map condition references only; JSON assertion payloads remain literal data.
pub fn map_commands(
    value: &Value,
    mut map: impl FnMut(&Value) -> Result<Value, TaskIssue>,
) -> Result<Value, TaskIssue> {
    fn visit(
        value: &Value,
        map: &mut impl FnMut(&Value) -> Result<Value, TaskIssue>,
    ) -> Result<Value, TaskIssue> {
        let mut result = value.clone();
        match value["kind"].as_str() {
            Some("all" | "any") => {
                result["conditions"] = Value::Array(
                    value["conditions"]
                        .as_array()
                        .ok_or_else(invalid)?
                        .iter()
                        .map(|child| visit(child, map))
                        .collect::<Result<_, _>>()?,
                );
            }
            Some("not") => result["condition"] = visit(&value["condition"], map)?,
            Some(_) if value.get("command").is_some() => {
                result["command"] = map(&value["command"])?;
            }
            _ => return Err(invalid()),
        }
        Ok(result)
    }
    bounded(value)?;
    visit(value, &mut map)
}

pub fn validate(value: &Value, commands: &Value) -> Result<(), TaskIssue> {
    bounded(value)?;
    let declared = commands
        .as_array()
        .ok_or_else(invalid)?
        .iter()
        .map(|v| v.as_str().filter(|s| !s.is_empty()).ok_or_else(invalid))
        .collect::<Result<BTreeSet<_>, _>>()?;
    if declared.is_empty() || commands.as_array().unwrap().len() != declared.len() {
        return Err(invalid());
    }
    let mut referenced = BTreeSet::new();
    let mapped = map_commands(value, |command| {
        let command = command
            .as_str()
            .filter(|s| declared.contains(s))
            .ok_or_else(invalid)?;
        referenced.insert(command.to_owned());
        Ok(json!(command))
    })?;
    let condition: VerificationCondition = serde_json::from_value(mapped).map_err(|_| invalid())?;
    fn path(path: &str) -> bool {
        crate::derivation::identity::runtime_relative_path(path)
    }
    fn pointer(pointer: &str) -> bool {
        if !pointer.is_empty() && !pointer.starts_with('/') {
            return false;
        }
        let mut bytes = pointer.bytes();
        while let Some(byte) = bytes.next() {
            if byte == b'~' && !matches!(bytes.next(), Some(b'0' | b'1')) {
                return false;
            }
        }
        pointer.len() <= 4096
    }
    fn visit(condition: &VerificationCondition) -> Result<(), TaskIssue> {
        use VerificationCondition::*;
        match condition {
            All { conditions } | Any { conditions } => {
                if conditions.is_empty() {
                    return Err(invalid());
                }
                for condition in conditions {
                    visit(condition)?;
                }
            }
            Not { condition } => visit(condition)?,
            ExitCode { .. } => {}
            StdoutEquals { text, .. } | StderrEquals { text, .. } => {
                if text.len() > 64 * 1024 {
                    return Err(invalid());
                }
            }
            StdoutContains { text, .. } | StderrContains { text, .. } => {
                if text.is_empty() || text.len() > 64 * 1024 {
                    return Err(invalid());
                }
            }
            StdoutSha256 { sha256, .. } | StderrSha256 { sha256, .. } => {
                if !crate::protocol::valid_sha256(sha256) {
                    return Err(invalid());
                }
            }
            ReportSha256 {
                path: report,
                sha256,
                ..
            } => {
                if !path(report) || !crate::protocol::valid_sha256(sha256) {
                    return Err(invalid());
                }
            }
            JsonReportEquals {
                path: report,
                pointer: selector,
                equals,
                ..
            } => {
                if !path(report)
                    || !pointer(selector)
                    || serde_json::to_vec(equals).map_err(|_| invalid())?.len() > 64 * 1024
                {
                    return Err(invalid());
                }
            }
        }
        Ok(())
    }
    visit(&condition)?;
    if referenced
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>()
        != declared
    {
        return Err(invalid());
    }
    Ok(())
}

/// Semantic positions retain the same exact-reference and shape rules as formal IDs.
pub fn validate_positions(validation: &Value) -> Result<(), TaskIssue> {
    let Some(condition) = validation.get("verification").filter(|v| !v.is_null()) else {
        return Ok(());
    };
    if validation["kind"] != "automated" {
        return Err(invalid());
    }
    let position = |v: &Value| {
        v.as_u64()
            .filter(|v| *v > 0)
            .map(|v| json!(v.to_string()))
            .ok_or_else(invalid)
    };
    let mapped = map_commands(condition, position)?;
    let commands = validation["command_positions"]
        .as_array()
        .ok_or_else(invalid)?
        .iter()
        .map(position)
        .collect::<Result<Vec<_>, _>>()?;
    validate(&mapped, &json!(commands))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_conditions_validate_references_paths_limits_and_literal_payloads() {
        let condition = json!({"kind":"all","conditions":[
            {"kind":"exit_code","command":"CMD-001","equals":0},
            {"kind":"not","condition":{"kind":"stderr_contains","command":"CMD-001","text":"failed"}},
            {"kind":"json_report_equals","command":"CMD-002","path":"project/report.json",
                "pointer":"/success","equals":{"kind":"exit_code","command":"literal"}}]});
        validate(&condition, &json!(["CMD-001", "CMD-002"])).unwrap();
        let mapped = map_commands(&condition, |v| {
            Ok(json!(format!("mapped-{}", v.as_str().unwrap())))
        })
        .unwrap();
        assert_eq!(mapped["conditions"][2]["equals"]["command"], "literal");
        for invalid in [
            json!({"kind":"all","conditions":[]}),
            json!({"kind":"exit_code","command":"CMD-003","equals":0}),
            json!({"kind":"stdout_contains","command":"CMD-001","text":""}),
            json!({"kind":"report_sha256","command":"CMD-001","path":"../private","sha256":"a".repeat(64)}),
            json!({"kind":"json_report_equals","command":"CMD-001","path":"r.json","pointer":"/~2","equals":true}),
            json!({"kind":"exit_code","command":"CMD-001","equals":0,"passed":true}),
        ] {
            assert!(
                validate(&invalid, &json!(["CMD-001"])).is_err(),
                "{invalid}"
            );
        }
        assert!(validate(&condition, &json!(["CMD-001"])).is_err());
        let mut deep = json!({"kind":"exit_code","command":"CMD-001","equals":0});
        for _ in 0..33 {
            deep = json!({"kind":"not","condition":deep});
        }
        assert!(validate(&deep, &json!(["CMD-001"])).is_err());
        let positions = json!({"kind":"automated","command_positions":[1],"verification":{
            "kind":"exit_code","command":1,"equals":0}});
        validate_positions(&positions).unwrap();
        let mut wrong = positions;
        wrong["verification"]["command"] = json!(2);
        assert!(validate_positions(&wrong).is_err());
    }
}
