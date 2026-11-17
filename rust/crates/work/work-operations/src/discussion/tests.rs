use super::*;
use work_model::{Answer, NewDecision, NewPending, QuestionOption, RecordId};

fn base() -> DiscussionRecord {
    new_discussion(DiscussionCreateRequest {
        schema: String::new(),
        id: RecordId::parse("d1").unwrap(),
        original_request: " raw\r\nrequest ".to_owned(),
        source_ids: Vec::new(),
    })
    .unwrap()
}

fn round(version: u32) -> DiscussionRoundRequest {
    DiscussionRoundRequest {
        schema: String::new(),
        id: RecordId::parse("d1").unwrap(),
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

fn question(id: &str) -> Question {
    Question {
        id: id.to_owned(),
        text: "which?".to_owned(),
        options: ["a", "b"]
            .map(|id| QuestionOption {
                id: id.to_owned(),
                text: id.to_owned(),
            })
            .into(),
        recommendation: Some("a".to_owned()),
        reason: None,
    }
}

fn asked() -> DiscussionRecord {
    let mut request = round(1);
    request.ask = Some(question("q1"));
    apply_round(&base(), request).unwrap()
}

fn answer_of(question: &str, option: Option<&str>, text: Option<&str>) -> Answer {
    Answer {
        question_id: question.to_owned(),
        option_id: option.map(str::to_owned),
        text: text.map(str::to_owned),
    }
}

#[test]
fn original_request_is_preserved_verbatim() {
    assert_eq!(base().original_request, " raw\r\nrequest ");
}

#[test]
fn answers_must_match_the_question_and_its_options() {
    let record = asked();
    let answer = |question: &str, option: Option<&str>, text: Option<&str>| {
        let mut request = round(2);
        request.answer = Some(answer_of(question, option, text));
        apply_round(&record, request)
    };
    assert!(answer("q1", Some("b"), None).is_ok());
    assert!(answer("q1", None, Some("free")).is_ok());
    assert!(answer("q9", Some("a"), None).is_err());
    assert!(answer("q1", Some("zzz"), None).is_err());
    assert!(answer("q1", None, None).is_err());
    assert!(answer("q1", None, Some("  ")).is_err());
}

#[test]
fn a_question_is_answered_once_and_one_is_asked_at_a_time() {
    let record = asked();
    let mut again = round(2);
    again.ask = Some(question("q2"));
    assert!(apply_round(&record, again).is_err());

    let mut both = round(2);
    both.answer = Some(answer_of("q1", Some("a"), None));
    both.ask = Some(question("q2"));
    let answered = apply_round(&record, both).unwrap();
    assert_eq!(outstanding_question(&answered).unwrap().id, "q2");

    let mut twice = round(3);
    twice.answer = Some(answer_of("q1", Some("b"), None));
    assert!(apply_round(&answered, twice).is_err());
}

#[test]
fn versions_must_match_and_rounds_must_change_something() {
    assert_eq!(
        apply_round(&base(), round(7)).unwrap_err().reason,
        Reason::VersionConflict
    );
    assert_eq!(
        apply_round(&base(), round(1)).unwrap_err().reason,
        Reason::InvalidRequest
    );
}

#[test]
fn decisions_pending_and_close_follow_their_rules() {
    let mut request = round(1);
    request.add_pending = vec![NewPending {
        id: "p1".to_owned(),
        text: "later".to_owned(),
    }];
    request.decisions = vec![NewDecision {
        id: "dec1".to_owned(),
        text: "use X".to_owned(),
        question_id: Some("q1".to_owned()),
    }];
    assert!(apply_round(&base(), request.clone()).is_err());
    request.decisions[0].question_id = None;
    let record = apply_round(&base(), request).unwrap();
    assert_eq!(summarise(&record).open_pending, 1);

    let mut close = round(2);
    close.close = true;
    assert!(apply_round(&record, close).is_err());
    let mut resolve = round(2);
    resolve.resolve_pending = vec!["p1".to_owned()];
    resolve.close = true;
    let closed = apply_round(&record, resolve).unwrap();
    assert_eq!(closed.status, DiscussionStatus::Closed);
    assert!(apply_round(&closed, round(3)).is_err());
}
