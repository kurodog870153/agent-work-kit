//! Whole-set file preparation and lifecycle decisions over explicit storage capabilities.
use super::ExecutionWriterContext;
use crate::error::{ExitCode, WorkError};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use work_model::execution::file_transaction::*;
use work_model::schema::PublicSchema;
use work_operations::derivation::{file_transaction as derive, fingerprint};

mod candidate;

pub fn effective_scope_collection(
    collection: &Value,
    accepted: &BTreeMap<String, Vec<String>>,
) -> Result<Value, WorkError> {
    candidate::effective_collection(collection, accepted)
}

pub fn error(reason: &str) -> WorkError {
    WorkError::new(
        ExitCode::ArtifactIntegrity,
        reason,
        "Project file publication is not verified. Preserve complete evidence and return to preparation or authorized recovery.",
        json!({"recovery_required":true}),
    )
}

pub trait FileTransactionRepository {
    fn input_paths(
        &self,
        context: &ExecutionWriterContext,
        task: &Value,
        _attempt: &str,
    ) -> Result<Vec<String>, WorkError> {
        work_operations::execution::file_transaction::input_paths(
            &context.task_context().contract,
            task,
        )
        .map_err(|e| error(e.reason_code))
    }
    fn migration_review_sources(
        &self,
        _context: &ExecutionWriterContext,
    ) -> Result<BTreeMap<String, Vec<u8>>, WorkError> {
        Err(error("file_transaction_migration_review_required"))
    }
    fn is_directory(&self, _path: &str) -> Result<bool, WorkError> {
        Err(error("file_transaction_input_kind_unavailable"))
    }
    fn accepted_file_scopes(
        &self,
        _context: &ExecutionWriterContext,
    ) -> Result<BTreeMap<String, Vec<String>>, WorkError> {
        Err(error("file_transaction_scope_history_unavailable"))
    }
    fn new_file_metadata(&self, _path: &str) -> Result<FileMetadata, WorkError> {
        Err(error("file_transaction_new_metadata_unavailable"))
    }
    fn history(
        &self,
        _context: &ExecutionWriterContext,
        _attempt_id: &str,
    ) -> Result<work_operations::execution::file_transaction::history::Current, WorkError> {
        Err(error("file_transaction_history_unavailable"))
    }
    fn check_resources(&self, paths: &[String]) -> Result<(), WorkError>;
    fn check_complete_resources(&self, _weighted_bytes: u64) -> Result<(), WorkError> {
        Err(error("file_transaction_capacity_unverified"))
    }
    fn path_identity(&self, _path: &str) -> Result<Option<String>, WorkError> {
        Err(error("file_transaction_identity_unavailable"))
    }
    fn execute_selection(&self, task: &Value, attempt: &Value) -> Result<Value, WorkError>;
    fn read_source(&self, path: &str) -> Result<Vec<u8>, WorkError>;
    fn inspect(&self, path: &str) -> Result<Option<FileState>, WorkError>;
    fn check_prepare(&self, context: &ExecutionWriterContext) -> Result<(), WorkError>;
    fn apply(
        &self,
        context: &ExecutionWriterContext,
        preview: &FileTransactionPreview,
        evidence: &str,
    ) -> Result<FileTransactionResult, WorkError>;
    fn recovery_preview(
        &self,
        context: &ExecutionWriterContext,
        identity: &str,
    ) -> Result<FileRecoveryPreview, WorkError>;
    fn restore(
        &self,
        context: &ExecutionWriterContext,
        preview: &FileRecoveryPreview,
        evidence: &str,
    ) -> Result<FileTransactionResult, WorkError>;
}

pub fn prepare(
    repository: &impl FileTransactionRepository,
    context: &ExecutionWriterContext,
    attempt_id: &str,
    changes: &[FileChange],
    baseline_confirmation: Option<&FileBaselineConfirmation>,
) -> Result<FileTransactionPreview, WorkError> {
    repository.check_prepare(context)?;
    let target = context.target();
    let task_context = context.task_context();
    let index_path = format!("{}/index.json", target.execution_dir);
    let attempt_path = format!(
        "{}/{}/{}/attempt.json",
        target.execution_dir, target.task_id, attempt_id
    );
    let index_raw = repository.read_source(&index_path)?;
    let attempt_raw = repository.read_source(&attempt_path)?;
    let parse = |bytes: &[u8]| {
        work_operations::canonical::parse_json_contract(bytes)
            .map_err(|_| error("file_transaction_source_invalid"))
    };
    let index = parse(&index_raw)?;
    let attempt = parse(&attempt_raw)?;
    context.check_execution_index(&index, &index_raw)?;
    work_operations::execution::attempt::validate_attempt_bytes(&attempt, &attempt_raw)
        .map_err(|e| error(e.reason_code))?;
    work_operations::execution::validate_execution_identity(
        &task_context.collection,
        &task_context.validation,
        &index,
        &attempt,
        target.task_id,
    )
    .map_err(|e| error(e.reason_code))?;
    if attempt["status"] != "in_progress"
        || index["lock"]["task_id"] != target.task_id
        || index["lock"]["attempt_id"] != attempt_id
        || !index["lock"]["record_id"].is_null()
    {
        return Err(error("file_transaction_attempt_not_idle"));
    }
    let task = task_context.contract["tasks"]
        .as_array()
        .and_then(|tasks| tasks.iter().find(|t| t["id"] == target.task_id))
        .ok_or_else(|| error("file_transaction_task_missing"))?;
    verify_execute(repository, task, &attempt)?;
    for dep in task["dependencies"].as_array().into_iter().flatten() {
        if !index["tasks"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|t| t["id"] == *dep && t["status"] == "completed")
        {
            return Err(error("file_transaction_dependency_incomplete"));
        }
    }
    // Requirement provenance does not identify the Task publication workflow.
    // Reconstruction may preserve a real Snapshot without creating a Discussion.
    let review_sources = if task_context.index.get("discussion").is_none() {
        repository.migration_review_sources(context)?
    } else {
        let trace: work_model::task::discussion_trace::DiscussionTrace =
            serde_json::from_value(task_context.index["discussion"].clone())
                .map_err(|_| error("file_transaction_planning_review_required"))?;
        let review_path = format!(
            "outputs/work/discussions/{}/history/{}/session.json",
            context.writer().requirement_id.as_str(),
            trace.revision
        );
        let review_raw = repository.read_source(&review_path)?;
        let session: work_model::discussion::DiscussionSession =
            serde_json::from_slice(&review_raw)
                .map_err(|_| error("file_transaction_planning_review_required"))?;
        if session.commit.content_sha256 != trace.content_sha256
            || session.revision != trace.revision
            || session.requirement_id != context.writer().requirement_id.as_str()
        {
            return Err(error("file_transaction_review_binding_mismatch"));
        }
        work_operations::discussion::ready_to_generate(&session).map_err(|e| error(e.0))?;
        let reviewed = work_operations::discussion::assembly::task_records(&session)
            .map_err(|e| error(e.0))?;
        if !work_operations::task::file_dependencies::matches_reviewed_ancestry(
            &task_context.collection,
            &reviewed,
            context.target().task_id,
        ) || !work_operations::task::file_dependencies::matches_reviewed_source(
            &session,
            &task_context.collection,
        ) {
            return Err(error("file_transaction_review_binding_mismatch"));
        }
        BTreeMap::from([(review_path, review_raw)])
    };
    let effective = candidate::effective_collection(
        &task_context.collection,
        &repository.accepted_file_scopes(context)?,
    )?;
    let prepared = candidate::prepare(repository, &effective, task, &attempt, changes, |path| {
        repository.new_file_metadata(path)
    })?;
    let history = repository.history(context, attempt_id)?;
    let before: BTreeMap<_, _> = prepared
        .targets
        .iter()
        .map(|target| {
            (
                target.path.clone(),
                target.before.as_ref().map(|bytes| FileState {
                    bytes: bytes.bytes.clone(),
                    metadata: prepared.before_metadata[&target.path]
                        .clone()
                        .expect("prepared metadata"),
                }),
            )
        })
        .collect();
    if let Some(confirmation) = baseline_confirmation {
        if history.head.is_empty()
            || confirmation.previous_transaction != history.head
            || confirmation.before_sha256
                != work_operations::execution::file_transaction::history::before_sha256(
                    &before,
                    &prepared.path_identities,
                )
            || confirmation.evidence.trim().is_empty()
        {
            return Err(error("file_transaction_baseline_confirmation_invalid"));
        }
    }
    for (path, expected) in &history.expected {
        if let Some(actual) = before.get(path) {
            let identity_changed = history
                .path_identities
                .get(path)
                .is_some_and(|expected| prepared.path_identities.get(path) != Some(expected));
            if (actual != expected || identity_changed) && baseline_confirmation.is_none() {
                return Err(WorkError::new(
                    ExitCode::ArtifactIntegrity,
                    "file_transaction_baseline_confirmation_required",
                    "Confirm the complete inspected current baseline before another change; preserve the user's contents.",
                    json!({"previous_transaction":history.head,
                        "before_sha256":work_operations::execution::file_transaction::history::before_sha256(&before, &prepared.path_identities),
                        "before":before,"path_identities":prepared.path_identities}),
                ));
            }
        } else {
            let actual_identity = repository.path_identity(path)?;
            if repository.inspect(path)? != *expected
                || history.path_identities.get(path) != Some(&actual_identity)
            {
                return Err(error("file_transaction_target_drift"));
            }
        }
    }
    let mut source_sha256: BTreeMap<_, _> = task_context
        .sources
        .iter()
        .map(|(p, b)| (p.clone(), fingerprint::raw(b)))
        .collect();
    for (path, raw) in review_sources {
        source_sha256.insert(path, fingerprint::raw(&raw));
    }
    for path in crate::task::source::evidence_paths(&task_context.collection)? {
        source_sha256.insert(
            path.clone(),
            fingerprint::raw(&repository.read_source(&path)?),
        );
    }
    let mut directory_inputs = Vec::new();
    for path in repository.input_paths(context, task, attempt_id)? {
        if repository.is_directory(&path)? {
            directory_inputs.push(path);
            continue;
        }
        source_sha256.insert(
            path.clone(),
            fingerprint::raw(&repository.read_source(&path)?),
        );
    }
    source_sha256.insert(index_path, fingerprint::raw(&index_raw));
    source_sha256.insert(attempt_path, fingerprint::raw(&attempt_raw));
    let binding = FileTransactionBinding {
        task_id: target.task_id.into(),
        attempt_id: attempt_id.into(),
        source_sha256,
        directory_inputs,
        changes: changes.to_vec(),
        previous_transaction: (!history.head.is_empty()).then_some(history.head),
        baseline_confirmation: baseline_confirmation.cloned(),
        path_identities: prepared.path_identities,
        before_metadata: prepared.before_metadata,
        after_metadata: prepared.after_metadata,
    };
    for path in &binding.directory_inputs {
        if !repository.is_directory(path)? {
            return Err(error("file_transaction_source_drift"));
        }
    }
    for (p, h) in &binding.source_sha256 {
        if fingerprint::raw(&repository.read_source(p)?) != *h {
            return Err(error("file_transaction_source_drift"));
        }
    }
    derive::preview(
        &context.writer().canonical_project_root.to_string_lossy(),
        context.writer().requirement_id.as_str(),
        target.execution_dir,
        binding,
        prepared.targets,
    )
    .map_err(error)
}

pub fn verify_execute(
    repository: &impl FileTransactionRepository,
    task: &Value,
    attempt: &Value,
) -> Result<(), WorkError> {
    let selection = repository.execute_selection(task, attempt)?;
    if selection["selected_paths"] != task["instruction_selection"]["selected_paths"]
        || selection["resolved_paths"] != task["instruction_selection"]["resolved_paths"]
        || selection["instructions_sha256"] != attempt["execute_instructions_sha256"]
    {
        return Err(error("file_transaction_execute_instructions_stale"));
    }
    Ok(())
}

pub fn run(
    repository: &impl FileTransactionRepository,
    context: &ExecutionWriterContext,
    operation: &str,
    request: &Value,
) -> Result<Value, WorkError> {
    let request: FileTransactionRequest = serde_json::from_value(request.clone())
        .map_err(|_| error("file_transaction_request_invalid"))?;
    if request.schema != PublicSchema::WorkFileTransactionRequest {
        return Err(error("file_transaction_request_schema"));
    }
    let output = match (operation, request.command) {
        (
            "file-prepare",
            FileTransactionCommand::Prepare {
                attempt_id,
                changes,
                baseline_confirmation,
            },
        ) => json!(prepare(
            repository,
            context,
            &attempt_id,
            &changes,
            baseline_confirmation.as_ref()
        )?),
        (
            "file-apply",
            FileTransactionCommand::Apply {
                attempt_id,
                changes,
                expected_before,
                baseline_confirmation,
            },
        ) => {
            let current = prepare(
                repository,
                context,
                &attempt_id,
                &changes,
                baseline_confirmation.as_ref(),
            )?;
            let observed: BTreeMap<_, _> = current
                .manifest
                .targets
                .iter()
                .map(|target| {
                    (
                        target.path.clone(),
                        target.before.as_ref().map(|before| before.sha256.clone()),
                    )
                })
                .collect();
            if expected_before != observed {
                return Err(error("file_transaction_read_baseline_stale"));
            }
            let attempt_path = format!(
                "{}/{}/{}/attempt.json",
                context.target().execution_dir,
                context.target().task_id,
                attempt_id
            );
            let attempt: Value = serde_json::from_slice(&repository.read_source(&attempt_path)?)
                .map_err(|_| error("file_transaction_attempt_invalid"))?;
            let evidence = attempt["authorization"]["authorization_evidence"]
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| error("file_transaction_authorization_required"))?;
            json!(repository.apply(context, &current, evidence)?)
        }
        (
            "file-recovery-prepare",
            FileTransactionCommand::RecoveryPrepare {
                transaction_identity,
            },
        ) => json!(repository.recovery_preview(context, &transaction_identity)?),
        (
            "file-restore",
            FileTransactionCommand::Restore {
                transaction_identity,
                approved_sha256,
                authorization_evidence,
            },
        ) => {
            if authorization_evidence.trim().is_empty() {
                return Err(error("file_transaction_recovery_authorization_required"));
            }
            let preview = repository.recovery_preview(context, &transaction_identity)?;
            if preview.approval_sha256 != approved_sha256 {
                return Err(error("file_transaction_recovery_approval_stale"));
            }
            json!(repository.restore(context, &preview, &authorization_evidence)?)
        }
        _ => return Err(error("file_transaction_command_mismatch")),
    };
    Ok(output)
}
