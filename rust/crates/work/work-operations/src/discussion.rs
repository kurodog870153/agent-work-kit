//! Rules for Discussion rounds: answers must match the question and option
//! they were given for, and every round yields a new immutable version.

use std::collections::BTreeSet;

use work_model::{
    DISCUSSION_SCHEMA, Decision, DiscussionCreateRequest, DiscussionRecord, DiscussionRoundRequest,
    DiscussionStatus, Pending, Question, Reason, RequiredText, VersionSummary, WorkError,
};

fn violation(message: impl Into<String>) -> WorkError {
    WorkError::new(Reason::RuleViolation, message)
}

fn require(field: &'static str, value: &str) -> Result<(), WorkError> {
    RequiredText::parse(field, value)
        .map(|_| ())
        .map_err(WorkError::from)
}

/// Build version 1 of a Discussion. The original request is kept verbatim.
pub fn new_discussion(request: DiscussionCreateRequest) -> Result<DiscussionRecord, WorkError> {
    require("original_request", &request.original_request)?;
    let mut seen = BTreeSet::new();
    if !request.source_ids.iter().all(|id| seen.insert(id)) {
        return Err(violation("duplicate source link"));
    }
    Ok(DiscussionRecord {
        schema: DISCUSSION_SCHEMA.to_owned(),
        id: request.id,
        version: 1,
        original_request: request.original_request,
        status: DiscussionStatus::Open,
        source_ids: request.source_ids,
        questions: Vec::new(),
        answers: Vec::new(),
        decisions: Vec::new(),
        pending: Vec::new(),
    })
}

/// The question that still needs an answer, if any.
pub fn outstanding_question(record: &DiscussionRecord) -> Option<&Question> {
    record.questions.iter().find(|question| {
        !record
            .answers
            .iter()
            .any(|answer| answer.question_id == question.id)
    })
}

fn check_question(question: &Question, existing: &[Question]) -> Result<(), WorkError> {
    require("question.id", &question.id)?;
    require("question.text", &question.text)?;
    if existing.iter().any(|item| item.id == question.id) {
        return Err(violation(format!("duplicate question: {}", question.id)));
    }
    let mut ids = BTreeSet::new();
    for option in &question.options {
        require("option.id", &option.id)?;
        require("option.text", &option.text)?;
        if !ids.insert(option.id.as_str()) {
            return Err(violation(format!("duplicate option: {}", option.id)));
        }
    }
    if let Some(recommended) = &question.recommendation
        && !ids.contains(recommended.as_str())
    {
        return Err(violation("recommendation must name one of the options"));
    }
    Ok(())
}

/// Produce the next version of a Discussion from one round.
///
/// Order inside a round: answer, decisions, pending, question, close. The base
/// record is never modified.
pub fn apply_round(
    base: &DiscussionRecord,
    request: DiscussionRoundRequest,
) -> Result<DiscussionRecord, WorkError> {
    if request.expected_version != base.version {
        return Err(WorkError::new(
            Reason::VersionConflict,
            format!(
                "expected version {} but latest is {}",
                request.expected_version, base.version
            ),
        ));
    }
    if base.status == DiscussionStatus::Closed {
        return Err(violation("a closed Discussion accepts no further rounds"));
    }
    let mut next = base.clone();
    next.version = base.version + 1;

    if let Some(answer) = request.answer {
        let question = next
            .questions
            .iter()
            .find(|item| item.id == answer.question_id)
            .ok_or_else(|| violation(format!("unknown question: {}", answer.question_id)))?;
        if next
            .answers
            .iter()
            .any(|item| item.question_id == answer.question_id)
        {
            return Err(violation(format!(
                "question already answered: {}",
                answer.question_id
            )));
        }
        if let Some(option) = &answer.option_id
            && !question.options.iter().any(|item| &item.id == option)
        {
            return Err(violation(format!(
                "option {option} does not belong to question {}",
                answer.question_id
            )));
        }
        let has_text = answer.text.as_deref().is_some_and(|t| !t.trim().is_empty());
        if answer.option_id.is_none() && !has_text {
            return Err(violation("an answer needs an option or text"));
        }
        next.answers.push(answer);
    }
    for decision in request.decisions {
        require("decision.id", &decision.id)?;
        require("decision.text", &decision.text)?;
        if next.decisions.iter().any(|item| item.id == decision.id) {
            return Err(violation(format!("duplicate decision: {}", decision.id)));
        }
        if let Some(question_id) = &decision.question_id
            && !next
                .answers
                .iter()
                .any(|item| &item.question_id == question_id)
        {
            return Err(violation(format!(
                "decision refers to unanswered question: {question_id}"
            )));
        }
        next.decisions.push(Decision {
            id: decision.id,
            text: decision.text,
            question_id: decision.question_id,
        });
    }
    for id in request.resolve_pending {
        let item = next
            .pending
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or_else(|| violation(format!("unknown pending item: {id}")))?;
        if item.resolved {
            return Err(violation(format!("pending item already resolved: {id}")));
        }
        item.resolved = true;
    }
    for item in request.add_pending {
        require("pending.id", &item.id)?;
        require("pending.text", &item.text)?;
        if next.pending.iter().any(|existing| existing.id == item.id) {
            return Err(violation(format!("duplicate pending item: {}", item.id)));
        }
        next.pending.push(Pending {
            id: item.id,
            text: item.text,
            resolved: false,
        });
    }
    for id in request.link_sources {
        if next.source_ids.contains(&id) {
            return Err(violation(format!("source already linked: {}", id.as_str())));
        }
        next.source_ids.push(id);
    }
    if let Some(question) = request.ask {
        if outstanding_question(&next).is_some() {
            return Err(violation("ask one question at a time"));
        }
        check_question(&question, &next.questions)?;
        next.questions.push(question);
    }
    if request.close {
        if outstanding_question(&next).is_some() {
            return Err(violation("cannot close with an unanswered question"));
        }
        if next.pending.iter().any(|item| !item.resolved) {
            return Err(violation("cannot close with unresolved pending items"));
        }
        next.status = DiscussionStatus::Closed;
    }
    let mut unchanged = next.clone();
    unchanged.version = base.version;
    if unchanged == *base {
        return Err(WorkError::new(
            Reason::InvalidRequest,
            "the round changes nothing",
        ));
    }
    Ok(next)
}

/// Counts of one stored version, for history queries.
pub fn summarise(record: &DiscussionRecord) -> VersionSummary {
    VersionSummary {
        version: record.version,
        status: record.status,
        questions: record.questions.len(),
        answers: record.answers.len(),
        decisions: record.decisions.len(),
        open_pending: record.pending.iter().filter(|item| !item.resolved).count(),
    }
}

#[cfg(test)]
mod tests;
