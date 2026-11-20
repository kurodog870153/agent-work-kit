//! Rules for Source and Correction records.

use work_model::{
    CORRECTION_SCHEMA, CorrectionDraftRequest, CorrectionRecord, Reason, RecordKind, RequiredText,
    SOURCE_SCHEMA, SourceCreateRequest, SourceRecord, WorkError,
};

fn require(field: &'static str, value: &str) -> Result<(), WorkError> {
    RequiredText::parse(field, value)
        .map(|_| ())
        .map_err(WorkError::from)
}

/// Build an immutable Source from exact evidence bytes and a technical digest.
pub fn new_source(
    request: SourceCreateRequest,
    content_digest: String,
) -> Result<SourceRecord, WorkError> {
    require("title", &request.title)?;
    require("locator", &request.locator)?;
    let content = request.content.ok_or_else(|| {
        WorkError::new(
            Reason::InvalidRequest,
            "Source content is required unless --from-file is supplied",
        )
    })?;
    if let work_model::SourceContent::Text { text } = &content {
        require("content.text", text)?;
    }
    require("content_digest", &content_digest)?;
    let content_length = content.bytes().len() as u64;
    Ok(SourceRecord {
        schema: SOURCE_SCHEMA.to_owned(),
        id: request.id,
        version: 1,
        title: request.title,
        locator: request.locator,
        content,
        content_length,
        content_digest,
        origin: request.origin,
    })
}

/// Build a Correction. Corrections are additive and never target themselves.
pub fn new_correction(request: CorrectionDraftRequest) -> Result<CorrectionRecord, WorkError> {
    require("statement", &request.statement)?;
    require("reason", &request.reason)?;
    if request.target.kind == RecordKind::Correction {
        return Err(WorkError::new(
            Reason::RuleViolation,
            "a Correction cannot target another Correction",
        ));
    }
    if request.target.version == 0 {
        return Err(WorkError::new(
            Reason::InvalidRequest,
            "target version starts at 1",
        ));
    }
    Ok(CorrectionRecord {
        schema: CORRECTION_SCHEMA.to_owned(),
        id: request.id,
        version: 1,
        target: request.target,
        statement: request.statement,
        reason: request.reason,
        origin: request.origin,
    })
}

#[cfg(test)]
mod tests;
