//! In-memory store and use-case tests. The store only models the port contract:
//! preconditions are re-checked before publishing a single record.

use std::cell::RefCell;
use std::collections::BTreeMap;

use work_model::{
    APPLY_REQUEST_SCHEMA, Answer, CorrectionApplyRequest, CorrectionDraftRequest,
    DISCUSSION_CREATE_SCHEMA, DISCUSSION_ROUND_SCHEMA, DRAFT_REQUEST_SCHEMA,
    DiscussionCreateRequest, DiscussionRoundRequest, Origin, OriginSource, Question,
    QuestionOption, Reason, RecordId, RecordKind, RecordRef, SOURCE_CREATE_SCHEMA, SourceContent,
    SourceCreateRequest, WorkError,
};

use crate::*;

#[derive(Default)]
pub(crate) struct MemoryStore {
    records: RefCell<BTreeMap<(RecordKind, RecordId), Vec<StoredRecord>>>,
    pub(crate) fail_commit: RefCell<bool>,
}

impl RecordStore for MemoryStore {
    fn content_digest(&self, _bytes: &[u8]) -> String {
        "memory-store-digest".to_owned()
    }

    fn latest_version(&self, kind: RecordKind, id: &RecordId) -> Result<Option<u32>, WorkError> {
        Ok(self
            .records
            .borrow()
            .get(&(kind, id.clone()))
            .map(|versions| versions.len() as u32))
    }

    fn read(
        &self,
        kind: RecordKind,
        id: &RecordId,
        version: u32,
    ) -> Result<StoredRecord, WorkError> {
        self.records
            .borrow()
            .get(&(kind, id.clone()))
            .and_then(|versions| versions.get((version as usize).checked_sub(1)?))
            .cloned()
            .ok_or_else(|| WorkError::new(Reason::NotFound, "missing version"))
    }

    fn ids(&self, kind: RecordKind) -> Result<Vec<RecordId>, WorkError> {
        Ok(self
            .records
            .borrow()
            .keys()
            .filter(|(candidate, _)| *candidate == kind)
            .map(|(_, id)| id.clone())
            .collect())
    }

    fn write(
        &self,
        record: &StoredRecord,
        preconditions: &[Precondition],
    ) -> Result<(), WorkError> {
        if *self.fail_commit.borrow() {
            return Err(WorkError::new(Reason::StorageFailure, "injected"));
        }
        for condition in preconditions {
            if self.latest_version(condition.kind, &condition.id)? != condition.latest {
                return Err(WorkError::new(Reason::VersionConflict, "precondition"));
            }
        }
        let latest = self
            .latest_version(record.kind(), record.id())?
            .unwrap_or(0);
        if latest.checked_add(1) != Some(record.version()) {
            return Err(WorkError::new(
                Reason::AlreadyExists,
                "published history is immutable",
            ));
        }
        self.records
            .borrow_mut()
            .entry((record.kind(), record.id().clone()))
            .or_default()
            .push(record.clone());
        Ok(())
    }
}

pub(crate) fn id(value: &str) -> RecordId {
    RecordId::parse(value).unwrap()
}

pub(crate) fn origin() -> Origin {
    Origin {
        source: OriginSource::External,
        reference: Some("issue".to_owned()),
    }
}

pub(crate) fn source(name: &str) -> SourceCreateRequest {
    SourceCreateRequest {
        schema: SOURCE_CREATE_SCHEMA.to_owned(),
        id: id(name),
        title: "Issue".to_owned(),
        locator: "https://example.test".to_owned(),
        content: Some(SourceContent::Text {
            text: "body\r\n".to_owned(),
        }),
        origin: origin(),
    }
}

pub(crate) fn discussion(name: &str, sources: &[&str]) -> DiscussionCreateRequest {
    DiscussionCreateRequest {
        schema: DISCUSSION_CREATE_SCHEMA.to_owned(),
        id: id(name),
        original_request: "build it".to_owned(),
        source_ids: sources.iter().map(|value| id(value)).collect(),
    }
}

pub(crate) fn round(name: &str, version: u32) -> DiscussionRoundRequest {
    DiscussionRoundRequest {
        schema: DISCUSSION_ROUND_SCHEMA.to_owned(),
        id: id(name),
        expected_version: version,
        answer: None,
        decisions: Vec::new(),
        add_pending: Vec::new(),
        resolve_pending: Vec::new(),
        link_sources: Vec::new(),
        ask: None,
        close: false,
    }
}

pub(crate) fn question() -> Question {
    Question {
        id: "q1".to_owned(),
        text: "which?".to_owned(),
        options: vec![
            QuestionOption {
                id: "1".to_owned(),
                text: "one".to_owned(),
            },
            QuestionOption {
                id: "2".to_owned(),
                text: "two".to_owned(),
            },
        ],
        recommendation: Some("1".to_owned()),
        reason: Some("simpler".to_owned()),
    }
}

fn draft(statement: &str) -> CorrectionDraftRequest {
    CorrectionDraftRequest {
        schema: DRAFT_REQUEST_SCHEMA.to_owned(),
        id: id("cor-1"),
        target: RecordRef {
            kind: RecordKind::Source,
            id: id("src-1"),
            version: 1,
        },
        statement: statement.to_owned(),
        reason: "typo".to_owned(),
        origin: origin(),
    }
}

#[test]
fn a_source_is_created_once_and_never_overwritten() {
    let store = MemoryStore::default();
    assert_eq!(create_source(&store, source("src-1")).unwrap().version, 1);
    assert_eq!(
        create_source(&store, source("src-1")).unwrap_err().reason,
        Reason::VersionConflict
    );
    let (record, linked, corrections) = show_source(&store, &id("src-1"), None).unwrap();
    assert_eq!(
        record.content,
        SourceContent::Text {
            text: "body\r\n".to_owned()
        }
    );
    assert!(linked.is_empty() && corrections.is_empty());
    assert_eq!(
        show_source(&store, &id("ghost"), None).unwrap_err().reason,
        Reason::NotFound
    );
}

#[test]
fn wrong_schema_is_rejected_before_any_write() {
    let store = MemoryStore::default();
    let mut request = source("src-1");
    request.schema = "other".to_owned();
    assert_eq!(
        create_source(&store, request).unwrap_err().reason,
        Reason::InvalidRequest
    );
    assert!(status(&store).unwrap().is_empty());
}

#[test]
fn discussions_link_sources_and_resume_across_sessions() {
    let store = MemoryStore::default();
    assert_eq!(
        create_discussion(&store, discussion("d1", &["src-1"]))
            .unwrap_err()
            .reason,
        Reason::NotFound
    );
    create_source(&store, source("src-1")).unwrap();
    create_discussion(&store, discussion("d1", &["src-1"])).unwrap();

    let mut ask = round("d1", 1);
    ask.ask = Some(question());
    discussion_round(&store, ask).unwrap();

    // A new "session" restores the question and answer state from the store.
    let progress = show_progress(&store, &id("d1")).unwrap();
    assert_eq!(progress.questions[0].id, "q1");
    assert!(progress.answers.is_empty());

    let mut answer = round("d1", 2);
    answer.answer = Some(Answer {
        question_id: "q1".to_owned(),
        option_id: Some("2".to_owned()),
        text: None,
    });
    discussion_round(&store, answer).unwrap();

    let (record, newest, _) = show_discussion(&store, &id("d1"), Some(2)).unwrap();
    assert_eq!((newest, record.answers.len()), (3, 0));
    let (latest, _, _) = show_discussion(&store, &id("d1"), None).unwrap();
    assert_eq!(latest.answers[0].option_id.as_deref(), Some("2"));
    let history = discussion_history(&store, &id("d1")).unwrap();
    assert_eq!(history.len(), 3);
    assert_eq!(history[2].answers, 1);

    let (_, referenced_by, _) = show_source(&store, &id("src-1"), None).unwrap();
    assert_eq!(referenced_by, vec![id("d1")]);
}

#[test]
fn stale_rounds_and_missing_links_are_rejected() {
    let store = MemoryStore::default();
    create_discussion(&store, discussion("d1", &[])).unwrap();
    let mut first = round("d1", 1);
    first.ask = Some(question());
    discussion_round(&store, first.clone()).unwrap();
    assert_eq!(
        discussion_round(&store, first).unwrap_err().reason,
        Reason::VersionConflict
    );
    let mut link = round("d1", 2);
    link.link_sources = vec![id("ghost")];
    assert_eq!(
        discussion_round(&store, link).unwrap_err().reason,
        Reason::NotFound
    );
    assert_eq!(discussion_history(&store, &id("d1")).unwrap().len(), 2);
}

#[test]
fn a_failed_commit_leaves_no_record() {
    let store = MemoryStore::default();
    *store.fail_commit.borrow_mut() = true;
    assert_eq!(
        create_source(&store, source("src-1")).unwrap_err().reason,
        Reason::StorageFailure
    );
    assert!(status(&store).unwrap().is_empty());
}

#[test]
fn correction_needs_the_exact_reviewed_candidate_and_leaves_history_intact() {
    let store = MemoryStore::default();
    create_source(&store, source("src-1")).unwrap();
    let (candidate, reviewed) = correction_preview(&store, draft("fixed")).unwrap();
    assert_eq!(reviewed, 1);
    assert!(
        status(&store)
            .unwrap()
            .iter()
            .all(|r| r.kind == RecordKind::Source)
    );

    let apply = |text: &str, version: u32, statement: &str| {
        correction_apply(
            &store,
            CorrectionApplyRequest {
                schema: APPLY_REQUEST_SCHEMA.to_owned(),
                draft: draft(statement),
                approved_candidate: text.to_owned(),
                reviewed_target_version: version,
            },
        )
    };
    assert_eq!(
        apply(&candidate, 1, "changed after review")
            .unwrap_err()
            .reason,
        Reason::ApprovalMismatch
    );
    assert_eq!(
        apply("", 1, "fixed").unwrap_err().reason,
        Reason::ApprovalMismatch
    );
    assert_eq!(
        apply(&candidate, 9, "fixed").unwrap_err().reason,
        Reason::VersionConflict
    );
    apply(&candidate, 1, "fixed").unwrap();

    let (record, _, corrections) = show_source(&store, &id("src-1"), None).unwrap();
    assert_eq!(
        record.content,
        SourceContent::Text {
            text: "body\r\n".to_owned()
        }
    );
    assert_eq!(corrections.len(), 1);
    assert_eq!(corrections[0].statement, "fixed");
    assert_eq!(
        correction_preview(&store, draft("fixed"))
            .unwrap_err()
            .reason,
        Reason::AlreadyExists
    );
}

#[test]
fn correction_targets_must_exist() {
    let store = MemoryStore::default();
    assert_eq!(
        correction_preview(&store, draft("x")).unwrap_err().reason,
        Reason::NotFound
    );
    create_source(&store, source("src-1")).unwrap();
    let mut far = draft("x");
    far.target.version = 5;
    assert_eq!(
        correction_preview(&store, far).unwrap_err().reason,
        Reason::NotFound
    );
}
