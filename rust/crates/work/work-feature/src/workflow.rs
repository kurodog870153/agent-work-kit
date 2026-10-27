//! Pre-execution workflow state assembly.

use serde_json::{Value, json};
use work_operations::derivation::fingerprint;
use work_operations::operation::{operation_effect, routing_identity};
use work_operations::routing::RoutingRequest;
use work_operations::workflow::{
    WorkflowDecision, decide_execution, decide_pre_execution, lifecycle_for_status,
    mode_for_action, next_action_guidance,
};

use crate::error::{ExitCode, WorkError};

pub trait WorkflowRoutingRepository {
    fn route(&mut self, request: &RoutingRequest<'_>) -> Result<Value, WorkError>;
}

pub struct WorkflowSnapshot {
    pub artifacts: Value,
    pub source: Option<Value>,
    pub discussion: Option<Value>,
    pub task: Option<Value>,
    pub index: Option<Value>,
    pub latest_attempts: Value,
}

pub struct OperationContextRequest<'a> {
    pub command: &'a str,
    pub operation: &'a str,
    pub delegated_role: Option<&'a str>,
    pub artifacts: &'a Value,
    pub project_root: &'a str,
    pub approval_sha256: Option<&'a str>,
    pub transaction_workspace: Option<&'a str>,
}

pub fn build_operation_context(
    routing: &mut impl WorkflowRoutingRepository,
    input: &OperationContextRequest<'_>,
) -> Result<(Value, Value), WorkError> {
    let command = input.command;
    let operation = input.operation;
    let delegated_role = input.delegated_role;
    let artifacts = input.artifacts;
    let project_root = input.project_root;
    let approval_sha256 = input.approval_sha256;
    let transaction_workspace = input.transaction_workspace;
    let effect = operation_effect(command, operation).ok_or_else(|| {
        WorkError::new(
            ExitCode::WorkflowState,
            "operation_effect_unclassified",
            "The command operation has no explicit side-effect classification.",
            json!({"command":command,"operation":operation}),
        )
    })?;
    let identity =
        routing_identity(command, operation, delegated_role).expect("classified operation");
    let state_sha256 = fingerprint::structured(&json!({
        "command":command,"operation":operation,"mode":identity.mode,
        "events":identity.formal_events,"role":identity.role,"artifacts":artifacts,
        "project_root":project_root,
    }))
    .expect("operation state serializes");
    let request = RoutingRequest {
        status: &format!("cli_{operation}"),
        operation: identity.next_action,
        confirmation: false,
        mode: Some(identity.mode),
        artifact_lifecycle: "verified_cli_input",
        formal_events: &identity.formal_events,
        role: &identity.role,
        authorization_state: Some(effect.authorization_state()),
        verified_state_sha256: &state_sha256,
    };
    let selection = routing.route(&request)?;
    if selection["routing_status"] != "VALID" {
        return Err(WorkError::new(
            ExitCode::WorkflowState,
            "operation_routing_review_required",
            "The operation context cannot be created from the verified routing state.",
            json!({"routing_reasons":selection["routing_reasons"]}),
        ));
    }
    let mut envelope = json!({
        "schema":"work-operation-envelope","workflow":identity.mode,"operation":operation,
        "verified_state_sha256":state_sha256,"selection_sha256":selection["selection_sha256"],
        "artifacts":artifacts,"approval_sha256":approval_sha256,
        "authorization_state":effect.authorization_state(),
        "side_effect_boundary":effect.side_effect_boundary(),
        "transaction_workspace":transaction_workspace,"role":identity.role,
        "expected_result_contract":"work-operation-result",
    });
    envelope["context_sha256"] =
        json!(fingerprint::structured(&envelope).expect("operation context serializes"));
    let _: work_model::operation::OperationEnvelope =
        serde_json::from_value(envelope.clone()).expect("bound operation matches its model");
    Ok((envelope, selection))
}

pub fn validate_operation_context(
    envelope: &Value,
    selection: &Value,
    artifacts: &Value,
) -> Result<(), WorkError> {
    let mut context = envelope.clone();
    context
        .as_object_mut()
        .ok_or_else(|| {
            WorkError::new(
                ExitCode::Contract,
                "invalid_operation_envelope",
                "The operation envelope must be an object.",
                json!({}),
            )
        })?
        .remove("context_sha256");
    if envelope["context_sha256"] != fingerprint::structured(&context).expect("context serializes")
    {
        return Err(WorkError::new(
            ExitCode::WorkflowState,
            "operation_context_identity_mismatch",
            "The operation envelope identity does not match its bound fields.",
            json!({}),
        ));
    }
    if envelope["selection_sha256"] != selection["selection_sha256"] {
        return Err(WorkError::new(
            ExitCode::WorkflowState,
            "operation_selection_drift",
            "The operation selection changed before the worker started.",
            json!({}),
        ));
    }
    if envelope["artifacts"] != *artifacts {
        return Err(WorkError::new(
            ExitCode::ArtifactIntegrity,
            "operation_artifact_drift",
            "A bound operation artifact changed before the worker started.",
            json!({}),
        ));
    }
    Ok(())
}

/// Host-supplied Tooling review for a concrete operation, separate from formal
/// Work instruction selection and ordinary External Skill slots.
pub struct RuntimeToolPolicy<'a> {
    pub source_identities: &'a [&'a str],
    pub valid: bool,
}

pub fn validate_tool_policy_gate(
    envelope: &Value,
    selection: &Value,
    artifacts: &Value,
    required: bool,
    policy: Option<&RuntimeToolPolicy<'_>>,
) -> Result<(), WorkError> {
    validate_operation_context(envelope, selection, artifacts)?;
    if !required {
        return Ok(());
    }
    let review = policy.ok_or_else(|| {
        WorkError::new(
            ExitCode::WorkflowState,
            "tool_policy_unavailable",
            "Required Runtime Tool Policy is unavailable.",
            json!({}),
        )
    })?;
    if !review.valid || review.source_identities.is_empty() {
        return Err(WorkError::new(
            ExitCode::WorkflowState,
            "tool_policy_invalid",
            "Required Runtime Tool Policy did not pass validation.",
            json!({}),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolEffect {
    ReadOnly,
    ProtectedWrite,
    Unknown,
}

pub fn validate_task_tool_operation(
    envelope: &Value,
    selection: &Value,
    artifacts: &Value,
    policy: &RuntimeToolPolicy<'_>,
    effect: ToolEffect,
) -> Result<(), WorkError> {
    validate_tool_policy_gate(envelope, selection, artifacts, true, Some(policy))?;
    if envelope["workflow"] != "task" {
        return Err(WorkError::new(
            ExitCode::WorkflowState,
            "tool_operation_wrong_mode",
            "Task tool policy requires a Task operation envelope.",
            json!({}),
        ));
    }
    if effect != ToolEffect::ReadOnly {
        return Err(WorkError::new(
            ExitCode::WorkflowState,
            "task_tool_write_forbidden",
            "Task tool analysis cannot change protected project state.",
            json!({}),
        ));
    }
    Ok(())
}

/// The Host supplies already validated formal TASK, Execution index and Attempt
/// documents. This gate checks their live identity and the concrete write scope.
pub struct ExecuteToolWriteDocuments<'a> {
    pub task_validation: &'a Value,
    pub index: &'a Value,
    pub attempt: &'a Value,
}

pub fn validate_execute_tool_write(
    envelope: &Value,
    selection: &Value,
    artifacts: &Value,
    policy: &RuntimeToolPolicy<'_>,
    documents: ExecuteToolWriteDocuments<'_>,
    host_authorized: bool,
    requested_files: &[&str],
) -> Result<(), WorkError> {
    let ExecuteToolWriteDocuments {
        task_validation,
        index,
        attempt,
    } = documents;
    validate_tool_policy_gate(envelope, selection, artifacts, true, Some(policy))?;
    if envelope["workflow"] != "execute"
        || envelope["authorization_state"] != "authorized"
        || !host_authorized
    {
        return Err(WorkError::new(
            ExitCode::WorkflowState,
            "execute_tool_not_authorized",
            "Execute tool writes require an authorized Execute operation envelope.",
            json!({}),
        ));
    }
    let task_id = attempt["task_id"].as_str().unwrap_or("");
    let attempt_id = attempt["attempt_id"].as_str().unwrap_or("");
    let row = index["tasks"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["id"] == task_id));
    if task_id.is_empty()
        || attempt_id.is_empty()
        || !task_validation["task_ids"]
            .as_array()
            .is_some_and(|ids| ids.iter().any(|id| id == task_id))
        || attempt["status"] != "in_progress"
        || row
            .is_none_or(|row| row["status"] != "in_progress" || row["latest_attempt"] != attempt_id)
        || index["lock"]["task_id"] != task_id
        || index["lock"]["attempt_id"] != attempt_id
        || task_validation["task_item_sha256"][task_id] != attempt["task_item_sha256"]
        || task_validation["task_collection_sha256"] != attempt["task_collection_sha256"]
        || task_validation["task_index_sha256"] != attempt["task_index_sha256"]
    {
        return Err(WorkError::new(
            ExitCode::WorkflowState,
            "execute_tool_attempt_invalid",
            "Execute tool write is not bound to a current validated TASK and Attempt.",
            json!({}),
        ));
    }
    let allowed = attempt["authorization"]["modifiable_files"]
        .as_array()
        .ok_or_else(|| {
            WorkError::new(
                ExitCode::WorkflowState,
                "execute_tool_scope_invalid",
                "Attempt authorization has no valid modifiable-file scope.",
                json!({}),
            )
        })?;
    if requested_files.is_empty()
        || requested_files
            .iter()
            .any(|path| !allowed.iter().any(|value| value == path))
    {
        return Err(WorkError::new(
            ExitCode::WorkflowState,
            "execute_tool_scope_exceeded",
            "Tool write exceeds the Attempt's authorized files.",
            json!({}),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubagentToolAccess {
    Direct,
    HostProxy,
    Unavailable,
}

pub fn validate_subagent_tool_gate(
    envelope: &Value,
    selection: &Value,
    artifacts: &Value,
    policy: &RuntimeToolPolicy<'_>,
    expected_role: &str,
    host_authorized: bool,
    access: SubagentToolAccess,
) -> Result<SubagentToolAccess, WorkError> {
    validate_tool_policy_gate(envelope, selection, artifacts, true, Some(policy))?;
    if expected_role.is_empty()
        || expected_role == "main"
        || envelope["role"] != expected_role
        || !host_authorized
    {
        return Err(WorkError::new(
            ExitCode::WorkflowState,
            "subagent_tool_scope_invalid",
            "Subagent tool use exceeds its role, envelope or Host authorization.",
            json!({}),
        ));
    }
    if access == SubagentToolAccess::Unavailable {
        return Err(WorkError::new(
            ExitCode::WorkflowState,
            "subagent_tool_unavailable",
            "Neither direct nor Host-mediated access is available.",
            json!({}),
        ));
    }
    Ok(access)
}

pub fn pre_execution_state(
    routing: &mut impl WorkflowRoutingRepository,
    requirement_id: &str,
    artifacts: &Value,
    source_validation: Option<&Value>,
    discussion: Option<&Value>,
    task_validation: Option<&Value>,
    execution_index_exists: bool,
) -> Result<Option<Value>, WorkError> {
    pre_execution_state_with_events(
        routing,
        WorkflowRoutingContext {
            requirement_id,
            artifacts,
            formal_events: &[],
        },
        source_validation,
        discussion,
        task_validation,
        execution_index_exists,
    )
}

pub struct WorkflowRoutingContext<'a> {
    pub requirement_id: &'a str,
    pub artifacts: &'a Value,
    pub formal_events: &'a [&'a str],
}

pub fn pre_execution_state_with_events(
    routing: &mut impl WorkflowRoutingRepository,
    context: WorkflowRoutingContext<'_>,
    source_validation: Option<&Value>,
    discussion: Option<&Value>,
    task_validation: Option<&Value>,
    execution_index_exists: bool,
) -> Result<Option<Value>, WorkError> {
    let decision = decide_pre_execution(
        source_validation,
        discussion,
        task_validation,
        execution_index_exists,
    )
    .map_err(|issue| {
        WorkError::new(
            ExitCode::Contract,
            issue.reason_code,
            issue.message,
            json!({}),
        )
    })?;
    let Some(decision) = decision else {
        return Ok(None);
    };
    render_state(
        routing,
        context.requirement_id,
        context.artifacts,
        decision,
        context.formal_events,
    )
    .map(Some)
}

pub fn execution_state(
    routing: &mut impl WorkflowRoutingRepository,
    requirement_id: &str,
    artifacts: &Value,
    index: &Value,
    latest_attempts: &Value,
) -> Result<Value, WorkError> {
    execution_state_with_events(
        routing,
        WorkflowRoutingContext {
            requirement_id,
            artifacts,
            formal_events: &[],
        },
        index,
        latest_attempts,
    )
}

pub fn execution_state_with_events(
    routing: &mut impl WorkflowRoutingRepository,
    context: WorkflowRoutingContext<'_>,
    index: &Value,
    latest_attempts: &Value,
) -> Result<Value, WorkError> {
    render_state(
        routing,
        context.requirement_id,
        context.artifacts,
        decide_execution(index, latest_attempts),
        context.formal_events,
    )
}

fn render_state(
    routing: &mut impl WorkflowRoutingRepository,
    requirement_id: &str,
    artifacts: &Value,
    decision: WorkflowDecision,
    formal_events: &[&str],
) -> Result<Value, WorkError> {
    let target = artifacts[decision.target_artifact]
        .as_str()
        .ok_or_else(|| {
            WorkError::new(
                ExitCode::Contract,
                "workflow_artifact_path_missing",
                "The selected workflow artifact path is missing.",
                json!({"artifact":decision.target_artifact}),
            )
        })?;
    let state_sha256 = fingerprint::structured(&json!({
        "requirement_id":requirement_id,"status":decision.status,"next_action":decision.next_action,
        "target":target,"artifacts":artifacts,
    }))
    .expect("workflow state serializes");
    let routing_request = RoutingRequest {
        status: &decision.status,
        operation: &decision.next_action,
        confirmation: decision.requires_user_confirmation,
        mode: Some(mode_for_action(&decision.status, &decision.next_action)),
        artifact_lifecycle: lifecycle_for_status(&decision.status),
        formal_events,
        role: "main",
        authorization_state: Some(if decision.requires_user_confirmation {
            "confirmation_required"
        } else {
            "read_only"
        }),
        verified_state_sha256: &state_sha256,
    };
    let routed = routing.route(&routing_request)?;
    let guidance = next_action_guidance(&decision.next_action, requirement_id, artifacts);
    let mut state = json!({
        "schema":"work-workflow-state","requirement_id":requirement_id,
        "status":decision.status,"next_action":decision.next_action,"target":target,
        "requires_user_confirmation":decision.requires_user_confirmation,
        "required_checks":decision.required_checks,"artifacts":artifacts,"details":decision.details,
    });
    for additions in [&routed, &guidance] {
        let object = additions
            .as_object()
            .expect("workflow additions are objects");
        for (key, value) in object {
            state[key] = value.clone();
        }
    }
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixedRouting;

    impl WorkflowRoutingRepository for FixedRouting {
        fn route(&mut self, _request: &RoutingRequest<'_>) -> Result<Value, WorkError> {
            Ok(json!({"routing_status":"VALID","selection_sha256":"0".repeat(64)}))
        }
    }

    struct ReviewRouting;

    impl WorkflowRoutingRepository for ReviewRouting {
        fn route(&mut self, _request: &RoutingRequest<'_>) -> Result<Value, WorkError> {
            Ok(json!({"routing_status":"REVIEW_REQUIRED","routing_reasons":["source_drift"]}))
        }
    }

    #[test]
    fn operation_envelope_binds_read_only_context_and_requires_selection_sha() {
        let artifacts = json!({});
        let (envelope, selection) = build_operation_context(
            &mut FixedRouting,
            &OperationContextRequest {
                command: "task",
                operation: "preview",
                delegated_role: None,
                artifacts: &artifacts,
                project_root: "/project",
                approval_sha256: None,
                transaction_workspace: None,
            },
        )
        .unwrap();
        assert_eq!(envelope["schema"], "work-operation-envelope");
        assert_eq!(envelope["side_effect_boundary"], "read_only");
        validate_operation_context(&envelope, &selection, &artifacts).unwrap();
        assert_eq!(
            validate_operation_context(&envelope, &selection, &json!({"request":"changed"}))
                .unwrap_err()
                .reason_code,
            "operation_artifact_drift"
        );
        let mut changed_role = envelope.clone();
        changed_role["role"] = json!("worker");
        assert_eq!(
            validate_operation_context(&changed_role, &selection, &artifacts)
                .unwrap_err()
                .reason_code,
            "operation_context_identity_mismatch"
        );
        let mut missing = envelope;
        missing.as_object_mut().unwrap().remove("selection_sha256");
        assert_eq!(
            validate_operation_context(&missing, &selection, &artifacts)
                .unwrap_err()
                .reason_code,
            "operation_context_identity_mismatch"
        );
        let read = build_operation_context(
            &mut FixedRouting,
            &OperationContextRequest {
                command: "discussion",
                operation: "read",
                delegated_role: None,
                artifacts: &artifacts,
                project_root: "/project",
                approval_sha256: None,
                transaction_workspace: None,
            },
        )
        .unwrap()
        .0;
        let save = build_operation_context(
            &mut FixedRouting,
            &OperationContextRequest {
                command: "discussion",
                operation: "update",
                delegated_role: None,
                artifacts: &artifacts,
                project_root: "/project",
                approval_sha256: Some(&"a".repeat(64)),
                transaction_workspace: None,
            },
        )
        .unwrap()
        .0;
        assert_ne!(read["context_sha256"], save["context_sha256"]);
        assert_eq!(read["authorization_state"], "read_only");
        assert_eq!(save["authorization_state"], "authorized");
        assert_eq!(save["side_effect_boundary"], "authorized_atomic_write");
        let command_run = build_operation_context(
            &mut FixedRouting,
            &OperationContextRequest {
                command: "execute",
                operation: "command-run",
                delegated_role: None,
                artifacts: &artifacts,
                project_root: "/project",
                approval_sha256: Some(&"a".repeat(64)),
                transaction_workspace: None,
            },
        )
        .unwrap()
        .0;
        assert_eq!(
            command_run["side_effect_boundary"],
            "authorized_external_effect"
        );
        let prepare = build_operation_context(
            &mut FixedRouting,
            &OperationContextRequest {
                command: "execute",
                operation: "command-prepare",
                delegated_role: None,
                artifacts: &artifacts,
                project_root: "/project",
                approval_sha256: None,
                transaction_workspace: None,
            },
        )
        .unwrap()
        .0;
        let recovery = build_operation_context(
            &mut FixedRouting,
            &OperationContextRequest {
                command: "execute",
                operation: "recover",
                delegated_role: None,
                artifacts: &artifacts,
                project_root: "/project",
                approval_sha256: None,
                transaction_workspace: None,
            },
        )
        .unwrap()
        .0;
        assert_ne!(prepare["context_sha256"], recovery["context_sha256"]);
        assert_eq!(
            work_operations::operation::routing_identity("execute", "recover", None)
                .unwrap()
                .formal_events,
            ["recovery"]
        );
    }

    #[test]
    fn runtime_tool_policy_gate_is_separate_from_formal_work_selection() {
        let artifacts = json!({});
        let (envelope, selection) = build_operation_context(
            &mut FixedRouting,
            &OperationContextRequest {
                command: "task",
                operation: "preview",
                delegated_role: None,
                artifacts: &artifacts,
                project_root: "/project",
                approval_sha256: None,
                transaction_workspace: None,
            },
        )
        .unwrap();
        assert!(validate_tool_policy_gate(&envelope, &selection, &artifacts, false, None).is_ok());
        assert_eq!(
            validate_tool_policy_gate(&envelope, &selection, &artifacts, true, None)
                .unwrap_err()
                .reason_code,
            "tool_policy_unavailable"
        );
        let policy = RuntimeToolPolicy {
            source_identities: &["tooling/instructions.md"],
            valid: true,
        };
        assert!(
            validate_tool_policy_gate(&envelope, &selection, &artifacts, true, Some(&policy))
                .is_ok()
        );
        let invalid = RuntimeToolPolicy {
            source_identities: &["tooling/instructions.md"],
            valid: false,
        };
        assert_eq!(
            validate_tool_policy_gate(&envelope, &selection, &artifacts, true, Some(&invalid))
                .unwrap_err()
                .reason_code,
            "tool_policy_invalid"
        );
        assert_eq!(
            validate_tool_policy_gate(
                &envelope,
                &selection,
                &json!({"changed":true}),
                true,
                Some(&policy)
            )
            .unwrap_err()
            .reason_code,
            "operation_artifact_drift"
        );
        assert_eq!(selection["selection_sha256"], "0".repeat(64));
    }

    #[test]
    fn task_tool_gate_allows_analysis_but_blocks_protected_writes() {
        let artifacts = json!({});
        let (envelope, selection) = build_operation_context(
            &mut FixedRouting,
            &OperationContextRequest {
                command: "task",
                operation: "preview",
                delegated_role: None,
                artifacts: &artifacts,
                project_root: "/project",
                approval_sha256: None,
                transaction_workspace: None,
            },
        )
        .unwrap();
        let policy = RuntimeToolPolicy {
            source_identities: &["tooling/instructions.md"],
            valid: true,
        };
        assert!(
            validate_task_tool_operation(
                &envelope,
                &selection,
                &artifacts,
                &policy,
                ToolEffect::ReadOnly
            )
            .is_ok()
        );
        for effect in [ToolEffect::ProtectedWrite, ToolEffect::Unknown] {
            assert_eq!(
                validate_task_tool_operation(&envelope, &selection, &artifacts, &policy, effect)
                    .unwrap_err()
                    .reason_code,
                "task_tool_write_forbidden"
            );
        }
        let mut invalid = envelope;
        invalid["selection_sha256"] = json!("changed");
        assert_eq!(
            validate_task_tool_operation(
                &invalid,
                &selection,
                &artifacts,
                &policy,
                ToolEffect::ReadOnly
            )
            .unwrap_err()
            .reason_code,
            "operation_context_identity_mismatch"
        );
        assert_eq!(
            build_operation_context(
                &mut ReviewRouting,
                &OperationContextRequest {
                    command: "task",
                    operation: "preview",
                    delegated_role: None,
                    artifacts: &artifacts,
                    project_root: "/project",
                    approval_sha256: None,
                    transaction_workspace: None,
                },
            )
            .err()
            .unwrap()
            .reason_code,
            "operation_routing_review_required"
        );
    }

    #[test]
    fn execute_tool_write_requires_current_attempt_and_file_scope() {
        let artifacts = json!({"task_id":"TASK-001","attempt_id":"ATTEMPT-001"});
        let (envelope, selection) = build_operation_context(
            &mut FixedRouting,
            &OperationContextRequest {
                command: "execute",
                operation: "command-run",
                delegated_role: None,
                artifacts: &artifacts,
                project_root: "/project",
                approval_sha256: Some(&"a".repeat(64)),
                transaction_workspace: None,
            },
        )
        .unwrap();
        let policy = RuntimeToolPolicy {
            source_identities: &["tooling/instructions.md"],
            valid: true,
        };
        let validation = json!({
            "task_ids":["TASK-001"],
            "task_item_sha256":{"TASK-001":"c".repeat(64)},
            "task_collection_sha256":"a".repeat(64),
            "task_index_sha256":"b".repeat(64)
        });
        let index = json!({
            "tasks":[{"id":"TASK-001","status":"in_progress","latest_attempt":"ATTEMPT-001"}],
            "lock":{"task_id":"TASK-001","attempt_id":"ATTEMPT-001"}
        });
        let attempt = json!({
            "task_id":"TASK-001","attempt_id":"ATTEMPT-001","status":"in_progress",
            "task_item_sha256":"c".repeat(64),
            "task_collection_sha256":"a".repeat(64),
            "task_index_sha256":"b".repeat(64),
            "authorization":{"modifiable_files":["src/main.rs"]}
        });
        assert!(
            validate_execute_tool_write(
                &envelope,
                &selection,
                &artifacts,
                &policy,
                ExecuteToolWriteDocuments {
                    task_validation: &validation,
                    index: &index,
                    attempt: &attempt
                },
                true,
                &["src/main.rs"]
            )
            .is_ok()
        );
        assert_eq!(
            validate_execute_tool_write(
                &envelope,
                &selection,
                &artifacts,
                &policy,
                ExecuteToolWriteDocuments {
                    task_validation: &validation,
                    index: &index,
                    attempt: &attempt
                },
                false,
                &["src/main.rs"]
            )
            .unwrap_err()
            .reason_code,
            "execute_tool_not_authorized"
        );
        assert_eq!(
            validate_execute_tool_write(
                &envelope,
                &selection,
                &artifacts,
                &policy,
                ExecuteToolWriteDocuments {
                    task_validation: &validation,
                    index: &index,
                    attempt: &attempt
                },
                true,
                &["src/other.rs"]
            )
            .unwrap_err()
            .reason_code,
            "execute_tool_scope_exceeded"
        );
        let mut stopped = attempt.clone();
        stopped["status"] = json!("stopped");
        assert_eq!(
            validate_execute_tool_write(
                &envelope,
                &selection,
                &artifacts,
                &policy,
                ExecuteToolWriteDocuments {
                    task_validation: &validation,
                    index: &index,
                    attempt: &stopped
                },
                true,
                &["src/main.rs"]
            )
            .unwrap_err()
            .reason_code,
            "execute_tool_attempt_invalid"
        );
        let mut drifted = validation.clone();
        drifted["task_item_sha256"]["TASK-001"] = json!("d".repeat(64));
        assert_eq!(
            validate_execute_tool_write(
                &envelope,
                &selection,
                &artifacts,
                &policy,
                ExecuteToolWriteDocuments {
                    task_validation: &drifted,
                    index: &index,
                    attempt: &attempt
                },
                true,
                &["src/main.rs"]
            )
            .unwrap_err()
            .reason_code,
            "execute_tool_attempt_invalid"
        );
        let invalid_policy = RuntimeToolPolicy {
            source_identities: &["tooling/instructions.md"],
            valid: false,
        };
        assert_eq!(
            validate_execute_tool_write(
                &envelope,
                &selection,
                &artifacts,
                &invalid_policy,
                ExecuteToolWriteDocuments {
                    task_validation: &validation,
                    index: &index,
                    attempt: &attempt
                },
                true,
                &["src/main.rs"]
            )
            .unwrap_err()
            .reason_code,
            "tool_policy_invalid"
        );
    }

    #[test]
    fn subagent_tool_gate_keeps_role_and_host_authorization() {
        let artifacts = json!({"target":"src/main.rs"});
        let (envelope, selection) = build_operation_context(
            &mut FixedRouting,
            &OperationContextRequest {
                command: "delegation",
                operation: "validate",
                delegated_role: Some("task-skill"),
                artifacts: &artifacts,
                project_root: "/project",
                approval_sha256: None,
                transaction_workspace: None,
            },
        )
        .unwrap();
        let policy = RuntimeToolPolicy {
            source_identities: &["tooling/instructions.md"],
            valid: true,
        };
        for access in [SubagentToolAccess::Direct, SubagentToolAccess::HostProxy] {
            assert_eq!(
                validate_subagent_tool_gate(
                    &envelope,
                    &selection,
                    &artifacts,
                    &policy,
                    "task-skill",
                    true,
                    access
                )
                .unwrap(),
                access
            );
        }
        assert_eq!(
            validate_subagent_tool_gate(
                &envelope,
                &selection,
                &artifacts,
                &policy,
                "task-skill",
                true,
                SubagentToolAccess::Unavailable
            )
            .unwrap_err()
            .reason_code,
            "subagent_tool_unavailable"
        );
        assert_eq!(
            validate_subagent_tool_gate(
                &envelope,
                &selection,
                &artifacts,
                &policy,
                "execute",
                true,
                SubagentToolAccess::Direct
            )
            .unwrap_err()
            .reason_code,
            "subagent_tool_scope_invalid"
        );
        assert_eq!(
            validate_subagent_tool_gate(
                &envelope,
                &selection,
                &artifacts,
                &policy,
                "task-skill",
                false,
                SubagentToolAccess::HostProxy
            )
            .unwrap_err()
            .reason_code,
            "subagent_tool_scope_invalid"
        );
        assert_eq!(
            validate_subagent_tool_gate(
                &envelope,
                &selection,
                &json!({"target":"src/other.rs"}),
                &policy,
                "task-skill",
                true,
                SubagentToolAccess::Direct
            )
            .unwrap_err()
            .reason_code,
            "operation_artifact_drift"
        );
    }
}
