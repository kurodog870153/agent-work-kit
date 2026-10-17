//! Evaluation uses complete executor evidence; unknown never becomes true via logic.
use super::ExecutionIssue;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use work_model::execution::command::capture::{
    CommandExecutionEvidence, ValidationExecutionEvidence,
};
use work_model::task::verification::VerificationCondition;

fn unknown() -> ExecutionIssue {
    ExecutionIssue {
        reason_code: "validation_execution_unverified",
        message: "Validation requires complete evidence with the declared source, owner and current version.",
        details: json!({}),
    }
}

pub fn validation_source(
    validation: &Value,
) -> Result<work_model::execution::acceptance::ValidationEvidenceSource, ExecutionIssue> {
    use work_model::execution::acceptance::ValidationEvidenceSource::*;
    match validation["kind"].as_str() {
        Some("automated") => Ok(Procedure),
        Some("ai_review") => Ok(AiReview),
        Some("manual") => Ok(UserConfirmation),
        _ => Err(unknown()),
    }
}

/// Standard command checks do not require callers to author a condition expression.
/// Explicit conditions continue to express reports, text checks and expected nonzero exits.
pub fn procedure_condition(validation: &Value) -> Result<Value, ExecutionIssue> {
    if validation["kind"] != "automated" {
        return Err(unknown());
    }
    let condition = if let Some(condition) = validation.get("verification") {
        condition.clone()
    } else {
        let commands = validation["command_ids"]
            .as_array()
            .filter(|c| !c.is_empty())
            .ok_or_else(unknown)?;
        json!({"kind":"all","conditions":commands.iter().map(|c|json!({"kind":"exit_code","command":c,"equals":0})).collect::<Vec<_>>()})
    };
    crate::task::verification::validate(&condition, &validation["command_ids"])
        .map_err(|_| unknown())?;
    Ok(condition)
}
fn artifact_path(path: &str) -> bool {
    crate::derivation::identity::runtime_relative_path(path)
        && path != ".git"
        && !path.starts_with(".git/")
}
pub fn validate_review(
    validation: &Value,
    review: &work_model::execution::acceptance::AiReviewEvidence,
) -> Result<(), ExecutionIssue> {
    use work_model::execution::acceptance::ReviewReference::*;
    if validation["kind"] != "ai_review"
        || review.reviewer != validation["confirmer"]
        || review.reviewer.trim().is_empty()
        || review.rationale.trim().is_empty()
        || review.references.is_empty()
        || review.references.len() > 64
    {
        return Err(unknown());
    }
    let mut seen = BTreeSet::new();
    for reference in &review.references {
        let detail = match reference {
            Artifact {
                path,
                sha256,
                detail,
            } => {
                if !artifact_path(path) || !crate::protocol::valid_sha256(sha256) {
                    return Err(unknown());
                }
                detail
            }
            Command {
                record_id,
                finished_sha256,
                detail,
            } => {
                let (base, retry) = record_id.split_once('#').unwrap_or((record_id, ""));
                if crate::task::item::numbered(base, "CMD").is_none()
                    || !crate::protocol::valid_sha256(finished_sha256)
                    || (record_id.contains('#')
                        && retry
                            .parse::<u64>()
                            .ok()
                            .is_none_or(|n| n == 0 || n.to_string() != retry))
                {
                    return Err(unknown());
                }
                detail
            }
            Change {
                transaction_identity,
                path,
                before_sha256,
                after_sha256,
                detail,
            } => {
                if !crate::protocol::valid_sha256(transaction_identity)
                    || !artifact_path(path)
                    || (before_sha256.is_none() && after_sha256.is_none())
                    || [before_sha256, after_sha256]
                        .into_iter()
                        .flatten()
                        .any(|h| !crate::protocol::valid_sha256(h))
                {
                    return Err(unknown());
                }
                detail
            }
        };
        let key =
            crate::derivation::fingerprint::structured(&json!(reference)).map_err(|_| unknown())?;
        if detail.trim().is_empty() || !seen.insert(key) {
            return Err(unknown());
        }
    }
    Ok(())
}

pub fn validate_record_provenance(
    validation: &Value,
    attempt: &Value,
    record: &Value,
) -> Result<(), ExecutionIssue> {
    use work_model::execution::acceptance::*;
    let source = validation_source(validation)?;
    if record["source"] != json!(source)
        || !record["file_version"]
            .as_str()
            .is_some_and(crate::protocol::valid_sha256)
        || record["id"].as_str().and_then(|id| id.split('#').next()) != validation["id"].as_str()
    {
        return Err(unknown());
    }
    match source {
        ValidationEvidenceSource::Procedure => {
            if record.get("review_evidence").is_some()
                || record.get("confirmation_evidence").is_some()
            {
                return Err(unknown());
            }
            validate_validation_evidence(validation, attempt, record)
        }
        ValidationEvidenceSource::AiReview => {
            if record.get("verification_evidence").is_some()
                || record.get("confirmation_evidence").is_some()
            {
                return Err(unknown());
            }
            let review: AiReviewEvidence =
                serde_json::from_value(record["review_evidence"].clone()).map_err(|_| unknown())?;
            validate_review(validation, &review)?;
            if record["evidence"] != review.rationale {
                return Err(unknown());
            }
            Ok(())
        }
        ValidationEvidenceSource::UserConfirmation => {
            if record.get("verification_evidence").is_some()
                || record.get("review_evidence").is_some()
            {
                return Err(unknown());
            }
            let proof: UserConfirmationEvidence =
                serde_json::from_value(record["confirmation_evidence"].clone())
                    .map_err(|_| unknown())?;
            if !artifact_path(&proof.reference.path)
                || proof.raw.len() > MAX_USER_CONFIRMATION_BYTES
                || crate::derivation::fingerprint::raw(&proof.raw) != proof.reference.sha256
            {
                return Err(unknown());
            }
            let parsed =
                crate::canonical::parse_json_contract(&proof.raw).map_err(|_| unknown())?;
            let document: UserConfirmationDocument =
                serde_json::from_value(parsed).map_err(|_| unknown())?;
            if document != proof.document
                || document.source_reference.trim().is_empty()
                || document.statement.trim().is_empty()
                || document.confirmer != validation["confirmer"]
                || document.confirmer.trim().is_empty()
                || document.task_id != attempt["task_id"]
                || document.attempt_id != attempt["attempt_id"]
                || document.validation_id != validation["id"]
                || document.record_id != record["id"]
                || document.file_version != record["file_version"]
                || json!(document.outcome) != record["outcome"]
                || document.statement != record["evidence"]
            {
                return Err(unknown());
            }
            Ok(())
        }
    }
}

pub struct VerifiedCommandResult {
    pub exit_code: i64,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub reports: BTreeMap<String, Option<Vec<u8>>>,
}

pub fn engine_sha256() -> String {
    let mut sources = Vec::new();
    for source in [
        include_bytes!("verification.rs").as_slice(),
        include_bytes!("../../../work-model/src/execution/command/capture.rs").as_slice(),
    ] {
        sources.extend_from_slice(&(source.len() as u64).to_le_bytes());
        sources.extend_from_slice(source);
    }
    crate::derivation::fingerprint::raw(&sources)
}

pub fn evaluate(
    condition: &Value,
    command_ids: &Value,
    results: &BTreeMap<String, VerifiedCommandResult>,
) -> Result<bool, ExecutionIssue> {
    crate::task::verification::validate(condition, command_ids).map_err(|_| unknown())?;
    let condition: VerificationCondition =
        serde_json::from_value(condition.clone()).map_err(|_| unknown())?;
    fn visit(
        condition: &VerificationCondition,
        results: &BTreeMap<String, VerifiedCommandResult>,
    ) -> Result<bool, ExecutionIssue> {
        use VerificationCondition::*;
        match condition {
            All { conditions } | Any { conditions } => {
                // Evaluate every child before combining: a known true/false child
                // cannot hide missing, malformed or unknown evidence elsewhere.
                let values = conditions
                    .iter()
                    .map(|condition| visit(condition, results))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(if matches!(condition, All { .. }) {
                    values.iter().all(|v| *v)
                } else {
                    values.iter().any(|v| *v)
                })
            }
            Not { condition } => Ok(!visit(condition, results)?),
            leaf => {
                let command = match leaf {
                    ExitCode { command, .. }
                    | StdoutEquals { command, .. }
                    | StdoutContains { command, .. }
                    | StderrEquals { command, .. }
                    | StderrContains { command, .. }
                    | StdoutSha256 { command, .. }
                    | StderrSha256 { command, .. }
                    | ReportSha256 { command, .. }
                    | JsonReportEquals { command, .. } => command,
                    _ => return Err(unknown()),
                };
                let result = results.get(command).ok_or_else(unknown)?;
                let report = |path: &str| {
                    result
                        .reports
                        .get(path)
                        .and_then(Option::as_ref)
                        .ok_or_else(unknown)
                };
                let contains = |bytes: &[u8], text: &str| {
                    bytes
                        .windows(text.len())
                        .any(|part| part == text.as_bytes())
                };
                match leaf {
                    ExitCode { equals, .. } => Ok(result.exit_code == *equals),
                    StdoutEquals { text, .. } => Ok(result.stdout == text.as_bytes()),
                    StderrEquals { text, .. } => Ok(result.stderr == text.as_bytes()),
                    StdoutContains { text, .. } => Ok(contains(&result.stdout, text)),
                    StderrContains { text, .. } => Ok(contains(&result.stderr, text)),
                    StdoutSha256 { sha256, .. } => {
                        Ok(crate::derivation::fingerprint::raw(&result.stdout) == *sha256)
                    }
                    StderrSha256 { sha256, .. } => {
                        Ok(crate::derivation::fingerprint::raw(&result.stderr) == *sha256)
                    }
                    ReportSha256 { path, sha256, .. } => {
                        Ok(crate::derivation::fingerprint::raw(report(path)?) == *sha256)
                    }
                    JsonReportEquals {
                        path,
                        pointer,
                        equals,
                        ..
                    } => {
                        // Reuse the strict duplicate-key parser, allowing any JSON
                        // root inside a wrapper instead of treating reports as artifacts.
                        let mut wrapped = b"{\"report\":".to_vec();
                        wrapped.extend_from_slice(report(path)?);
                        wrapped.push(b'}');
                        let parsed = crate::canonical::parse_json_contract(&wrapped)
                            .map_err(|_| unknown())?;
                        let actual = parsed["report"].pointer(pointer).ok_or_else(unknown)?;
                        Ok(actual == equals)
                    }
                    _ => Err(unknown()),
                }
            }
        }
    }
    let passed = visit(&condition, results)?;
    // The explicit condition determines the result, including an expected
    // nonzero exit. Missing or incomplete execution evidence remains unknown.
    Ok(passed)
}

pub fn validate_command_evidence(
    value: &Value,
) -> Result<CommandExecutionEvidence, ExecutionIssue> {
    let evidence: CommandExecutionEvidence =
        serde_json::from_value(value.clone()).map_err(|_| unknown())?;
    if [
        &evidence.approved_sha256,
        &evidence.started_sha256,
        &evidence.finished_sha256,
    ]
    .iter()
    .any(|hash| !crate::protocol::valid_sha256(hash))
        || !crate::derivation::identity::runtime_relative_path(&evidence.receipt_dir)
    {
        return Err(unknown());
    }
    Ok(evidence)
}

/// Stored provenance remains readable after engine changes; eligibility is checked separately.
pub fn validate_carried_evidence_shape(
    attempt: &Value,
    carried: &Value,
) -> Result<work_model::execution::attempt::CarriedValidationEvidence, ExecutionIssue> {
    use work_model::execution::attempt::{RecordKind, ValidationOutcome};
    let evidence: work_model::execution::attempt::CarriedValidationEvidence =
        serde_json::from_value(carried["validation_evidence"].clone()).map_err(|_| unknown())?;
    let proof = evidence
        .record
        .verification_evidence
        .as_ref()
        .ok_or_else(unknown)?;
    let source = carried["source_attempt_id"].as_str().ok_or_else(unknown)?;
    let current = attempt["attempt_id"].as_str().ok_or_else(unknown)?;
    let (_, base, _) =
        super::attempt::record_identity(&json!(evidence.record.id), "carried validation record")?;
    if evidence.record.source
        != work_model::execution::acceptance::ValidationEvidenceSource::Procedure
        || !evidence
            .record
            .file_version
            .as_deref()
            .is_some_and(crate::protocol::valid_sha256)
        || !crate::protocol::valid_sha256(&evidence.source_attempt_sha256)
        || !crate::protocol::valid_sha256(&evidence.owner_attempt_sha256)
        || !crate::protocol::valid_sha256(&proof.engine_sha256)
        || !crate::protocol::valid_sha256(&proof.condition_sha256)
        || crate::task::item::numbered(&evidence.owner_attempt_id, "ATTEMPT").is_none()
        || crate::task::item::numbered(source, "ATTEMPT").is_none()
        || crate::task::item::numbered(current, "ATTEMPT").is_none()
        || evidence.owner_attempt_id.as_str() > source
        || source >= current
        || (evidence.owner_attempt_id == source
            && evidence.owner_attempt_sha256 != evidence.source_attempt_sha256)
        || evidence.record.kind != RecordKind::Validation
        || evidence.record.outcome != ValidationOutcome::Passed
        || evidence.record.evidence.trim().is_empty()
        || evidence.record.id != carried["record_id"]
        || !base.starts_with("VAL-")
        || proof.task_id != attempt["task_id"]
        || proof.attempt_id != evidence.owner_attempt_id
        || proof.record_id != evidence.record.id
        || !proof.passed
        || proof.commands.is_empty()
    {
        return Err(unknown());
    }
    for (command, selected) in &proof.commands {
        super::attempt::record_identity(
            &json!(selected.execution.record_id),
            "carried command record",
        )?;
        let execution = validate_command_evidence(
            &serde_json::to_value(&selected.execution).map_err(|_| unknown())?,
        )?;
        if crate::task::item::numbered(command, "CMD").is_none()
            || execution.task_id != attempt["task_id"]
            || execution.record_id.split('#').next() != Some(command.as_str())
            || crate::task::item::numbered(&execution.attempt_id, "ATTEMPT").is_none()
            || crate::task::item::numbered(&selected.selected_from_attempt_id, "ATTEMPT").is_none()
            || execution.attempt_id > evidence.owner_attempt_id
            || execution.attempt_id > selected.selected_from_attempt_id
            || selected.selected_from_attempt_id > evidence.owner_attempt_id
        {
            return Err(unknown());
        }
    }
    Ok(evidence)
}

pub fn validate_carried_validation_evidence(
    validation: &Value,
    attempt: &Value,
    carried: &Value,
) -> Result<work_model::execution::attempt::CarriedValidationEvidence, ExecutionIssue> {
    let evidence = validate_carried_evidence_shape(attempt, carried)?;
    let proof = evidence
        .record
        .verification_evidence
        .as_ref()
        .ok_or_else(unknown)?;
    let condition = procedure_condition(validation)?;
    let declared = validation["command_ids"]
        .as_array()
        .ok_or_else(unknown)?
        .iter()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    if validation["kind"] != "automated"
        || proof.record_id.split('#').next() != validation["id"].as_str()
        || proof.engine_sha256 != engine_sha256()
        || proof.condition_sha256
            != crate::derivation::fingerprint::structured(&condition).map_err(|_| unknown())?
        || proof
            .commands
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            != declared
    {
        return Err(unknown());
    }
    for (command, selected) in &proof.commands {
        // A current retry or skip replaces the old execution that this VAL proved.
        if attempt["records"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|row| {
                row["kind"] == "command"
                    && row["id"]
                        .as_str()
                        .is_some_and(|id| id.split('#').next() == Some(command.as_str()))
            })
        {
            return Err(unknown());
        }
        let current = attempt["carried_records"]
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
        if current.is_some_and(|row| row["record_id"] != selected.execution.record_id) {
            return Err(unknown());
        }
    }
    Ok(evidence)
}

pub fn validate_validation_evidence(
    validation: &Value,
    attempt: &Value,
    record: &Value,
) -> Result<(), ExecutionIssue> {
    let condition = procedure_condition(validation)?;
    let evidence: ValidationExecutionEvidence =
        serde_json::from_value(record["verification_evidence"].clone()).map_err(|_| unknown())?;
    let declared = validation["command_ids"]
        .as_array()
        .ok_or_else(unknown)?
        .iter()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    if evidence.task_id != attempt["task_id"]
        || evidence.attempt_id != attempt["attempt_id"]
        || record["id"] != evidence.record_id
        || evidence.record_id.split('#').next() != validation["id"].as_str()
        || evidence.engine_sha256 != engine_sha256()
        || evidence.condition_sha256
            != crate::derivation::fingerprint::structured(&condition).map_err(|_| unknown())?
        || evidence.passed != (record["outcome"] == "passed")
        || evidence
            .commands
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            != declared
    {
        return Err(unknown());
    }
    for (command, selected) in &evidence.commands {
        let proof = validate_command_evidence(
            &serde_json::to_value(&selected.execution).map_err(|_| unknown())?,
        )?;
        if proof.task_id != attempt["task_id"]
            || proof.record_id.split('#').next() != Some(command.as_str())
        {
            return Err(unknown());
        }
        let latest = attempt["records"]
            .as_array()
            .into_iter()
            .flatten()
            .rev()
            .find(|row| {
                row["kind"] == "command"
                    && row["id"]
                        .as_str()
                        .is_some_and(|id| id.split('#').next() == Some(command))
            });
        if let Some(latest) = latest {
            if selected.selected_from_attempt_id != attempt["attempt_id"]
                || proof.attempt_id != attempt["attempt_id"]
                || latest["id"] != proof.record_id
                || latest["status"] == "skipped"
                || latest["exit_code"] != proof.exit_code
                || latest["execution_evidence"]
                    != serde_json::to_value(&proof).map_err(|_| unknown())?
            {
                return Err(unknown());
            }
        } else if !attempt["carried_records"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|row| {
                row["record_id"] == proof.record_id
                    && row["source_attempt_id"] == selected.selected_from_attempt_id
            })
        {
            return Err(unknown());
        }
    }
    Ok(())
}

/// Structural unit-test packets only; this helper is not compiled into any executor.
#[cfg(test)]
pub(crate) fn user_record_fixture(
    attempt: &Value,
    validation: &Value,
    id: &str,
    outcome: &str,
    statement: &str,
) -> Value {
    use work_model::execution::acceptance::*;
    let document = UserConfirmationDocument {
        source: UserConfirmationOrigin::UserDocument,
        source_reference: format!("synthetic-fixture-user/{id}"),
        confirmer: validation["confirmer"].as_str().unwrap_or("user").into(),
        task_id: attempt["task_id"].as_str().unwrap_or("TASK-001").into(),
        attempt_id: attempt["attempt_id"]
            .as_str()
            .unwrap_or("ATTEMPT-001")
            .into(),
        validation_id: id.split('#').next().unwrap().into(),
        record_id: id.into(),
        file_version: "a".repeat(64),
        outcome: serde_json::from_value(json!(outcome)).unwrap(),
        statement: statement.into(),
    };
    let raw = serde_json::to_vec(&document).unwrap();
    let proof = UserConfirmationEvidence {
        reference: EvidenceArtifact {
            path: "fixtures/user-statement.json".into(),
            sha256: crate::derivation::fingerprint::raw(&raw),
        },
        document,
        raw,
    };
    json!({"id":id,"kind":"validation","source":"user_confirmation","file_version":"a".repeat(64),
        "outcome":outcome,"evidence":statement,"confirmation_evidence":proof})
}
#[cfg(test)]
pub(crate) fn user_finish_fixture(
    attempt: &Value,
    validation: &Value,
    id: &str,
    outcome: &str,
    statement: &str,
) -> Value {
    let mut record = user_record_fixture(attempt, validation, id, outcome, statement);
    record.as_object_mut().unwrap().remove("id");
    record.as_object_mut().unwrap().remove("kind");
    json!({"schema":"work-record-finish-request","record":record})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_methods_define_sources_independently_of_actor_names() {
        for (kind, source) in [
            ("automated", "procedure"),
            ("ai_review", "ai_review"),
            ("manual", "user_confirmation"),
        ] {
            let validation = json!({"kind":kind,"confirmer":"AI user or arbitrary actor name"});
            assert_eq!(json!(validation_source(&validation).unwrap()), source);
        }
        assert!(validation_source(&json!({"kind":"unknown"})).is_err());
    }
    #[test]
    fn default_procedure_requires_all_actual_zero_exits_and_explicit_nonzero_is_preserved() {
        let validation = json!({"kind":"automated","command_ids":["CMD-001","CMD-002"]});
        let condition = procedure_condition(&validation).unwrap();
        let mut results = BTreeMap::new();
        let observed = |exit_code| VerifiedCommandResult {
            exit_code,
            stdout: vec![],
            stderr: vec![],
            reports: BTreeMap::new(),
        };
        results.insert("CMD-001".into(), observed(0));
        assert!(evaluate(&condition, &validation["command_ids"], &results).is_err());
        results.insert("CMD-002".into(), observed(7));
        assert!(!evaluate(&condition, &validation["command_ids"], &results).unwrap());
        results.insert("CMD-002".into(), observed(0));
        assert!(evaluate(&condition, &validation["command_ids"], &results).unwrap());
        let explicit = json!({"kind":"automated","command_ids":["CMD-002"],"verification":{"kind":"exit_code","command":"CMD-002","equals":7}});
        results.insert("CMD-002".into(), observed(7));
        assert!(
            evaluate(
                &procedure_condition(&explicit).unwrap(),
                &explicit["command_ids"],
                &results
            )
            .unwrap()
        );
    }
    #[test]
    fn ai_review_requires_declared_reviewer_concrete_refs_and_no_user_or_procedure_proof() {
        let validation = json!({"id":"VAL-001","kind":"ai_review","confirmer":"user"});
        let attempt = json!({"task_id":"TASK-001","attempt_id":"ATTEMPT-001"});
        let record = json!({"id":"VAL-001","source":"ai_review","file_version":"a".repeat(64),
            "outcome":"passed","evidence":"Reviewed the specific output bytes",
            "review_evidence":{"reviewer":"user","rationale":"Reviewed the specific output bytes",
                "references":[{"kind":"artifact","path":"src/example.rs","sha256":"b".repeat(64),"detail":"Checked full implementation"}]}});
        validate_record_provenance(&validation, &attempt, &record).unwrap();
        for (pointer, value) in [
            ("/source", json!("user_confirmation")),
            ("/review_evidence/reviewer", json!("another")),
            ("/review_evidence/references", json!([])),
            ("/review_evidence/rationale", json!("")),
            ("/review_evidence/references/0/path", json!("../foreign")),
            ("/review_evidence/references/0/sha256", json!("not a hash")),
        ] {
            let mut bad = record.clone();
            *bad.pointer_mut(pointer).unwrap() = value;
            assert!(
                validate_record_provenance(&validation, &attempt, &bad).is_err(),
                "{pointer}"
            );
        }
        for field in ["verification_evidence", "confirmation_evidence"] {
            let mut bad = record.clone();
            bad[field] = json!({});
            assert!(validate_record_provenance(&validation, &attempt, &bad).is_err());
        }
        let mut user = validation;
        user["kind"] = json!("manual");
        assert!(validate_record_provenance(&user, &attempt, &record).is_err());
    }
    #[test]
    fn user_proof_freezes_actual_raw_bytes_and_exact_reserved_bindings() {
        let validation = json!({"id":"VAL-001","kind":"manual","confirmer":"designated-user"});
        let attempt = json!({"task_id":"TASK-001","attempt_id":"ATTEMPT-001"});
        let record = user_record_fixture(
            &attempt,
            &validation,
            "VAL-001#2",
            "passed",
            "Actual synthetic user statement",
        );
        validate_record_provenance(&validation, &attempt, &record).unwrap();
        for (field, value) in [
            ("source", json!("ai_review")),
            ("source_reference", json!("")),
            ("confirmer", json!("AI reviewer")),
            ("task_id", json!("TASK-002")),
            ("attempt_id", json!("ATTEMPT-002")),
            ("validation_id", json!("VAL-002")),
            ("record_id", json!("VAL-001#1")),
            ("file_version", json!("0".repeat(64))),
            ("outcome", json!("failed")),
            ("statement", json!("")),
        ] {
            let mut bad = record.clone();
            bad["confirmation_evidence"]["document"][field] = value;
            let raw = serde_json::to_vec(&bad["confirmation_evidence"]["document"]).unwrap();
            bad["confirmation_evidence"]["raw"] = json!(raw);
            bad["confirmation_evidence"]["reference"]["sha256"] =
                json!(crate::derivation::fingerprint::raw(&raw));
            assert!(
                validate_record_provenance(&validation, &attempt, &bad).is_err(),
                "{field}"
            );
        }
        let mut bad = record.clone();
        bad["confirmation_evidence"]["raw"] = json!(b"different source".to_vec());
        assert!(validate_record_provenance(&validation, &attempt, &bad).is_err());
        let bare = json!({"id":"VAL-001","source":"user_confirmation","file_version":"a".repeat(64),"outcome":"passed","evidence":"AI declares user approved"});
        assert!(validate_record_provenance(&validation, &attempt, &bare).is_err());
    }

    #[test]
    fn carried_validation_shape_preserves_history_and_binds_actual_record_ownership() {
        let attempt = json!({"task_id":"TASK-001","attempt_id":"ATTEMPT-003"});
        // Synthetic contract data only; it cannot authorize a native execution.
        let carried = json!({"source_attempt_id":"ATTEMPT-002","record_id":"VAL-001#2","evidence":"Reviewed continuation",
            "validation_evidence":{"source_attempt_sha256":"a".repeat(64),
                "owner_attempt_id":"ATTEMPT-001","owner_attempt_sha256":"b".repeat(64),
                "record":{"id":"VAL-001#2","kind":"validation","source":"procedure","file_version":"a".repeat(64),"outcome":"passed","evidence":"Actual source result",
                    "verification_evidence":{"task_id":"TASK-001","attempt_id":"ATTEMPT-001","record_id":"VAL-001#2",
                        "engine_sha256":"c".repeat(64),"condition_sha256":"d".repeat(64),"passed":true,
                        "commands":{"CMD-001":{"selected_from_attempt_id":"ATTEMPT-001","execution":{
                            "task_id":"TASK-001","attempt_id":"ATTEMPT-001","record_id":"CMD-001",
                            "receipt_dir":"executions/TASK-001/ATTEMPT-001/receipts/CMD-001",
                            "approved_sha256":"e".repeat(64),"started_sha256":"f".repeat(64),"finished_sha256":"1".repeat(64),
                            "exit_code":0}}}}}}});
        let proof = validate_carried_evidence_shape(&attempt, &carried).unwrap();
        assert_ne!(
            proof.record.verification_evidence.unwrap().engine_sha256,
            engine_sha256()
        );
        for (pointer, value) in [
            ("/source_attempt_id", json!("ATTEMPT-003")),
            (
                "/validation_evidence/source_attempt_sha256",
                json!("not a hash"),
            ),
            (
                "/validation_evidence/owner_attempt_id",
                json!("ATTEMPT-003"),
            ),
            ("/validation_evidence/record/id", json!("VAL-002")),
            ("/validation_evidence/record/kind", json!("command")),
            ("/validation_evidence/record/status", json!("skipped")),
            ("/validation_evidence/record/outcome", json!("failed")),
            (
                "/validation_evidence/record/verification_evidence/task_id",
                json!("TASK-002"),
            ),
            (
                "/validation_evidence/record/verification_evidence/attempt_id",
                json!("ATTEMPT-002"),
            ),
            (
                "/validation_evidence/record/verification_evidence/passed",
                json!(false),
            ),
            (
                "/validation_evidence/record/verification_evidence/commands",
                json!({}),
            ),
        ] {
            let mut forged = carried.clone();
            if pointer.ends_with("/status") {
                forged["validation_evidence"]["record"]["status"] = value;
            } else {
                *forged.pointer_mut(pointer).unwrap() = value;
            }
            assert!(
                validate_carried_evidence_shape(&attempt, &forged).is_err(),
                "{pointer}"
            );
        }
        let legacy = json!({"source_attempt_id":"ATTEMPT-001","record_id":"VAL-001","evidence":"Historical carry"});
        let typed: work_model::execution::attempt::CarriedRecord =
            serde_json::from_value(legacy.clone()).unwrap();
        assert_eq!(serde_json::to_value(typed).unwrap(), legacy);
        assert!(validate_carried_evidence_shape(&attempt, &legacy).is_err());
        let condition = json!({"kind":"exit_code","command":"CMD-001","equals":0});
        let validation = json!({"id":"VAL-001","kind":"automated","command_ids":["CMD-001"],"verification":condition});
        assert!(validate_carried_validation_evidence(&validation, &attempt, &carried).is_err());
        let mut eligible = carried.clone();
        eligible["validation_evidence"]["record"]["verification_evidence"]["engine_sha256"] =
            json!(engine_sha256());
        eligible["validation_evidence"]["record"]["verification_evidence"]["condition_sha256"] =
            json!(crate::derivation::fingerprint::structured(&condition).unwrap());
        validate_carried_validation_evidence(&validation, &attempt, &eligible).unwrap();
        let mut retry = attempt.clone();
        retry["records"] = json!([{"id":"CMD-001#2","kind":"command","status":"skipped"}]);
        assert!(validate_carried_validation_evidence(&validation, &retry, &eligible).is_err());
        retry["records"] = json!([]);
        retry["carried_records"] = json!([{"record_id":"CMD-001#2"}]);
        assert!(validate_carried_validation_evidence(&validation, &retry, &eligible).is_err());
        let mut changed = validation.clone();
        changed["verification"]["equals"] = json!(7);
        assert!(validate_carried_validation_evidence(&changed, &attempt, &eligible).is_err());
        let mut forged = carried;
        forged["source_attempt_id"] = json!("ATTEMPT-001");
        assert!(validate_carried_evidence_shape(&attempt, &forged).is_err());
        forged["validation_evidence"]["source_attempt_sha256"] = json!("b".repeat(64));
        validate_carried_evidence_shape(&attempt, &forged).unwrap();
        forged["validation_evidence"]["record"]["id"] = json!("VAL-001#02");
        forged["record_id"] = json!("VAL-001#02");
        forged["validation_evidence"]["record"]["verification_evidence"]["record_id"] =
            json!("VAL-001#02");
        assert!(validate_carried_evidence_shape(&attempt, &forged).is_err());
    }
    #[test]
    fn validation_evidence_binds_task_attempt_condition_latest_record_and_engine() {
        use work_model::execution::command::capture::ValidationCommandEvidence;
        let condition = json!({"kind":"exit_code","command":"CMD-001","equals":0});
        let command = CommandExecutionEvidence {
            task_id: "TASK-001".into(),
            attempt_id: "ATTEMPT-001".into(),
            record_id: "CMD-001".into(),
            receipt_dir: "executions/TASK-001/ATTEMPT-001/receipts/CMD-001".into(),
            approved_sha256: "a".repeat(64),
            started_sha256: "b".repeat(64),
            finished_sha256: "c".repeat(64),
            exit_code: 0,
        };
        let attempt = json!({"task_id":"TASK-001","attempt_id":"ATTEMPT-001","records":[
            {"id":"CMD-001","kind":"command","exit_code":0,"execution_evidence":command}]});
        let validation = json!({"id":"VAL-001","kind":"automated","command_ids":["CMD-001"],"verification":condition});
        let evidence = ValidationExecutionEvidence {
            task_id: "TASK-001".into(),
            attempt_id: "ATTEMPT-001".into(),
            record_id: "VAL-001".into(),
            engine_sha256: engine_sha256(),
            condition_sha256: crate::derivation::fingerprint::structured(&condition).unwrap(),
            passed: true,
            commands: BTreeMap::from([(
                "CMD-001".into(),
                ValidationCommandEvidence {
                    selected_from_attempt_id: "ATTEMPT-001".into(),
                    execution: command,
                },
            )]),
        };
        let record = json!({"id":"VAL-001","outcome":"passed","verification_evidence":evidence});
        validate_validation_evidence(&validation, &attempt, &record).unwrap();
        for (field, wrong) in [
            ("task_id", json!("TASK-002")),
            ("attempt_id", json!("ATTEMPT-002")),
            ("record_id", json!("VAL-002")),
            ("engine_sha256", json!("0".repeat(64))),
            ("condition_sha256", json!("0".repeat(64))),
            ("passed", json!(false)),
        ] {
            let mut forged = record.clone();
            forged["verification_evidence"][field] = wrong;
            assert!(
                validate_validation_evidence(&validation, &attempt, &forged).is_err(),
                "{field}"
            );
        }
        let mut retry = attempt.clone();
        retry["records"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"CMD-001#2","kind":"command","exit_code":1}));
        assert!(validate_validation_evidence(&validation, &retry, &record).is_err());
        let mut legacy = validation.clone();
        legacy.as_object_mut().unwrap().remove("verification");
        assert!(validate_validation_evidence(&legacy, &attempt, &record).is_err());
        let mut forged = record;
        forged["verification_evidence"]["commands"]["CMD-001"]["selected_from_attempt_id"] =
            json!("ATTEMPT-002");
        assert!(validate_validation_evidence(&validation, &attempt, &forged).is_err());
    }
    #[test]
    fn unknown_is_not_hidden_by_not_any_or_all_and_real_results_drive_outcome() {
        let mut results = BTreeMap::from([(
            "CMD-001".into(),
            VerifiedCommandResult {
                exit_code: 0,
                stdout: b"actual complete output".to_vec(),
                stderr: vec![],
                reports: BTreeMap::from([(
                    "report.json".into(),
                    Some(b"{\"passed\":true}".to_vec()),
                )]),
            },
        )]);
        let exit = json!({"kind":"exit_code","command":"CMD-001","equals":0});
        let missing = json!({"kind":"json_report_equals","command":"CMD-001","path":"missing.json","pointer":"/passed","equals":true});
        for condition in [
            json!({"kind":"not","condition":missing}),
            json!({"kind":"any","conditions":[exit,missing]}),
            json!({"kind":"all","conditions":[{"kind":"not","condition":exit},missing]}),
        ] {
            assert!(evaluate(&condition, &json!(["CMD-001"]), &results).is_err());
        }
        let report = json!({"kind":"json_report_equals","command":"CMD-001","path":"report.json","pointer":"/passed","equals":true});
        assert!(evaluate(&report, &json!(["CMD-001"]), &results).unwrap());
        results.get_mut("CMD-001").unwrap().reports.insert(
            "report.json".into(),
            Some(b"{\"passed\":false,\"passed\":true}".to_vec()),
        );
        assert!(evaluate(&report, &json!(["CMD-001"]), &results).is_err());
        results.get_mut("CMD-001").unwrap().exit_code = 7;
        let failure = json!({"kind":"exit_code","command":"CMD-001","equals":7});
        assert!(evaluate(&failure, &json!(["CMD-001"]), &results).unwrap());
        assert!(evaluate(&exit, &json!(["CMD-001"]), &BTreeMap::new()).is_err());
    }
}
