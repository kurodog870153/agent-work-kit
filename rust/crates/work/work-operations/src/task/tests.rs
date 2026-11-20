use super::*;
use work_model::{AttemptInput, AttemptResult, Origin, OriginSource, RecordKind, RecordRef};

fn id(value: &str) -> RecordId {
    RecordId::parse(value).unwrap()
}

fn criterion(name: &str, text: &str, required: bool) -> Criterion {
    Criterion {
        id: name.to_owned(),
        text: text.to_owned(),
        required,
    }
}

fn create() -> TaskCreateRequest {
    TaskCreateRequest {
        schema: String::new(),
        id: id("t1"),
        title: "Export".to_owned(),
        goal: "Add export".to_owned(),
        requirements: vec!["CSV".to_owned()],
        constraints: Vec::new(),
        decisions: Vec::new(),
        criteria: vec![
            criterion("c1", "csv works", true),
            criterion("c2", "docs", false),
        ],
        discussion_ids: Vec::new(),
        source_ids: Vec::new(),
        notes: Vec::new(),
    }
}

fn revise(version: u32) -> TaskReviseRequest {
    TaskReviseRequest {
        schema: String::new(),
        id: id("t1"),
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

fn origin() -> Origin {
    Origin {
        source: OriginSource::Host,
        reference: None,
    }
}

fn conclusion(criterion: &str, verdict: Verdict) -> ConclusionInput {
    ConclusionInput {
        criterion_id: criterion.to_owned(),
        verdict,
        evidence: "ran it".to_owned(),
        origin: origin(),
    }
}

fn accepted(task: &TaskRecord, items: Vec<ConclusionInput>) -> TaskAcceptanceRecord {
    new_task_acceptance(
        TaskAcceptanceRecordRequest {
            schema: String::new(),
            id: id("a1"),
            task_id: task.id.clone(),
            task_version: task.version,
            expected_version: None,
            conclusions: items,
        },
        task,
    )
    .unwrap()
}

fn states(history: &[TaskRecord], acceptance: &TaskAcceptanceRecord) -> Vec<CriterionStatus> {
    criterion_states(history, Some(acceptance))
        .into_iter()
        .map(|state| state.status)
        .collect()
}

#[test]
fn a_task_needs_core_content() {
    assert!(new_task(create()).is_ok());
    for mutate in [
        (|r: &mut TaskCreateRequest| r.goal = " ".to_owned()) as fn(&mut _),
        |r| r.requirements.clear(),
        |r| r.criteria.clear(),
        |r| r.criteria[1].id = "c1".to_owned(),
        |r| r.criteria[0].text = String::new(),
        |r| r.discussion_ids = vec![id("d"), id("d")],
    ] {
        let mut request = create();
        mutate(&mut request);
        assert!(new_task(request).is_err());
    }
}

#[test]
fn completion_requires_every_required_criterion_to_pass() {
    let task = new_task(create()).unwrap();
    let none = criterion_states(std::slice::from_ref(&task), None);
    assert_eq!(
        complete_task(&task, 1, &none).unwrap_err().reason,
        Reason::RuleViolation
    );

    let failed = accepted(&task, vec![conclusion("c1", Verdict::Failed)]);
    let current = criterion_states(std::slice::from_ref(&task), Some(&failed));
    assert!(complete_task(&task, 1, &current).is_err());

    // The optional criterion does not gate completion.
    let passed = accepted(&task, vec![conclusion("c1", Verdict::Passed)]);
    let current = criterion_states(std::slice::from_ref(&task), Some(&passed));
    let done = complete_task(&task, 1, &current).unwrap();
    assert_eq!((done.version, done.status), (2, TaskStatus::Completed));
    assert!(complete_task(&done, 2, &current).is_err());
    assert_eq!(
        complete_task(&task, 5, &current).unwrap_err().reason,
        Reason::VersionConflict
    );
}

#[test]
fn a_later_conclusion_replaces_an_earlier_one() {
    let task = new_task(create()).unwrap();
    let mut record = accepted(&task, vec![conclusion("c1", Verdict::Failed)]);
    record = append_task_acceptance(
        &record,
        TaskAcceptanceRecordRequest {
            schema: String::new(),
            id: id("a1"),
            task_id: id("t1"),
            task_version: 1,
            expected_version: Some(1),
            conclusions: vec![conclusion("c1", Verdict::Passed)],
        },
        1,
        &task,
    )
    .unwrap();
    assert_eq!(
        states(std::slice::from_ref(&task), &record),
        [CriterionStatus::Passed, CriterionStatus::Missing]
    );
}

#[test]
fn revision_reopens_and_only_affected_criteria_need_reconfirmation() {
    let task = new_task(create()).unwrap();
    let record = accepted(
        &task,
        vec![
            conclusion("c1", Verdict::Passed),
            conclusion("c2", Verdict::Passed),
        ],
    );
    let done = complete_task(
        &task,
        1,
        &criterion_states(std::slice::from_ref(&task), Some(&record)),
    )
    .unwrap();

    let mut change = revise(2);
    change.criteria = Some(vec![
        criterion("c1", "csv works", true),
        criterion("c2", "docs and examples", false),
        criterion("c3", "pdf works", true),
    ]);
    let next = revise_task(&done, change).unwrap();
    let revision = next.revision.clone().unwrap();
    assert_eq!(revision.affected_criteria, ["c2", "c3"]);
    assert_eq!((next.version, next.status), (3, TaskStatus::Open));

    let history = [task, done, next];
    assert_eq!(
        states(&history, &record),
        [
            CriterionStatus::Passed,
            CriterionStatus::NeedsReconfirm,
            CriterionStatus::Missing
        ]
    );
}

#[test]
fn unchanged_criteria_can_be_forced_back_into_review() {
    let task = new_task(create()).unwrap();
    let record = accepted(&task, vec![conclusion("c1", Verdict::Passed)]);
    let mut change = revise(1);
    change.reconfirm_criteria = vec!["c1".to_owned()];
    let next = revise_task(&task, change).unwrap();
    assert_eq!(
        states(&[task.clone(), next.clone()], &record)[0],
        CriterionStatus::NeedsReconfirm
    );
    let mut unknown = revise(1);
    unknown.reconfirm_criteria = vec!["zz".to_owned()];
    assert!(revise_task(&task, unknown).is_err());
    assert_eq!(
        revise_task(&task, revise(1)).unwrap_err().reason,
        Reason::InvalidRequest
    );
    assert_eq!(
        revise_task(&task, revise(9)).unwrap_err().reason,
        Reason::VersionConflict
    );
}

#[test]
fn conclusions_must_name_known_criteria_on_the_latest_version() {
    let task = new_task(create()).unwrap();
    let request = |version: u32, items: Vec<ConclusionInput>| TaskAcceptanceRecordRequest {
        schema: String::new(),
        id: id("a1"),
        task_id: id("t1"),
        task_version: version,
        expected_version: None,
        conclusions: items,
    };
    assert!(
        new_task_acceptance(request(1, vec![conclusion("zz", Verdict::Passed)]), &task).is_err()
    );
    assert!(new_task_acceptance(request(1, Vec::new()), &task).is_err());
    assert!(
        new_task_acceptance(
            request(
                1,
                vec![
                    conclusion("c1", Verdict::Passed),
                    conclusion("c1", Verdict::Failed)
                ]
            ),
            &task
        )
        .is_err()
    );
    assert_eq!(
        new_task_acceptance(request(2, vec![conclusion("c1", Verdict::Passed)]), &task)
            .unwrap_err()
            .reason,
        Reason::VersionConflict
    );
    let mut blank = conclusion("c1", Verdict::Passed);
    blank.evidence = " ".to_owned();
    assert!(new_task_acceptance(request(1, vec![blank]), &task).is_err());
}

fn execution_request(attempt: &str) -> ExecutionRecordRequest {
    ExecutionRecordRequest {
        schema: String::new(),
        id: id("e1"),
        task_id: id("t1"),
        expected_version: None,
        attempt: AttemptInput {
            id: attempt.to_owned(),
            result: AttemptResult::Failed,
            changes: vec!["edited exporter".to_owned()],
            anomalies: vec!["timeout".to_owned()],
            verification: Vec::new(),
            evidence: Vec::new(),
        },
    }
}

#[test]
fn attempts_accumulate_without_touching_earlier_ones() {
    let first = new_execution(execution_request("a1"), 1).unwrap();
    let second = add_attempt(&first, execution_request("a2"), 1, 2).unwrap();
    assert_eq!(second.version, 2);
    assert_eq!(second.attempts[0], first.attempts[0]);
    assert_eq!(second.attempts[1].task_version, 2);
    assert!(add_attempt(&second, execution_request("a2"), 2, 2).is_err());
    assert_eq!(
        add_attempt(&second, execution_request("a3"), 1, 2)
            .unwrap_err()
            .reason,
        Reason::VersionConflict
    );
    let mut other = execution_request("a3");
    other.task_id = id("t2");
    assert!(add_attempt(&second, other, 2, 2).is_err());
    assert!(new_execution(execution_request(" "), 1).is_err());
}

fn requirement(verdict: Verdict, refs: Vec<RecordRef>) -> RequirementAcceptanceRecordRequest {
    RequirementAcceptanceRecordRequest {
        schema: String::new(),
        id: id("r1"),
        discussion_id: id("d1"),
        expected_version: None,
        verdict,
        evidence: "user confirmed".to_owned(),
        origin: Origin {
            source: OriginSource::User,
            reference: None,
        },
        task_refs: refs,
    }
}

fn task_ref(version: u32) -> RecordRef {
    RecordRef {
        kind: RecordKind::Task,
        id: id("t1"),
        version,
    }
}

#[test]
fn requirement_state_ignores_task_completion_and_follows_revisions() {
    let task = new_task(create()).unwrap();
    // No conclusion: completed tasks do not make the requirement pass.
    assert_eq!(
        requirement_state(None, &[vec![task.clone()]]),
        RequirementState {
            verdict: None,
            needs_reconfirm: false
        }
    );
    let record =
        new_requirement_acceptance(requirement(Verdict::Passed, vec![task_ref(1)]), 2).unwrap();
    assert_eq!(record.conclusions[0].discussion_version, 2);
    assert!(!requirement_state(Some(&record), &[vec![task.clone()]]).needs_reconfirm);

    let mut change = revise(1);
    change.goal = Some("bigger goal".to_owned());
    change.reconfirm_requirement = true;
    let next = revise_task(&task, change).unwrap();
    let history = vec![task, next];
    let state = requirement_state(Some(&record), std::slice::from_ref(&history));
    assert_eq!(state.verdict, Some(Verdict::Passed));
    assert!(state.needs_reconfirm);

    let again = append_requirement_acceptance(
        &record,
        requirement(Verdict::Passed, vec![task_ref(2)]),
        1,
        2,
    )
    .unwrap();
    assert!(!requirement_state(Some(&again), std::slice::from_ref(&history)).needs_reconfirm);
    assert_eq!(
        append_requirement_acceptance(&record, requirement(Verdict::Failed, Vec::new()), 7, 2)
            .unwrap_err()
            .reason,
        Reason::VersionConflict
    );
    assert!(
        new_requirement_acceptance(
            requirement(Verdict::Passed, vec![task_ref(1), task_ref(1)]),
            1
        )
        .is_err()
    );
}
