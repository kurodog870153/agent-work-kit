//! Deterministic bounds for the current Attempt authorization; semantic review remains AI-owned.
use crate::execution::ExecutionIssue;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use work_model::execution::attempt::{ScopeAdjustment, ScopeAuthorization};

fn fail(reason: &'static str) -> ExecutionIssue {
    ExecutionIssue {
        reason_code: reason,
        message: "The operation is outside the recorded authorization boundary.",
        details: json!({}),
    }
}
fn path(value: &str) -> bool {
    crate::derivation::identity::runtime_relative_path(value)
        && value.split('/').next() != Some(".git")
}
fn unique(values: &[String]) -> bool {
    !values.is_empty()
        && values.iter().all(|s| !s.trim().is_empty())
        && values.iter().collect::<BTreeSet<_>>().len() == values.len()
}
pub fn validate(value: &Value, task_id: &str) -> Result<ScopeAuthorization, ExecutionIssue> {
    let scope: ScopeAuthorization = serde_json::from_value(value.clone())
        .map_err(|_| fail("execution_authorization_boundary_invalid"))?;
    if scope.revoked_reason != work_model::common::Nullable::Null {
        return Err(fail("execution_authorization_revoked"));
    }
    if scope.project_root.trim().is_empty()
        || scope
            .requirement_id
            .parse::<work_model::identifiers::RequirementId>()
            .is_err()
        || scope.goal.trim().is_empty()
        || scope.risk_boundary.trim().is_empty()
        || !unique(&scope.module_paths)
        || !unique(&scope.validation_scope)
        || !unique(&scope.task_ids)
        || scope.modification_types.is_empty()
        || scope
            .modification_types
            .iter()
            .collect::<BTreeSet<_>>()
            .len()
            != scope.modification_types.len()
        || scope.adjustments.iter().collect::<BTreeSet<_>>().len() != scope.adjustments.len()
        || scope.module_paths.iter().any(|p| p != "." && !path(p))
        || scope.task_ids.iter().any(|id| {
            !id.strip_prefix("TASK-").is_some_and(|n| {
                n.len() == 3 && n.bytes().all(|b| b.is_ascii_digit()) && n != "000"
            })
        })
        || (!task_id.is_empty() && !scope.task_ids.iter().any(|id| id == task_id))
    {
        return Err(fail("execution_authorization_boundary_invalid"));
    }
    if let Some(files) = &scope.exact_files {
        let ids: BTreeSet<_> = files
            .iter()
            .map(|p| crate::canonical::portable_path_identity(p))
            .collect();
        if ids.len() != files.len() || files.iter().any(|p| !path(p) || !in_modules(&scope, p)) {
            return Err(fail("execution_authorization_boundary_invalid"));
        }
    }
    Ok(scope)
}
pub fn from_authorization(
    authorization: &Value,
) -> Result<Option<ScopeAuthorization>, ExecutionIssue> {
    authorization
        .get("scope")
        .map(|value| validate(value, authorization["task_id"].as_str().unwrap_or("")))
        .transpose()
}
pub fn from_attempt(attempt: &Value) -> Result<Option<ScopeAuthorization>, ExecutionIssue> {
    from_authorization(&attempt["authorization"])
}
fn in_modules(scope: &ScopeAuthorization, value: &str) -> bool {
    let key = crate::canonical::portable_path_identity(value);
    scope.module_paths.iter().any(|module| {
        let module = crate::canonical::portable_path_identity(module);
        module == "." || key == module || key.starts_with(&format!("{module}/"))
    })
}
pub fn contains_path(scope: &ScopeAuthorization, value: &str) -> bool {
    path(value)
        && in_modules(scope, value)
        && scope.exact_files.as_ref().is_none_or(|files| {
            files.iter().any(|p| {
                crate::canonical::portable_path_identity(p)
                    == crate::canonical::portable_path_identity(value)
            })
        })
}
pub fn check_project(
    authorization: &Value,
    project_root: &str,
    requirement: &str,
) -> Result<(), ExecutionIssue> {
    if let Some(scope) = from_authorization(authorization)?
        && (scope.project_root != project_root || scope.requirement_id != requirement)
    {
        return Err(fail("execution_authorization_project_mismatch"));
    }
    Ok(())
}
pub fn within_impact(value: &Value) -> Result<bool, ExecutionIssue> {
    let impact: work_model::execution::deviation::DeviationImpact =
        serde_json::from_value(value.clone())
            .map_err(|_| fail("execution_authorization_impact_required"))?;
    if impact.summary.trim().is_empty() {
        return Err(fail("execution_authorization_impact_required"));
    }
    Ok(!impact.requirement_changed
        && !impact.scope_changed.unwrap_or(false)
        && !impact.acceptance_criteria_changed
        && !impact.deliverables_changed
        && !impact.safety_boundary_changed
        && !impact.external_side_effect_boundary_changed)
}
pub fn permits_adjustment(
    attempt: &Value,
    action: &Value,
    impact: &Value,
    files: &Value,
) -> Result<bool, ExecutionIssue> {
    let Some(scope) = from_attempt(attempt)? else {
        return Ok(false);
    };
    let kind = match action["kind"].as_str() {
        Some("replace_command" | "add_command") => ScopeAdjustment::CommandAdjustment,
        Some("add_validation") => ScopeAdjustment::ValidationAdjustment,
        Some("add_files") => ScopeAdjustment::AdditionalFiles,
        _ => return Ok(false),
    };
    if !scope.adjustments.contains(&kind) || !within_impact(impact)? {
        return Ok(false);
    }
    if action["kind"] == "add_files" {
        let types: Vec<work_model::execution::attempt::ScopeModification> =
            serde_json::from_value(action["modification_types"].clone())
                .map_err(|_| fail("execution_authorization_boundary_invalid"))?;
        if types.is_empty()
            || types.iter().any(|t| !scope.modification_types.contains(t))
            || files.as_array().is_none_or(Vec::is_empty)
        {
            return Ok(false);
        }
    }
    for field in ["replacement", "command"] {
        if let Some(directory) = action[field]["execution"]["working_directory"].as_str()
            && directory != "."
            && !path(directory)
        {
            return Ok(false);
        }
    }
    if let Some(rows) = files.as_array() {
        if !rows.is_empty()
            && (!scope
                .adjustments
                .contains(&ScopeAdjustment::AdditionalFiles)
                || rows
                    .iter()
                    .any(|p| p.as_str().is_none_or(|p| !contains_path(&scope, p))))
        {
            return Ok(false);
        }
    } else if !files.is_null() {
        return Err(fail("execution_authorization_boundary_invalid"));
    }
    Ok(true)
}
pub fn known_retry(attempt: &Value, record_id: &str) -> Result<bool, ExecutionIssue> {
    let Some(scope) = from_attempt(attempt)? else {
        return Ok(false);
    };
    if !scope.adjustments.contains(&ScopeAdjustment::Retry) {
        return Ok(false);
    }
    let base = record_id.split('#').next().unwrap_or(record_id);
    let last = attempt["records"]
        .as_array()
        .into_iter()
        .flatten()
        .rev()
        .find(|r| {
            r["id"]
                .as_str()
                .is_some_and(|id| id.split('#').next() == Some(base))
        });
    Ok(last.is_some_and(|r| match r["kind"].as_str() {
        Some("command") => r["exit_code"].is_i64(),
        Some("validation") => matches!(r["outcome"].as_str(), Some("passed" | "failed")),
        _ => false,
    }))
}

#[cfg(test)]
pub(crate) fn example_scope() -> Value {
    json!({"project_root":"/project","requirement_id":"example","goal":"Deliver the approved feature",
        "module_paths":["src"],"modification_types":["create","modify","move"],
        "risk_boundary":"Local source changes; no installs or external effects",
        "validation_scope":["Project tests"],"task_ids":["TASK-001","TASK-002"],
        "adjustments":["command_adjustment","validation_adjustment","additional_files","retry"],
        "revoked_reason":null})
}
#[cfg(test)]
pub(crate) fn example_impact() -> Value {
    json!({"summary":"Narrow test selection within the approved validation scope",
        "requirement_changed":false,"scope_changed":false,"acceptance_criteria_changed":false,
        "deliverables_changed":false,"safety_boundary_changed":false,"external_side_effect_boundary_changed":false})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn one_boundary_covers_named_tasks_and_preserves_explicit_file_limits() {
        for id in ["TASK-001", "TASK-002"] {
            let authorization = json!({"task_id":id,"scope":example_scope()});
            let scope = from_authorization(&authorization).unwrap().unwrap();
            assert!(contains_path(&scope, "src/new.rs"));
            assert!(!contains_path(&scope, "src-other/new.rs"));
            assert!(!contains_path(&scope, "src/../outside"));
            check_project(&authorization, "/project", "example").unwrap();
            assert!(check_project(&authorization, "/other", "example").is_err());
        }
        assert!(validate(&example_scope(), "TASK-003").is_err());
        let mut scoped = example_scope();
        scoped["exact_files"] = json!(["src/allowed.rs"]);
        let scope = validate(&scoped, "TASK-001").unwrap();
        assert!(!contains_path(&scope, "src/new.rs"));
        assert!(contains_path(&scope, "src/allowed.rs"));
        scoped["revoked_reason"] = json!("User withdrew the approval");
        assert_eq!(
            validate(&scoped, "TASK-001").unwrap_err().reason_code,
            "execution_authorization_revoked"
        );
    }
    #[test]
    fn scoped_authorization_uses_current_validation_and_declared_modification_types() {
        let mut authorization =
            work_model::contract::data::registry().items["work-attempt-authorization"]
                .description
                .example
                .clone();
        authorization["scope"] = example_scope();
        authorization["modifiable_files"] = json!(["src/lib.rs"]);
        authorization["reapproval_conditions"] = json!([
            "scope_expansion",
            "source_or_worktree_drift",
            "recovery",
            "unknown_result"
        ]);
        let task = json!({"id":"TASK-001","files":[{"action":"modify","path":"src/lib.rs"}],
            "commands":[],"validations":[],"operations":[]});
        crate::execution::attempt::validate_authorization(&authorization).unwrap();
        crate::execution::validate_authorization_scope(&authorization, &task, &json!({})).unwrap();
        let parsed: work_model::execution::attempt::AttemptAuthorization =
            serde_json::from_value(authorization.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), authorization);
        authorization["scope"]["modification_types"] = json!(["create"]);
        assert!(
            crate::execution::validate_authorization_scope(&authorization, &task, &json!({}))
                .is_err()
        );
        authorization["scope"] = example_scope();
        authorization["reapproval_conditions"] = json!(["retry"]);
        assert!(crate::execution::attempt::validate_authorization(&authorization).is_err());
    }

    #[test]
    fn reviewed_adjustments_cannot_extend_risk_or_task_scope() {
        let attempt = json!({"authorization":{"task_id":"TASK-001","scope":example_scope()}});
        let action = json!({"kind":"add_command","command":{"id":"CMD-002","mode":"argv","argv":["tool","test"]}});
        assert!(
            permits_adjustment(&attempt, &action, &example_impact(), &json!(["src/new.rs"]))
                .unwrap()
        );
        assert!(
            !permits_adjustment(&attempt, &action, &example_impact(), &json!(["outside.rs"]))
                .unwrap()
        );
        for field in [
            "requirement_changed",
            "scope_changed",
            "acceptance_criteria_changed",
            "deliverables_changed",
            "safety_boundary_changed",
            "external_side_effect_boundary_changed",
        ] {
            let mut impact = example_impact();
            impact[field] = json!(true);
            assert!(
                !permits_adjustment(&attempt, &action, &impact, &json!([])).unwrap(),
                "{field}"
            );
        }
        assert!(permits_adjustment(&attempt, &action, &json!({}), &json!([])).is_err());
        let mut unknown = example_scope();
        unknown["sandbox"] = json!(true);
        assert!(validate(&unknown, "TASK-001").is_err());
        for field in [
            "project_root",
            "requirement_id",
            "goal",
            "module_paths",
            "modification_types",
            "risk_boundary",
            "validation_scope",
            "task_ids",
            "adjustments",
            "revoked_reason",
        ] {
            let mut missing = example_scope();
            missing.as_object_mut().unwrap().remove(field);
            assert!(validate(&missing, "TASK-001").is_err(), "{field}");
        }
    }
}
