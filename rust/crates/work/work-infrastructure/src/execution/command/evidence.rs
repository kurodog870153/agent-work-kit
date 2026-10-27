//! Save actual command evidence and verify its owned receipt and current sources.
use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;
use serde_json::json;
use work_feature::error::{ExitCode, WorkError};
use work_feature::execution::{CommandPrepareRepository, ExecutionWriterContext};
use work_feature::ports::CommandOutcome;
use work_model::execution::command::capture::CommandCapturedEvidence;
use work_model::execution::command::capture::{
    CommandExecutionEvidence, ValidationCommandEvidence, ValidationExecutionEvidence,
};
use work_operations::execution::verification::VerifiedCommandResult;

pub(in crate::execution) fn freeze(
    root: &Path,
    directory: &str,
    preview: &Value,
    outcome: &CommandOutcome,
) -> Result<Option<CommandCapturedEvidence>, WorkError> {
    let Some(output) = outcome.complete_output.as_ref() else {
        return Ok(None);
    };
    let policy = serde_json::from_value(preview["execution"]["capture"].clone())
        .map_err(|_| unverified())?;
    let cwd = preview["working_directory"]
        .as_str()
        .ok_or_else(unverified)?;
    super::super::host_evidence::freeze(root, directory, Path::new(cwd), &policy, output).map(Some)
}

fn unverified() -> WorkError {
    WorkError::new(
        ExitCode::WorkflowState,
        "validation_execution_unverified",
        "Actual command evidence is missing, unknown, changed or owned by another Task/Attempt; do not rerun automatically.",
        json!({}),
    )
}

fn evidence_error(stage: &str, record_id: &str) -> WorkError {
    let mut error = unverified();
    error.details = json!({"evidence_stage":stage,"record_id":record_id});
    error
}

fn document(root: &Path, relative: &str) -> Result<(Value, Vec<u8>), WorkError> {
    let path = crate::files::resolve_runtime_path(root, relative)?;
    let raw = super::super::evidence_file::read(&path, 16 * 1024 * 1024)?;
    let value = work_operations::canonical::parse_json_contract(&raw).map_err(|_| unverified())?;
    Ok((value, raw))
}

fn verified_command(
    context: &ExecutionWriterContext,
    task: &Value,
    attempt: &Value,
    record_id: &str,
    correction: Option<&Value>,
    require_current_files: bool,
) -> Result<
    (
        CommandExecutionEvidence,
        Value,
        Option<VerifiedCommandResult>,
    ),
    WorkError,
> {
    let stage = std::cell::Cell::new("receipt_identity");
    let unverified = || evidence_error(stage.get(), record_id);
    let root = &context.writer().canonical_project_root;
    let attempt_id = attempt["attempt_id"].as_str().ok_or_else(unverified)?;
    let paths = work_operations::derivation::publication::command_receipt_paths(
        context.target().execution_dir,
        context.target().task_id,
        attempt_id,
        record_id,
    )
    .map_err(|_| unverified())?;
    let (started, started_raw) = document(root, &paths.started)?;
    let (finished, finished_raw) = document(root, &paths.finished)?;
    let preview = &started["preview"];
    let approval = preview["approved_sha256"].as_str().ok_or_else(unverified)?;
    let checked = work_feature::execution::command_publication::approved_receipt_paths(
        preview,
        context.target().execution_dir,
        approval,
    )?;
    super::super::host_evidence::verify_receipts(
        &super::super::host_evidence::ReceiptIdentity {
            execution_dir: context.target().execution_dir,
            task_id: context.target().task_id,
            attempt_id,
            record_id,
        },
        Some(&started),
        Some(&finished),
    )?;
    if checked != paths {
        return Err(unverified());
    }
    stage.set("finished_receipt_shape");
    let exit_code = finished["exit_code"].as_i64().ok_or_else(unverified)?;
    let _: work_model::execution::response::PreparedCommandResult =
        serde_json::from_value(finished.clone()).map_err(|_| unverified())?;
    let index_path = format!("{}/index.json", context.target().execution_dir);
    let attempt_path = format!(
        "{}/{}/{attempt_id}/attempt.json",
        context.target().execution_dir,
        context.target().task_id
    );
    stage.set("task_context_sources");
    let sources = preview["sources"].as_object().ok_or_else(unverified)?;
    for (path, current) in &context.task_context().sources {
        if sources.get(path)
            != Some(&json!(work_operations::derivation::fingerprint::raw(
                current
            )))
        {
            return Err(unverified());
        }
    }
    stage.set("receipt_file_version");
    let saved_version = preview["execution"]["file_version"]
        .as_str()
        .ok_or_else(unverified)?;
    let historical = super::super::file_transaction::storage::version_snapshot(
        context,
        attempt_id,
        saved_version,
    )?;
    if require_current_files
        && saved_version
            != super::super::file_transaction::storage::current_version(context, attempt_id)?
    {
        return Err(evidence_error("execution_settings", record_id));
    }
    stage.set("receipt_sources");
    if require_current_files && preview["execution"]["input_files"] != super::input_files(context)?
    {
        return Err(evidence_error("command_input_version", record_id));
    }
    for (path, expected) in sources {
        if path == &index_path || path == &attempt_path {
            continue;
        }
        if !require_current_files && let Some(state) = historical.get(path) {
            let Some(state) = state else {
                return Err(unverified());
            };
            if *expected != work_operations::derivation::fingerprint::raw(&state.bytes) {
                return Err(unverified());
            }
            continue;
        }
        let path = crate::files::resolve_runtime_path(root, path)?;
        let bytes = super::super::evidence_file::read(&path, 256 * 1024 * 1024)?;
        if *expected != work_operations::derivation::fingerprint::raw(&bytes) {
            return Err(unverified());
        }
    }
    stage.set("formal_command");
    let effective = work_operations::execution::authorization::effective_task_for_record(
        task,
        attempt,
        Some(record_id),
    )
    .map_err(|_| unverified())?;
    let base = record_id.split('#').next().ok_or_else(unverified)?;
    let formal = effective["commands"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|command| command["id"] == base)
        .ok_or_else(unverified)?;
    let mut actual = json!({"mode":formal["mode"],"argv":formal["argv"]});
    if let Some(correction) = correction {
        actual = work_operations::execution::command::correction::resolve_corrected_command(
            task, attempt, base, &actual, correction,
        )
        .map_err(|_| unverified())?;
    }
    if actual["mode"] != "argv" {
        return Err(unverified());
    }
    let storage = super::super::storage::LocalExecutionStorage {
        project_root: root.clone(),
    };
    stage.set("execution_settings");
    let settings = formal
        .get("execution")
        .unwrap_or(&context.task_context().contract["execution_defaults"]);
    let cwd = storage.working_directory(
        settings["working_directory"]
            .as_str()
            .ok_or_else(unverified)?,
    )?;
    let saved_settings = &preview["execution"];
    let mut expected_settings = settings.clone();
    // The approved preview freezes collection policy for this execution. Later
    // validations may require more evidence, but cannot rewrite this receipt.
    let saved_policy: work_model::execution::command::capture::CommandCapturePolicy =
        serde_json::from_value(saved_settings["capture"].clone()).map_err(|_| unverified())?;
    let bounded_policy = work_model::execution::command::capture::CommandCapturePolicy {
        report_paths: saved_policy.report_paths.clone(),
        ..Default::default()
    };
    if saved_policy != bounded_policy || saved_policy.report_paths.len() > 1024 {
        return Err(unverified());
    }
    expected_settings["capture"] = saved_settings["capture"].clone();
    expected_settings["host"] = super::runner::host_environment();
    expected_settings["file_version"] = json!(saved_version);
    if let Some(inputs) = saved_settings.get("input_files") {
        expected_settings["input_files"] = inputs.clone();
    }
    if *saved_settings != expected_settings || preview["working_directory"] != cwd {
        return Err(unverified());
    }
    stage.set("command_invocation");
    let argv = actual["argv"]
        .as_array()
        .ok_or_else(unverified)?
        .iter()
        .map(|v| v.as_str().map(str::to_owned).ok_or_else(unverified))
        .collect::<Result<Vec<_>, _>>()?;
    let invocation = storage.resolve_command_invocation(&argv, Path::new(&cwd))?;
    if invocation != preview["invocation"] {
        return Err(unverified());
    }
    stage.set("command_proof");
    let proof = CommandExecutionEvidence {
        task_id: context.target().task_id.into(),
        attempt_id: attempt_id.into(),
        record_id: record_id.into(),
        receipt_dir: paths.directory.clone(),
        approved_sha256: approval.into(),
        started_sha256: work_operations::derivation::fingerprint::raw(&started_raw),
        finished_sha256: work_operations::derivation::fingerprint::raw(&finished_raw),
        exit_code,
    };
    stage.set("capture_shape");
    let captures: CommandCapturedEvidence =
        serde_json::from_value(finished["captures"].clone()).map_err(|_| unverified())?;
    let policy = serde_json::from_value(preview["execution"]["capture"].clone())
        .map_err(|_| unverified())?;
    stage.set("capture_bytes");
    let super::super::host_evidence::VerifiedCaptures {
        stdout,
        stderr,
        reports,
    } = super::super::host_evidence::verify_captures(root, &paths.directory, &captures, &policy)
        .map_err(|_| unverified())?;
    let result = match (stdout, stderr) {
        (Some(stdout), Some(stderr)) => Some(VerifiedCommandResult {
            exit_code,
            stdout,
            stderr,
            reports,
        }),
        _ => None,
    };
    Ok((proof, finished, result))
}

fn owned_record(
    context: &ExecutionWriterContext,
    attempt: &Value,
    record_id: &str,
    depth: usize,
) -> Result<(Value, Value), WorkError> {
    if depth > 64 {
        return Err(unverified());
    }
    if let Some(record) = attempt["records"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|record| record["id"] == record_id)
    {
        return Ok((attempt.clone(), record.clone()));
    }
    let carried = attempt["carried_records"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|record| record["record_id"] == record_id)
        .ok_or_else(unverified)?;
    let source = carried["source_attempt_id"]
        .as_str()
        .ok_or_else(unverified)?;
    let path = format!(
        "{}/{}/{source}/attempt.json",
        context.target().execution_dir,
        context.target().task_id
    );
    let (previous, raw) = document(&context.writer().canonical_project_root, &path)?;
    work_operations::execution::attempt::validate_attempt_bytes(&previous, &raw)
        .map_err(|_| unverified())?;
    if previous["attempt_id"] != source
        || previous["task_id"] != context.target().task_id
        || previous["task_item_sha256"] != attempt["task_item_sha256"]
        || previous["status"] == "in_progress"
    {
        return Err(unverified());
    }
    owned_record(context, &previous, record_id, depth + 1)
}

fn validation_result(
    context: &ExecutionWriterContext,
    task: &Value,
    attempt: &Value,
    validation: &Value,
    record_id: &str,
) -> Result<ValidationExecutionEvidence, WorkError> {
    validation_result_with_current(context, task, attempt, validation, record_id, true)
}

fn validation_result_with_current(
    context: &ExecutionWriterContext,
    task: &Value,
    attempt: &Value,
    validation: &Value,
    record_id: &str,
    require_current: bool,
) -> Result<ValidationExecutionEvidence, WorkError> {
    let condition = work_operations::execution::verification::procedure_condition(validation)
        .map_err(|_| unverified())?;
    let mut commands = BTreeMap::new();
    let mut results = BTreeMap::new();
    let mut remaining = work_model::execution::command::capture::MAX_VALIDATION_BYTES;
    for command in validation["command_ids"]
        .as_array()
        .ok_or_else(unverified)?
    {
        let command = command.as_str().ok_or_else(unverified)?;
        let direct = attempt["records"]
            .as_array()
            .into_iter()
            .flatten()
            .rev()
            .find(|record| {
                record["kind"] == "command"
                    && record["id"]
                        .as_str()
                        .is_some_and(|id| id.split('#').next() == Some(command))
            });
        let (id, selected_from) = if let Some(record) = direct {
            (
                record["id"].as_str().ok_or_else(unverified)?,
                attempt["attempt_id"].as_str().ok_or_else(unverified)?,
            )
        } else {
            let carried = attempt["carried_records"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|record| {
                    record["record_id"]
                        .as_str()
                        .is_some_and(|id| id.split('#').next() == Some(command))
                })
                .max_by_key(|record| {
                    record["record_id"]
                        .as_str()
                        .and_then(|id| id.split_once('#'))
                        .and_then(|(_, retry)| retry.parse::<u64>().ok())
                        .unwrap_or(0)
                })
                .ok_or_else(unverified)?;
            (
                carried["record_id"].as_str().ok_or_else(unverified)?,
                carried["source_attempt_id"]
                    .as_str()
                    .ok_or_else(unverified)?,
            )
        };
        let (owner, record) = owned_record(context, attempt, id, 0)?;
        if record["status"] == "skipped" || record["kind"] != "command" {
            return Err(unverified());
        }
        let (proof, finished, result) = verified_command(
            context,
            task,
            &owner,
            id,
            record.get("correction"),
            require_current,
        )
        .map_err(|mut error| {
            error.details["record_id"] = json!(id);
            if error.details.get("evidence_stage").is_none() {
                error.details["evidence_stage"] = json!("command_reverification");
            }
            error
        })?;
        if record["execution_evidence"] != serde_json::to_value(&proof).map_err(|_| unverified())?
            || record["exit_code"] != proof.exit_code
        {
            return Err(evidence_error("stored_command_proof", id));
        }
        let result = result.ok_or_else(|| {
            let mut error = evidence_error("capture_incomplete", id);
            for stream in ["stdout", "stderr"] {
                error.details[stream] = json!({
                    "complete":finished["captures"][stream]["complete"],
                    "size_bytes":finished["captures"][stream]["size_bytes"]
                });
            }
            error
        })?;
        let size = result.reports.values().flatten().try_fold(
            (result.stdout.len() as u64)
                .checked_add(result.stderr.len() as u64)
                .ok_or_else(unverified)?,
            |sum, report| sum.checked_add(report.len() as u64).ok_or_else(unverified),
        )?;
        remaining = remaining.checked_sub(size).ok_or_else(unverified)?;
        results.insert(command.into(), result);
        commands.insert(
            command.into(),
            ValidationCommandEvidence {
                selected_from_attempt_id: selected_from.into(),
                execution: proof,
            },
        );
    }
    let passed = work_operations::execution::verification::evaluate(
        &condition,
        &validation["command_ids"],
        &results,
    )
    .map_err(|_| evidence_error("condition_evaluation", record_id))?;
    Ok(ValidationExecutionEvidence {
        task_id: context.target().task_id.into(),
        attempt_id: attempt["attempt_id"]
            .as_str()
            .ok_or_else(unverified)?
            .into(),
        record_id: record_id.into(),
        engine_sha256: work_operations::execution::verification::engine_sha256(),
        condition_sha256: work_operations::derivation::fingerprint::structured(&condition)
            .map_err(|_| unverified())?,
        passed,
        commands,
    })
}

fn verify_review_references(
    context: &ExecutionWriterContext,
    task: &Value,
    attempt: &Value,
    validation: &Value,
    review: &work_model::execution::acceptance::AiReviewEvidence,
) -> Result<(), WorkError> {
    use work_model::execution::acceptance::ReviewReference::*;
    work_operations::execution::verification::validate_review(validation, review)
        .map_err(|_| unverified())?;
    for reference in &review.references {
        match reference {
            Artifact { path, sha256, .. } => {
                let actual = crate::files::resolve_runtime_path(
                    &context.writer().canonical_project_root,
                    path,
                )?;
                let raw = super::super::evidence_file::read(
                    &actual,
                    work_model::execution::command::capture::MAX_REPORT_BYTES,
                )?;
                if work_operations::derivation::fingerprint::raw(&raw) != *sha256 {
                    return Err(unverified());
                }
            }
            Command {
                record_id,
                finished_sha256,
                ..
            } => {
                if !completed_record_ids(attempt)?.contains(&record_id.as_str()) {
                    return Err(unverified());
                }
                let (owner, record) = owned_record(context, attempt, record_id, 0)?;
                if record["kind"] != "command" || record["status"] == "skipped" {
                    return Err(unverified());
                }
                let (proof, _, _) = verified_command(
                    context,
                    task,
                    &owner,
                    record_id,
                    record.get("correction"),
                    true,
                )?;
                if proof.finished_sha256 != *finished_sha256
                    || record["execution_evidence"] != json!(proof)
                {
                    return Err(unverified());
                }
            }
            Change {
                transaction_identity,
                path,
                before_sha256,
                after_sha256,
                ..
            } => {
                super::super::file_transaction::storage::verify_change_reference(
                    context,
                    attempt["attempt_id"].as_str().ok_or_else(unverified)?,
                    transaction_identity,
                    path,
                    before_sha256.as_deref(),
                    after_sha256.as_deref(),
                )?;
            }
        }
    }
    Ok(())
}

pub(in crate::execution) fn finalize(
    context: &ExecutionWriterContext,
    task: &Value,
    attempt: &Value,
    index: &Value,
    request: &Value,
    generated: bool,
) -> Result<Value, WorkError> {
    verify_carried(context, task, attempt)?;
    let record_id = index["lock"]["record_id"].as_str().ok_or_else(unverified)?;
    let base = record_id.split('#').next().ok_or_else(unverified)?;
    let effective = work_operations::execution::authorization::effective_task(task, attempt)
        .map_err(|_| unverified())?;
    let kind = work_operations::execution::formal_record_kind(&effective, base)
        .map_err(|_| unverified())?;
    let has_proof = request["record"].get("execution_evidence").is_some()
        || request["record"].get("verification_evidence").is_some()
        || request["record"].get("file_version").is_some()
        || ["source", "review_evidence", "confirmation_evidence"]
            .iter()
            .any(|field| request["record"].get(*field).is_some());
    if !generated && has_proof {
        return Err(unverified());
    }
    let mut result = request.clone();
    if kind == "command" && request["record"]["status"] != "skipped" {
        if request
            .get("modified_files")
            .is_some_and(|files| !files.is_null())
        {
            return Err(unverified());
        }
        let selected = work_operations::execution::command::run::select_reserved_argv_command(
            task,
            attempt,
            index,
            context.target().task_id,
        )
        .map_err(|_| unverified())?;
        if selected.record_id != record_id {
            return Err(unverified());
        }
        let (proof, _, _) = verified_command(
            context,
            task,
            attempt,
            record_id,
            index["lock"].get("command_correction"),
            false,
        )?;
        if request["record"]["exit_code"] != proof.exit_code {
            return Err(unverified());
        }
        result["record"] = json!({"exit_code":proof.exit_code,"result":format!("Actual command exited with code {}; inspect frozen execution receipts.", proof.exit_code),"execution_evidence":proof});
    } else if kind == "validation" && request["record"]["status"] != "skipped" {
        let validation = effective["validations"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|row| row["id"] == base)
            .ok_or_else(unverified)?;
        use work_model::execution::acceptance::*;
        let source = work_operations::execution::verification::validation_source(validation)
            .map_err(|_| unverified())?;
        let version = super::acceptance_version(
            context,
            attempt["attempt_id"].as_str().ok_or_else(unverified)?,
        )?;
        match source {
            ValidationEvidenceSource::Procedure => {
                if request["record"].get("review").is_some()
                    || request["record"].get("confirmation").is_some()
                    || request["record"].get("review_evidence").is_some()
                    || request["record"].get("confirmation_evidence").is_some()
                {
                    return Err(unverified());
                }
                let proof = validation_result(context, task, attempt, validation, record_id)?;
                let outcome = if proof.passed { "passed" } else { "failed" };
                if request["record"]["outcome"] != outcome {
                    let mut error = evidence_error("validation_outcome", record_id);
                    error.details["requested_outcome"] = request["record"]["outcome"].clone();
                    error.details["evaluated_outcome"] = json!(outcome);
                    return Err(error);
                }
                result["record"] = json!({"outcome":outcome,"evidence":format!("Actual owned command evidence evaluated the condition as {outcome}."),"verification_evidence":proof});
            }
            ValidationEvidenceSource::AiReview => {
                if request["record"].get("confirmation").is_some()
                    || request["record"].get("confirmation_evidence").is_some()
                    || request["record"].get("verification_evidence").is_some()
                {
                    return Err(unverified());
                }
                let value = if generated {
                    &request["record"]["review_evidence"]
                } else {
                    &request["record"]["review"]
                };
                let review: AiReviewEvidence = serde_json::from_value(value.clone())
                    .map_err(|_| evidence_error("ai_review_required", record_id))?;
                verify_review_references(context, task, attempt, validation, &review)?;
                result["record"] = json!({"outcome":request["record"]["outcome"],"evidence":review.rationale,"review_evidence":review});
            }
            ValidationEvidenceSource::UserConfirmation => {
                if request["record"].get("review").is_some()
                    || request["record"].get("review_evidence").is_some()
                    || request["record"].get("verification_evidence").is_some()
                {
                    return Err(unverified());
                }
                let proof: UserConfirmationEvidence = if generated {
                    serde_json::from_value(request["record"]["confirmation_evidence"].clone())
                        .map_err(|_| unverified())?
                } else {
                    let reference: EvidenceArtifact =
                        serde_json::from_value(request["record"]["confirmation"].clone())
                            .map_err(|_| evidence_error("user_confirmation_required", record_id))?;
                    if !work_operations::protocol::valid_sha256(&reference.sha256) {
                        return Err(unverified());
                    }
                    let path = crate::files::resolve_runtime_path(
                        &context.writer().canonical_project_root,
                        &reference.path,
                    )?;
                    let raw = super::super::evidence_file::read(
                        &path,
                        MAX_USER_CONFIRMATION_BYTES as u64,
                    )?;
                    if work_operations::derivation::fingerprint::raw(&raw) != reference.sha256 {
                        return Err(unverified());
                    }
                    let parsed = work_operations::canonical::parse_json_contract(&raw)
                        .map_err(|_| unverified())?;
                    let document: UserConfirmationDocument = serde_json::from_value(parsed)
                        .map_err(|_| evidence_error("user_confirmation_origin", record_id))?;
                    UserConfirmationEvidence {
                        reference,
                        document,
                        raw,
                    }
                };
                result["record"] = json!({"outcome":request["record"]["outcome"],"evidence":proof.document.statement,"confirmation_evidence":proof});
            }
        }
        result["record"]["source"] = json!(source);
        result["record"]["file_version"] = json!(version);
        let mut record = result["record"].clone();
        record["id"] = json!(record_id);
        work_operations::execution::verification::validate_record_provenance(
            validation, attempt, &record,
        )
        .map_err(|_| unverified())?;
    } else if has_proof {
        return Err(unverified());
    }
    if generated && result != *request {
        return Err(unverified());
    }
    Ok(result)
}

fn completed_record_ids(attempt: &Value) -> Result<Vec<&str>, WorkError> {
    let records = attempt["records"].as_array().ok_or_else(unverified)?;
    let mut selected = BTreeMap::<&str, &str>::new();
    for record in records {
        let id = record["id"].as_str().ok_or_else(unverified)?;
        selected.insert(id.split('#').next().ok_or_else(unverified)?, id);
    }
    let mut carried = BTreeMap::<&str, (u64, &str)>::new();
    for record in attempt["carried_records"].as_array().into_iter().flatten() {
        let id = record["record_id"].as_str().ok_or_else(unverified)?;
        let (base, retry) = if let Some((base, retry)) = id.split_once('#') {
            (base, retry.parse::<u64>().map_err(|_| unverified())?)
        } else {
            (id, 1)
        };
        if !selected.contains_key(base)
            && carried
                .get(base)
                .is_none_or(|(previous, _)| retry > *previous)
        {
            carried.insert(base, (retry, id));
        }
    }
    selected.extend(carried.into_iter().map(|(base, (_, id))| (base, id)));
    Ok(selected.into_values().collect())
}

pub(in crate::execution) fn finalize_carried(
    context: &ExecutionWriterContext,
    task: &Value,
    attempt: &Value,
) -> Result<Value, WorkError> {
    let effective = work_operations::execution::authorization::effective_task(task, attempt)
        .map_err(|_| unverified())?;
    let mut result = attempt.clone();
    for (position, carried) in attempt["carried_records"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let id = carried["record_id"].as_str().ok_or_else(unverified)?;
        let base = id.split('#').next().ok_or_else(unverified)?;
        if !base.starts_with("VAL-")
            || attempt["records"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|row| {
                    row["kind"] == "validation"
                        && row["id"]
                            .as_str()
                            .is_some_and(|id| id.split('#').next() == Some(base))
                })
        {
            continue;
        }
        let validation = effective["validations"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|row| row["id"] == base)
            .ok_or_else(unverified)?;
        if validation["kind"] != "automated" {
            continue;
        }
        let (owner, record) = owned_record(context, attempt, id, 0)?;
        if record["kind"] != "validation"
            || record["status"] == "skipped"
            || record["outcome"] != "passed"
        {
            return Err(unverified());
        }
        let proof = validation_result_with_current(context, task, &owner, validation, id, false)?;
        if !proof.passed
            || record["verification_evidence"]
                != serde_json::to_value(&proof).map_err(|_| unverified())?
        {
            return Err(unverified());
        }
        let has_current_command = |command: &str| {
            attempt["records"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|row| {
                    row["kind"] == "command"
                        && row["id"]
                            .as_str()
                            .is_some_and(|id| id.split('#').next() == Some(command))
                })
        };
        for (command, selected) in &proof.commands {
            if has_current_command(command) {
                continue;
            }
            let latest = attempt["carried_records"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|row| {
                    row["record_id"]
                        .as_str()
                        .is_some_and(|id| id.split('#').next() == Some(command.as_str()))
                })
                .max_by_key(|row| {
                    row["record_id"]
                        .as_str()
                        .and_then(|id| id.split_once('#'))
                        .and_then(|(_, retry)| retry.parse::<u64>().ok())
                        .unwrap_or(0)
                });
            if let Some(latest) = latest {
                let id = latest["record_id"].as_str().ok_or_else(unverified)?;
                let (command_owner, command_record) = owned_record(context, attempt, id, 0)?;
                let (execution, _, _) = verified_command(
                    context,
                    task,
                    &command_owner,
                    id,
                    command_record.get("correction"),
                    false,
                )?;
                if execution != selected.execution {
                    return Err(unverified());
                }
            }
        }
        let source_id = carried["source_attempt_id"]
            .as_str()
            .ok_or_else(unverified)?;
        let owner_id = owner["attempt_id"].as_str().ok_or_else(unverified)?;
        let relative = |id: &str| {
            format!(
                "{}/{}/{id}/attempt.json",
                context.target().execution_dir,
                context.target().task_id
            )
        };
        let (source, source_raw) = document(
            &context.writer().canonical_project_root,
            &relative(source_id),
        )?;
        let (_, owner_raw) = document(
            &context.writer().canonical_project_root,
            &relative(owner_id),
        )?;
        if source["status"] == "in_progress" {
            return Err(unverified());
        }
        result["carried_records"][position]["validation_evidence"] = json!({
            "source_attempt_sha256":work_operations::derivation::fingerprint::raw(&source_raw),
            "owner_attempt_id":owner_id,"owner_attempt_sha256":work_operations::derivation::fingerprint::raw(&owner_raw),
            "record":record});
        if proof
            .commands
            .keys()
            .any(|command| has_current_command(command))
        {
            // A new direct CMD supersedes this VAL for acceptance. Keep and
            // authenticate its frozen provenance while allowing a real VAL
            // retry; aggregation still refuses it as current acceptance proof.
            work_operations::execution::verification::validate_carried_evidence_shape(
                &result,
                &result["carried_records"][position],
            )
            .map_err(|_| unverified())?;
        } else {
            work_operations::execution::verification::validate_carried_validation_evidence(
                validation,
                &result,
                &result["carried_records"][position],
            )
            .map_err(|_| unverified())?;
        }
    }
    result["acceptance_results"] =
        work_operations::execution::acceptance::aggregate_attempt(&effective, &result)
            .map_err(|_| unverified())?;
    Ok(result)
}

pub(in crate::execution) fn verify_carried(
    context: &ExecutionWriterContext,
    task: &Value,
    attempt: &Value,
) -> Result<(), WorkError> {
    let verified = finalize_carried(context, task, attempt)?;
    if verified["carried_records"] != attempt["carried_records"] {
        return Err(unverified());
    }
    Ok(())
}

pub(in crate::execution) fn verify_completed_attempt(
    context: &ExecutionWriterContext,
    task: &Value,
    attempt: &Value,
) -> Result<(), WorkError> {
    verify_carried(context, task, attempt)?;
    let effective = work_operations::execution::authorization::effective_task(task, attempt)
        .map_err(|_| unverified())?;
    for id in completed_record_ids(attempt)? {
        let (owner, record) = owned_record(context, attempt, id, 0)?;
        let carried = owner["attempt_id"] != attempt["attempt_id"];
        let base = id.split('#').next().ok_or_else(unverified)?;
        if record["status"] == "skipped" && !carried {
            continue;
        }
        if record["kind"] == "command" {
            if record["status"] == "skipped" {
                continue;
            }
            let (proof, _, _) =
                verified_command(context, task, &owner, id, record.get("correction"), false)?;
            if record["execution_evidence"]
                != serde_json::to_value(&proof).map_err(|_| unverified())?
                || record["exit_code"] != proof.exit_code
            {
                return Err(unverified());
            }
        } else if record["kind"] == "validation" {
            if record["status"] != "skipped"
                && record["file_version"]
                    != json!(super::acceptance_version(
                        context,
                        owner["attempt_id"].as_str().ok_or_else(unverified)?
                    )?)
            {
                return Err(evidence_error("validation_file_version", id));
            }
            let validation = effective["validations"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|row| row["id"] == base)
                .ok_or_else(unverified)?;
            if record["status"] != "skipped" {
                work_operations::execution::verification::validate_record_provenance(
                    validation, &owner, &record,
                )
                .map_err(|_| unverified())?;
                if validation["kind"] == "ai_review" {
                    let proof: work_model::execution::acceptance::AiReviewEvidence =
                        serde_json::from_value(record["review_evidence"].clone())
                            .map_err(|_| unverified())?;
                    verify_review_references(context, task, &owner, validation, &proof)?;
                }
            }
            if validation["kind"] == "automated" {
                if record["status"] == "skipped" {
                    return Err(unverified());
                }
                let proof = validation_result(context, task, &owner, validation, id)?;
                if record["verification_evidence"]
                    != serde_json::to_value(&proof).map_err(|_| unverified())?
                    || (proof.passed != (record["outcome"] == "passed"))
                    || (carried && !proof.passed)
                {
                    return Err(unverified());
                }
            }
        }
    }
    Ok(())
}

#[cfg(all(test, any(target_os = "macos", windows)))]
mod tests {
    use super::*;
    use work_feature::ports::{CommandOutputCapture, CommandStatus};
    use work_model::execution::command::capture::{CapturedReport, CommandCapturePolicy};

    #[test]
    fn absent_full_output_cannot_create_complete_evidence_or_a_finished_receipt() {
        let root = std::env::temp_dir();
        let result = CommandOutcome {
            diagnostic: None,
            timing: None,
            status: CommandStatus::Unknown,
            exit_code: None,
            signal: None,
            stdout_tail: "known partial".into(),
            stderr_tail: String::new(),
            stdout_truncated: false,
            stderr_truncated: false,
            complete_output: None,
        };
        assert!(
            freeze(&root, "never-created", &json!({}), &result)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn completed_evidence_selects_latest_direct_or_carried_record_without_resurrecting_skips() {
        let attempt = json!({"records":[
            {"id":"CMD-001"},{"id":"CMD-001#2","status":"skipped"},
            {"id":"VAL-001","kind":"validation"}],
            "carried_records":[{"record_id":"CMD-001#9"},
            {"record_id":"VAL-002#10"},{"record_id":"VAL-002#2"},
            {"record_id":"CMD-002"}]});
        assert_eq!(
            completed_record_ids(&attempt).unwrap(),
            ["CMD-001#2", "CMD-002", "VAL-001", "VAL-002#10"]
        );
        assert_eq!(
            completed_record_ids(&json!({"records":[]})).unwrap(),
            Vec::<&str>::new()
        );
        assert!(
            completed_record_ids(
                &json!({"records":[],"carried_records":[{"record_id":"VAL-001#unknown"}]})
            )
            .is_err()
        );
    }

    #[test]
    fn captures_freeze_full_bytes_reports_and_missing_evidence_without_reuse() {
        let root = std::env::temp_dir().join(format!(
            "work capture {} {}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("receipts/CMD-001")).unwrap();
        std::fs::create_dir(root.join("staging")).unwrap();
        let root = root.canonicalize().unwrap();
        std::fs::write(root.join("staging/report.json"), b"{\"passed\":true}").unwrap();
        let policy = CommandCapturePolicy {
            report_paths: ["report.json".into(), "missing.json".into()]
                .into_iter()
                .collect(),
            ..Default::default()
        };
        let preview = json!({"working_directory":root.join("staging"),
            "execution":{"capture":policy}});
        let outcome = CommandOutcome {
            diagnostic: None,
            timing: None,
            status: CommandStatus::Exited,
            exit_code: Some(0),
            signal: None,
            stdout_tail: "tail".into(),
            stderr_tail: String::new(),
            stdout_truncated: true,
            stderr_truncated: false,
            complete_output: Some(CommandOutputCapture {
                stdout: vec![b'x'; 8192],
                stderr: vec![0, 255],
                stdout_complete: true,
                stderr_complete: false,
            }),
        };
        let evidence = freeze(&root, "receipts/CMD-001", &preview, &outcome)
            .unwrap()
            .unwrap();
        assert_eq!(evidence.stdout.size_bytes, 8192);
        assert!(evidence.stdout.complete);
        assert!(!evidence.stderr.complete);
        assert_eq!(
            std::fs::read(root.join("receipts/CMD-001/stdout.bin")).unwrap(),
            vec![b'x'; 8192]
        );
        assert!(matches!(
            evidence.reports["missing.json"],
            CapturedReport::Unavailable { .. }
        ));
        let CapturedReport::Available { artifact } = &evidence.reports["report.json"] else {
            panic!("report not frozen")
        };
        std::fs::write(root.join("staging/report.json"), b"changed").unwrap();
        assert_eq!(
            std::fs::read(root.join("receipts/CMD-001").join(&artifact.file)).unwrap(),
            b"{\"passed\":true}"
        );
        assert!(freeze(&root, "receipts/CMD-001", &preview, &outcome).is_err());
        let mut untrusted = outcome;
        untrusted.complete_output = None;
        assert!(
            freeze(&root, "receipts/CMD-001", &preview, &untrusted)
                .unwrap()
                .is_none()
        );
    }
}
