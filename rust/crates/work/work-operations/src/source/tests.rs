use super::*;
use work_model::{Origin, OriginSource, RecordId, RecordRef, SourceContent};

fn origin() -> Origin {
    Origin {
        source: OriginSource::User,
        reference: None,
    }
}

fn source(content: &str) -> SourceCreateRequest {
    SourceCreateRequest {
        schema: String::new(),
        id: RecordId::parse("src-1").unwrap(),
        title: "Issue".to_owned(),
        locator: "https://example.test/1".to_owned(),
        content: Some(SourceContent::Text {
            text: content.to_owned(),
        }),
        origin: origin(),
    }
}

#[test]
fn source_keeps_content_bytes_and_rejects_blank_fields() {
    let raw = "  line\r\n\tkept  ";
    assert_eq!(
        new_source(source(raw), "test-digest".to_owned())
            .unwrap()
            .content,
        SourceContent::Text {
            text: raw.to_owned()
        }
    );
    assert_eq!(
        new_source(source("  "), "test-digest".to_owned())
            .unwrap_err()
            .reason,
        Reason::InvalidRequest
    );
}

fn draft(kind: RecordKind, version: u32, statement: &str) -> CorrectionDraftRequest {
    CorrectionDraftRequest {
        schema: String::new(),
        id: RecordId::parse("cor-1").unwrap(),
        target: RecordRef {
            kind,
            id: RecordId::parse("src-1").unwrap(),
            version,
        },
        statement: statement.to_owned(),
        reason: "typo".to_owned(),
        origin: origin(),
    }
}

#[test]
fn correction_rules_reject_self_targeting_and_blank_text() {
    assert!(new_correction(draft(RecordKind::Source, 1, "fixed")).is_ok());
    assert_eq!(
        new_correction(draft(RecordKind::Correction, 1, "x"))
            .unwrap_err()
            .reason,
        Reason::RuleViolation
    );
    assert!(new_correction(draft(RecordKind::Source, 0, "x")).is_err());
    assert!(new_correction(draft(RecordKind::Source, 1, " ")).is_err());
}
