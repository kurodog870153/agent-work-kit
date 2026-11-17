//! Use-case tests of TASK, Execution, Acceptance, Revise and traceability on
//! the in-memory store.

use work_model::{
    AttemptInput, AttemptResult, ConclusionInput, Criterion, CriterionStatus,
    EXECUTION_RECORD_SCHEMA, ExecutionRecordRequest, Origin, OriginSource,
    REQUIREMENT_ACCEPTANCE_RECORD_SCHEMA, Reason, RecordKind, RecordRef,
    RequirementAcceptanceRecordRequest, TASK_ACCEPTANCE_RECORD_SCHEMA, TASK_APPLY_SCHEMA,
    TASK_COMPLETE_SCHEMA, TASK_CREATE_SCHEMA, TASK_PREVIEW_SCHEMA, TASK_REVISE_SCHEMA,
    TaskAcceptanceRecordRequest, TaskApplyRequest, TaskCompleteRequest, TaskCreateRequest,
    TaskDraft, TaskPreviewRequest, TaskReviseRequest, TaskStatus, Verdict,
};

use crate::memory::{MemoryStore, discussion, id, source};
use crate::*;

// Test helpers exercise the same mandatory review-and-approval path as the CLI.
fn approved(store: &MemoryStore, draft: TaskDraft) -> Result<RecordRef, work_model::WorkError> {
    let (candidate, reviewed_dependencies) = task_preview(
        store,
        TaskPreviewRequest {
            schema: TASK_PREVIEW_SCHEMA.to_owned(),
            draft: draft.clone(),
        },
    )?;
    task_apply(
        store,
        TaskApplyRequest {
            schema: TASK_APPLY_SCHEMA.to_owned(),
            draft,
            approved_candidate: candidate,
            reviewed_dependencies,
        },
    )
}

fn create_task(
    store: &MemoryStore,
    request: TaskCreateRequest,
) -> Result<RecordRef, work_model::WorkError> {
    approved(store, TaskDraft::Create(request))
}

fn revise_task(
    store: &MemoryStore,
    request: TaskReviseRequest,
) -> Result<RecordRef, work_model::WorkError> {
    approved(store, TaskDraft::Revise(request))
}

fn criterion(name: &str, text: &str) -> Criterion {
    Criterion {
        id: name.to_owned(),
        text: text.to_owned(),
        required: true,
    }
}

fn task(name: &str, discussions: &[&str], sources: &[&str]) -> TaskCreateRequest {
    TaskCreateRequest {
        schema: TASK_CREATE_SCHEMA.to_owned(),
        id: id(name),
        title: "Export".to_owned(),
        goal: "Add export".to_owned(),
        requirements: vec!["CSV".to_owned()],
        constraints: Vec::new(),
        decisions: Vec::new(),
        criteria: vec![criterion("c1", "csv"), criterion("c2", "pdf")],
        discussion_ids: discussions.iter().map(|v| id(v)).collect(),
        source_ids: sources.iter().map(|v| id(v)).collect(),
        notes: Vec::new(),
    }
}

fn revise(name: &str, version: u32) -> TaskReviseRequest {
    TaskReviseRequest {
        schema: TASK_REVISE_SCHEMA.to_owned(),
        id: id(name),
        expected_version: version,
        reason: "scope moved".to_owned(),
        title: None,
        goal: None,
        requirements: None,
        constraints: None,
        decisions: None,
        criteria: None,
        notes: None,
        link_discussions: Vec::new(),
        link_sources: Vec::new(),
        reconfirm_criteria: Vec::new(),
        reconfirm_requirement: false,
    }
}

fn complete(name: &str, version: u32) -> TaskCompleteRequest {
    TaskCompleteRequest {
        schema: TASK_COMPLETE_SCHEMA.to_owned(),
        id: id(name),
        expected_version: version,
    }
}

fn origin(source: OriginSource) -> Origin {
    Origin {
        source,
        reference: None,
    }
}

fn judge(
    name: &str,
    task_name: &str,
    task_version: u32,
    expected: Option<u32>,
    items: &[(&str, Verdict)],
) -> TaskAcceptanceRecordRequest {
    TaskAcceptanceRecordRequest {
        schema: TASK_ACCEPTANCE_RECORD_SCHEMA.to_owned(),
        id: id(name),
        task_id: id(task_name),
        task_version,
        expected_version: expected,
        conclusions: items
            .iter()
            .map(|(criterion, verdict)| ConclusionInput {
                criterion_id: (*criterion).to_owned(),
                verdict: *verdict,
                evidence: "checked".to_owned(),
                origin: origin(OriginSource::Host),
            })
            .collect(),
    }
}

fn attempt(
    name: &str,
    task_name: &str,
    expected: Option<u32>,
    attempt: &str,
) -> ExecutionRecordRequest {
    ExecutionRecordRequest {
        schema: EXECUTION_RECORD_SCHEMA.to_owned(),
        id: id(name),
        task_id: id(task_name),
        expected_version: expected,
        attempt: AttemptInput {
            id: attempt.to_owned(),
            result: AttemptResult::Succeeded,
            changes: vec!["edited".to_owned()],
            anomalies: Vec::new(),
            verification: vec!["cargo test".to_owned()],
            evidence: Vec::new(),
        },
    }
}

fn requirement(
    name: &str,
    discussion: &str,
    expected: Option<u32>,
    refs: Vec<RecordRef>,
) -> RequirementAcceptanceRecordRequest {
    RequirementAcceptanceRecordRequest {
        schema: REQUIREMENT_ACCEPTANCE_RECORD_SCHEMA.to_owned(),
        id: id(name),
        discussion_id: id(discussion),
        expected_version: expected,
        verdict: Verdict::Passed,
        evidence: "user confirmed".to_owned(),
        origin: origin(OriginSource::User),
        task_refs: refs,
    }
}

fn task_ref(name: &str, version: u32) -> RecordRef {
    RecordRef {
        kind: RecordKind::Task,
        id: id(name),
        version,
    }
}

fn statuses(store: &MemoryStore, name: &str) -> Vec<CriterionStatus> {
    show_task(store, &id(name), None)
        .unwrap()
        .criteria
        .into_iter()
        .map(|state| state.status)
        .collect()
}

fn reason<T>(result: Result<T, work_model::WorkError>) -> Reason {
    match result {
        Ok(_) => panic!("expected a failure"),
        Err(error) => error.reason,
    }
}

#[test]
fn tasks_link_existing_records_and_one_discussion_serves_many_tasks() {
    let store = MemoryStore::default();
    assert_eq!(
        reason(create_task(&store, task("t1", &["d1"], &[]))),
        Reason::NotFound
    );
    create_source(&store, source("s1")).unwrap();
    create_discussion(&store, discussion("d1", &["s1"])).unwrap();
    create_task(&store, task("t1", &["d1"], &["s1"])).unwrap();
    create_task(&store, task("t2", &["d1"], &[])).unwrap();
    assert_eq!(
        reason(create_task(&store, task("t1", &[], &[]))),
        Reason::AlreadyExists
    );
    let mut wrong = task("t3", &[], &[]);
    wrong.schema = "other".to_owned();
    assert_eq!(reason(create_task(&store, wrong)), Reason::InvalidRequest);
    let view = show_task(&store, &id("t1"), None).unwrap();
    assert_eq!(view.record.status, TaskStatus::Open);
    assert!(!view.completable);
}

#[test]
fn a_task_completes_only_after_every_required_criterion_passes() {
    let store = MemoryStore::default();
    create_task(&store, task("t1", &[], &[])).unwrap();
    assert_eq!(
        reason(complete_task(&store, complete("t1", 1))),
        Reason::RuleViolation
    );

    record_task_acceptance(
        &store,
        judge("a1", "t1", 1, None, &[("c1", Verdict::Passed)]),
    )
    .unwrap();
    assert_eq!(
        reason(complete_task(&store, complete("t1", 1))),
        Reason::RuleViolation
    );
    assert_eq!(
        statuses(&store, "t1"),
        [CriterionStatus::Passed, CriterionStatus::Missing]
    );

    record_task_acceptance(
        &store,
        judge("a1", "t1", 1, Some(1), &[("c2", Verdict::Failed)]),
    )
    .unwrap();
    assert_eq!(
        reason(complete_task(&store, complete("t1", 1))),
        Reason::RuleViolation
    );
    record_task_acceptance(
        &store,
        judge("a1", "t1", 1, Some(2), &[("c2", Verdict::Passed)]),
    )
    .unwrap();
    assert!(show_task(&store, &id("t1"), None).unwrap().completable);

    assert_eq!(
        reason(complete_task(&store, complete("t1", 9))),
        Reason::VersionConflict
    );
    complete_task(&store, complete("t1", 1)).unwrap();
    let view = show_task(&store, &id("t1"), None).unwrap();
    assert_eq!(
        (view.latest_version, view.record.status),
        (2, TaskStatus::Completed)
    );
    assert_eq!(
        reason(complete_task(&store, complete("t1", 2))),
        Reason::RuleViolation
    );
}

#[test]
fn execution_results_and_task_completion_never_pass_acceptance_by_themselves() {
    let store = MemoryStore::default();
    create_discussion(&store, discussion("d1", &[])).unwrap();
    create_task(&store, task("t1", &["d1"], &[])).unwrap();
    record_execution(&store, attempt("e1", "t1", None, "a1")).unwrap();
    assert_eq!(
        reason(complete_task(&store, complete("t1", 1))),
        Reason::RuleViolation
    );

    record_task_acceptance(
        &store,
        judge(
            "a1",
            "t1",
            1,
            None,
            &[("c1", Verdict::Passed), ("c2", Verdict::Passed)],
        ),
    )
    .unwrap();
    complete_task(&store, complete("t1", 1)).unwrap();

    // Completing every task does not create or pass a Requirement Acceptance.
    assert!(
        status(&store)
            .unwrap()
            .iter()
            .all(|item| item.kind != RecordKind::RequirementAcceptance)
    );
    record_requirement_acceptance(
        &store,
        requirement("r1", "d1", None, vec![task_ref("t1", 2)]),
    )
    .unwrap();
    let view = show_requirement_acceptance(&store, &id("r1"), None).unwrap();
    assert_eq!(view.state.verdict, Some(Verdict::Passed));
    assert!(!view.state.needs_reconfirm);
}

#[test]
fn acceptance_records_are_unique_per_subject_and_validate_references() {
    let store = MemoryStore::default();
    create_discussion(&store, discussion("d1", &[])).unwrap();
    create_task(&store, task("t1", &["d1"], &[])).unwrap();
    create_task(&store, task("t2", &[], &[])).unwrap();

    record_task_acceptance(
        &store,
        judge("a1", "t1", 1, None, &[("c1", Verdict::Passed)]),
    )
    .unwrap();
    assert_eq!(
        reason(record_task_acceptance(
            &store,
            judge("a2", "t1", 1, None, &[("c1", Verdict::Passed)])
        )),
        Reason::AlreadyExists
    );
    assert_eq!(
        reason(record_task_acceptance(
            &store,
            judge("a3", "t2", 4, None, &[("c1", Verdict::Passed)])
        )),
        Reason::VersionConflict
    );
    assert_eq!(
        reason(record_task_acceptance(
            &store,
            judge("a3", "t2", 1, None, &[("zz", Verdict::Passed)])
        )),
        Reason::RuleViolation
    );
    assert_eq!(
        reason(record_task_acceptance(
            &store,
            judge("ghost", "t1", 1, Some(1), &[("c1", Verdict::Passed)])
        )),
        Reason::NotFound
    );

    record_requirement_acceptance(
        &store,
        requirement("r1", "d1", None, vec![task_ref("t1", 1)]),
    )
    .unwrap();
    assert_eq!(
        reason(record_requirement_acceptance(
            &store,
            requirement("r2", "d1", None, Vec::new())
        )),
        Reason::AlreadyExists
    );
    assert_eq!(
        reason(record_requirement_acceptance(
            &store,
            requirement("r3", "ghost", None, Vec::new())
        )),
        Reason::NotFound
    );
    create_discussion(&store, discussion("d2", &[])).unwrap();
    assert_eq!(
        reason(record_requirement_acceptance(
            &store,
            requirement("r3", "d2", None, vec![task_ref("t1", 1)])
        )),
        Reason::RuleViolation
    );
    assert_eq!(
        reason(record_requirement_acceptance(
            &store,
            requirement("r3", "d1", Some(1), vec![task_ref("t1", 7)])
        )),
        Reason::NotFound
    );
}

#[test]
fn revising_a_task_reconfirms_only_what_it_affects_and_keeps_history() {
    let store = MemoryStore::default();
    create_discussion(&store, discussion("d1", &[])).unwrap();
    create_task(&store, task("t1", &["d1"], &[])).unwrap();
    record_task_acceptance(
        &store,
        judge(
            "a1",
            "t1",
            1,
            None,
            &[("c1", Verdict::Passed), ("c2", Verdict::Passed)],
        ),
    )
    .unwrap();
    complete_task(&store, complete("t1", 1)).unwrap();
    record_requirement_acceptance(
        &store,
        requirement("r1", "d1", None, vec![task_ref("t1", 2)]),
    )
    .unwrap();

    let mut change = revise("t1", 2);
    change.criteria = Some(vec![
        criterion("c1", "csv"),
        criterion("c2", "pdf and xlsx"),
    ]);
    change.reconfirm_requirement = true;
    revise_task(&store, change).unwrap();

    let view = show_task(&store, &id("t1"), None).unwrap();
    assert_eq!(
        (view.latest_version, view.record.status),
        (3, TaskStatus::Open)
    );
    assert!(!view.completable);
    assert_eq!(
        statuses(&store, "t1"),
        [CriterionStatus::Passed, CriterionStatus::NeedsReconfirm]
    );
    assert_eq!(
        reason(complete_task(&store, complete("t1", 3))),
        Reason::RuleViolation
    );
    assert!(
        show_requirement_acceptance(&store, &id("r1"), None)
            .unwrap()
            .state
            .needs_reconfirm
    );

    // Published versions stay readable exactly as they were.
    let old = show_task(&store, &id("t1"), Some(2)).unwrap();
    assert_eq!(old.record.criteria[1].text, "pdf");
    assert_eq!(old.record.status, TaskStatus::Completed);
    assert_eq!(old.criteria[1].status, CriterionStatus::Passed);
    assert!(show_task(&store, &id("t1"), Some(9)).is_err());

    record_task_acceptance(
        &store,
        judge("a1", "t1", 3, Some(1), &[("c2", Verdict::Passed)]),
    )
    .unwrap();
    complete_task(&store, complete("t1", 3)).unwrap();
    record_requirement_acceptance(
        &store,
        requirement("r1", "d1", Some(1), vec![task_ref("t1", 4)]),
    )
    .unwrap();
    assert!(
        !show_requirement_acceptance(&store, &id("r1"), None)
            .unwrap()
            .state
            .needs_reconfirm
    );

    assert_eq!(
        reason(revise_task(&store, revise("t1", 4))),
        Reason::InvalidRequest
    );
    assert_eq!(
        reason(revise_task(&store, revise("ghost", 1))),
        Reason::NotFound
    );
}

#[test]
fn an_independent_new_requirement_becomes_its_own_task() {
    let store = MemoryStore::default();
    create_task(&store, task("t1", &[], &[])).unwrap();
    record_task_acceptance(
        &store,
        judge(
            "a1",
            "t1",
            1,
            None,
            &[("c1", Verdict::Passed), ("c2", Verdict::Passed)],
        ),
    )
    .unwrap();
    complete_task(&store, complete("t1", 1)).unwrap();
    create_task(&store, task("t2", &[], &[])).unwrap();
    assert_eq!(
        show_task(&store, &id("t1"), None).unwrap().record.status,
        TaskStatus::Completed
    );
    assert_eq!(
        statuses(&store, "t2"),
        [CriterionStatus::Missing, CriterionStatus::Missing]
    );
}

#[test]
fn executions_keep_every_attempt_and_stay_tied_to_one_task() {
    let store = MemoryStore::default();
    assert_eq!(
        reason(record_execution(&store, attempt("e1", "ghost", None, "a1"))),
        Reason::NotFound
    );
    create_task(&store, task("t1", &[], &[])).unwrap();
    create_task(&store, task("t2", &[], &[])).unwrap();
    record_execution(&store, attempt("e1", "t1", None, "a1")).unwrap();
    let mut failed = attempt("e1", "t1", Some(1), "a2");
    failed.attempt.result = AttemptResult::Failed;
    failed.attempt.anomalies = vec!["timeout".to_owned()];
    record_execution(&store, failed).unwrap();

    let (record, newest, _) = show_execution(&store, &id("e1"), None).unwrap();
    assert_eq!((newest, record.attempts.len()), (2, 2));
    assert_eq!(
        show_execution(&store, &id("e1"), Some(1))
            .unwrap()
            .0
            .attempts
            .len(),
        1
    );
    assert_eq!(
        reason(record_execution(&store, attempt("e1", "t1", Some(1), "a3"))),
        Reason::VersionConflict
    );
    assert_eq!(
        reason(record_execution(&store, attempt("e1", "t2", Some(2), "a3"))),
        Reason::RuleViolation
    );
    assert_eq!(
        reason(record_execution(&store, attempt("e1", "t1", Some(2), "a2"))),
        Reason::RuleViolation
    );
    assert_eq!(
        reason(record_execution(&store, attempt("e1", "t1", None, "a1"))),
        Reason::VersionConflict
    );
}

#[test]
fn a_failed_commit_leaves_acceptance_unchanged() {
    let store = MemoryStore::default();
    create_task(&store, task("t1", &[], &[])).unwrap();
    *store.fail_commit.borrow_mut() = true;
    assert_eq!(
        reason(record_task_acceptance(
            &store,
            judge("a1", "t1", 1, None, &[("c1", Verdict::Passed)])
        )),
        Reason::StorageFailure
    );
    *store.fail_commit.borrow_mut() = false;
    assert!(
        show_task(&store, &id("t1"), None)
            .unwrap()
            .acceptance
            .is_none()
    );
    assert_eq!(
        statuses(&store, "t1"),
        [CriterionStatus::Missing, CriterionStatus::Missing]
    );
}

#[test]
fn trace_links_source_discussion_task_execution_and_acceptance() {
    let store = MemoryStore::default();
    create_source(&store, source("s1")).unwrap();
    create_discussion(&store, discussion("d1", &["s1"])).unwrap();
    create_task(&store, task("t1", &["d1"], &[])).unwrap();
    create_task(&store, task("t2", &[], &["s1"])).unwrap();
    create_task(&store, task("t3", &[], &[])).unwrap();
    record_execution(&store, attempt("e1", "t1", None, "a1")).unwrap();
    record_task_acceptance(
        &store,
        judge("a1", "t1", 1, None, &[("c1", Verdict::Passed)]),
    )
    .unwrap();
    record_requirement_acceptance(&store, requirement("r1", "d1", None, Vec::new())).unwrap();

    let from_source = trace(&store, RecordKind::Source, &id("s1")).unwrap();
    let task_ids: Vec<_> = from_source.tasks.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(task_ids, ["t1", "t2"]);
    assert_eq!(from_source.discussions, [id("d1")]);
    assert_eq!(from_source.requirement_acceptances, [id("r1")]);
    let first = &from_source.tasks[0];
    assert_eq!(first.executions[0].attempts, 1);
    assert_eq!(first.acceptance, Some(id("a1")));
    assert_eq!(first.criteria[0].status, CriterionStatus::Passed);

    for (kind, name) in [
        (RecordKind::Task, "t1"),
        (RecordKind::Execution, "e1"),
        (RecordKind::TaskAcceptance, "a1"),
    ] {
        let found = trace(&store, kind, &id(name)).unwrap();
        assert_eq!(found.sources, [id("s1")], "{name}");
        assert_eq!(found.discussions, [id("d1")], "{name}");
        assert_eq!(found.tasks.len(), 1, "{name}");
        assert_eq!(found.requirement_acceptances, [id("r1")], "{name}");
    }
    assert_eq!(
        trace(&store, RecordKind::Discussion, &id("d1"))
            .unwrap()
            .tasks
            .len(),
        1
    );
    let from_requirement = trace(&store, RecordKind::RequirementAcceptance, &id("r1")).unwrap();
    assert_eq!(from_requirement.tasks[0].id, id("t1"));
    assert_eq!(
        reason(trace(&store, RecordKind::Task, &id("ghost"))),
        Reason::NotFound
    );
    assert_eq!(
        reason(trace(&store, RecordKind::Correction, &id("x"))),
        Reason::NotFound
    );
}

#[test]
fn task_approval_rejects_missing_changed_or_stale_review() {
    let store = MemoryStore::default();
    create_source(&store, source("s1")).unwrap();
    create_discussion(&store, discussion("d1", &["s1"])).unwrap();
    let draft = TaskDraft::Create(task("t1", &["d1"], &["s1"]));
    let (candidate, dependencies) = task_preview(
        &store,
        TaskPreviewRequest {
            schema: TASK_PREVIEW_SCHEMA.to_owned(),
            draft: draft.clone(),
        },
    )
    .unwrap();
    assert_eq!(
        store.latest_version(RecordKind::Task, &id("t1")).unwrap(),
        None
    );
    let request = |content: String, reviewed: Vec<RecordRef>| TaskApplyRequest {
        schema: TASK_APPLY_SCHEMA.to_owned(),
        draft: draft.clone(),
        approved_candidate: content,
        reviewed_dependencies: reviewed,
    };

    assert_eq!(
        reason(task_apply(
            &store,
            request(String::new(), dependencies.clone())
        )),
        Reason::ApprovalMismatch
    );
    assert_eq!(
        reason(task_apply(
            &store,
            request(format!("{candidate}!"), dependencies.clone())
        )),
        Reason::ApprovalMismatch
    );
    assert_eq!(
        reason(task_apply(&store, request(candidate.clone(), Vec::new()))),
        Reason::VersionConflict
    );
    assert_eq!(
        store.latest_version(RecordKind::Task, &id("t1")).unwrap(),
        None
    );

    // All linked Discussion versions are part of the approval.
    let mut change = crate::memory::round("d1", 1);
    change.add_pending.push(work_model::NewPending {
        id: "p".to_owned(),
        text: "check".to_owned(),
    });
    discussion_round(&store, change).unwrap();
    assert_eq!(
        reason(task_apply(
            &store,
            request(candidate.clone(), dependencies.clone())
        )),
        Reason::VersionConflict
    );
    assert_eq!(
        store.latest_version(RecordKind::Task, &id("t1")).unwrap(),
        None
    );

    let (current, reviewed) = task_preview(
        &store,
        TaskPreviewRequest {
            schema: TASK_PREVIEW_SCHEMA.to_owned(),
            draft: draft.clone(),
        },
    )
    .unwrap();
    assert_eq!(current, candidate);
    let saved = task_apply(&store, request(current, reviewed)).unwrap();
    assert_eq!(saved.version, 1);
    assert_eq!(
        show_task(&store, &id("t1"), None).unwrap().record.version,
        1
    );
}

#[test]
fn revision_approval_rejects_changed_draft_task_and_acceptance() {
    let store = MemoryStore::default();
    create_task(&store, task("t1", &[], &[])).unwrap();
    let mut revision = revise("t1", 1);
    revision.goal = Some("Add CSV".to_owned());
    let draft = TaskDraft::Revise(revision.clone());
    let (candidate, deps) = task_preview(
        &store,
        TaskPreviewRequest {
            schema: TASK_PREVIEW_SCHEMA.to_owned(),
            draft: draft.clone(),
        },
    )
    .unwrap();
    assert_eq!(
        store.latest_version(RecordKind::Task, &id("t1")).unwrap(),
        Some(1)
    );

    let mut modified = revision.clone();
    modified.goal = Some("Unapproved scope".to_owned());
    assert_eq!(
        reason(task_apply(
            &store,
            TaskApplyRequest {
                schema: TASK_APPLY_SCHEMA.to_owned(),
                draft: TaskDraft::Revise(modified),
                approved_candidate: candidate.clone(),
                reviewed_dependencies: deps.clone(),
            }
        )),
        Reason::ApprovalMismatch
    );
    assert_eq!(
        store.latest_version(RecordKind::Task, &id("t1")).unwrap(),
        Some(1)
    );

    record_task_acceptance(
        &store,
        judge("a1", "t1", 1, None, &[("c1", Verdict::Passed)]),
    )
    .unwrap();
    assert_eq!(
        reason(task_apply(
            &store,
            TaskApplyRequest {
                schema: TASK_APPLY_SCHEMA.to_owned(),
                draft: draft.clone(),
                approved_candidate: candidate.clone(),
                reviewed_dependencies: deps.clone(),
            }
        )),
        Reason::VersionConflict
    );

    let (current, reviewed) = task_preview(
        &store,
        TaskPreviewRequest {
            schema: TASK_PREVIEW_SCHEMA.to_owned(),
            draft: draft.clone(),
        },
    )
    .unwrap();
    task_apply(
        &store,
        TaskApplyRequest {
            schema: TASK_APPLY_SCHEMA.to_owned(),
            draft: draft.clone(),
            approved_candidate: current,
            reviewed_dependencies: reviewed,
        },
    )
    .unwrap();
    assert_eq!(
        store.latest_version(RecordKind::Task, &id("t1")).unwrap(),
        Some(2)
    );
    assert_eq!(
        reason(task_apply(
            &store,
            TaskApplyRequest {
                schema: TASK_APPLY_SCHEMA.to_owned(),
                draft,
                approved_candidate: candidate,
                reviewed_dependencies: deps,
            }
        )),
        Reason::VersionConflict
    );
}
