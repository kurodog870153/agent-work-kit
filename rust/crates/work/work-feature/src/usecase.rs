//! Use cases over Source, Discussion and Correction records.

use work_model::{
    APPLY_REQUEST_SCHEMA, CorrectionApplyRequest, CorrectionDraftRequest, CorrectionRecord,
    DISCUSSION_CREATE_SCHEMA, DISCUSSION_ROUND_SCHEMA, DRAFT_REQUEST_SCHEMA,
    DiscussionCreateRequest, DiscussionRecord, DiscussionRoundRequest, Reason, RecordId,
    RecordKind, RecordRef, RecordSummary, SOURCE_CREATE_SCHEMA, SourceCreateRequest, SourceRecord,
    VersionSummary, WorkError,
};
use work_operations::{apply_round, encode, new_correction, new_discussion, new_source, summarise};

use crate::port::{Precondition, RecordStore, StoredRecord, WritePlan};

pub(crate) fn check_schema(actual: &str, expected: &str) -> Result<(), WorkError> {
    if actual == expected {
        Ok(())
    } else {
        Err(WorkError::new(
            Reason::InvalidRequest,
            format!("schema must be {expected}"),
        ))
    }
}

pub(crate) fn not_found(kind: RecordKind, id: &RecordId) -> WorkError {
    WorkError::new(
        Reason::NotFound,
        format!("{} not found: {}", kind.code(), id.as_str()),
    )
}

pub(crate) fn latest(
    store: &impl RecordStore,
    kind: RecordKind,
    id: &RecordId,
) -> Result<u32, WorkError> {
    store
        .latest_version(kind, id)?
        .ok_or_else(|| not_found(kind, id))
}

pub(crate) fn reference(record: &StoredRecord) -> RecordRef {
    RecordRef {
        kind: record.kind(),
        id: record.id().clone(),
        version: record.version(),
    }
}

fn read_source(
    store: &impl RecordStore,
    id: &RecordId,
    version: u32,
) -> Result<SourceRecord, WorkError> {
    match store.read(RecordKind::Source, id, version)? {
        StoredRecord::Source(record) => Ok(record),
        _ => Err(WorkError::new(
            Reason::InvalidRecord,
            "record kind mismatch",
        )),
    }
}

pub(crate) fn read_discussion(
    store: &impl RecordStore,
    id: &RecordId,
    version: u32,
) -> Result<DiscussionRecord, WorkError> {
    match store.read(RecordKind::Discussion, id, version)? {
        StoredRecord::Discussion(record) => Ok(record),
        _ => Err(WorkError::new(
            Reason::InvalidRecord,
            "record kind mismatch",
        )),
    }
}

pub(crate) fn require_sources(store: &impl RecordStore, ids: &[RecordId]) -> Result<(), WorkError> {
    for id in ids {
        if store.latest_version(RecordKind::Source, id)?.is_none() {
            return Err(WorkError::new(
                Reason::NotFound,
                format!("linked source not found: {}", id.as_str()),
            ));
        }
    }
    Ok(())
}

/// Corrections that target any version of one record, in identifier order.
pub fn corrections_for(
    store: &impl RecordStore,
    kind: RecordKind,
    id: &RecordId,
) -> Result<Vec<CorrectionRecord>, WorkError> {
    let mut found = Vec::new();
    for correction_id in store.ids(RecordKind::Correction)? {
        let version = latest(store, RecordKind::Correction, &correction_id)?;
        if let StoredRecord::Correction(record) =
            store.read(RecordKind::Correction, &correction_id, version)?
            && record.target.kind == kind
            && &record.target.id == id
        {
            found.push(record);
        }
    }
    Ok(found)
}

pub fn create_source(
    store: &impl RecordStore,
    request: SourceCreateRequest,
) -> Result<RecordRef, WorkError> {
    check_schema(&request.schema, SOURCE_CREATE_SCHEMA)?;
    let bytes = request
        .content
        .as_ref()
        .ok_or_else(|| {
            WorkError::new(
                Reason::InvalidRequest,
                "Source content is required unless --from-file is supplied",
            )
        })?
        .bytes();
    let digest = store.content_digest(bytes);
    let record = StoredRecord::Source(new_source(request, digest)?);
    let plan = WritePlan {
        preconditions: vec![Precondition {
            kind: RecordKind::Source,
            id: record.id().clone(),
            latest: None,
        }],
        writes: vec![record.clone()],
    };
    store.commit(&plan)?;
    Ok(reference(&record))
}

/// A Source is immutable, so every query serves a published version as saved.
pub fn show_source(
    store: &impl RecordStore,
    id: &RecordId,
    version: Option<u32>,
) -> Result<(SourceRecord, Vec<RecordId>, Vec<CorrectionRecord>), WorkError> {
    let version = match version {
        Some(version) => version,
        None => latest(store, RecordKind::Source, id)?,
    };
    let record = read_source(store, id, version)?;
    let mut referenced_by = Vec::new();
    for discussion_id in store.ids(RecordKind::Discussion)? {
        let newest = latest(store, RecordKind::Discussion, &discussion_id)?;
        if read_discussion(store, &discussion_id, newest)?
            .source_ids
            .contains(id)
        {
            referenced_by.push(discussion_id);
        }
    }
    let corrections = corrections_for(store, RecordKind::Source, id)?;
    Ok((record, referenced_by, corrections))
}

pub fn create_discussion(
    store: &impl RecordStore,
    request: DiscussionCreateRequest,
) -> Result<RecordRef, WorkError> {
    check_schema(&request.schema, DISCUSSION_CREATE_SCHEMA)?;
    require_sources(store, &request.source_ids)?;
    let record = StoredRecord::Discussion(new_discussion(request)?);
    let mut preconditions = vec![Precondition {
        kind: RecordKind::Discussion,
        id: record.id().clone(),
        latest: None,
    }];
    if let StoredRecord::Discussion(discussion) = &record {
        for id in &discussion.source_ids {
            preconditions.push(Precondition {
                kind: RecordKind::Source,
                id: id.clone(),
                latest: Some(1),
            });
        }
    }
    store.commit(&WritePlan {
        preconditions,
        writes: vec![record.clone()],
    })?;
    Ok(reference(&record))
}

pub fn discussion_round(
    store: &impl RecordStore,
    request: DiscussionRoundRequest,
) -> Result<RecordRef, WorkError> {
    check_schema(&request.schema, DISCUSSION_ROUND_SCHEMA)?;
    let id = request.id.clone();
    let newest = latest(store, RecordKind::Discussion, &id)?;
    let base = read_discussion(store, &id, newest)?;
    require_sources(store, &request.link_sources)?;
    let link_sources = request.link_sources.clone();
    let next = StoredRecord::Discussion(apply_round(&base, request)?);
    let mut preconditions = vec![Precondition {
        kind: RecordKind::Discussion,
        id,
        latest: Some(newest),
    }];
    preconditions.extend(link_sources.into_iter().map(|id| Precondition {
        kind: RecordKind::Source,
        id,
        latest: Some(1),
    }));
    store.commit(&WritePlan {
        preconditions,
        writes: vec![next.clone()],
    })?;
    Ok(reference(&next))
}

pub fn show_discussion(
    store: &impl RecordStore,
    id: &RecordId,
    version: Option<u32>,
) -> Result<(DiscussionRecord, u32, Vec<CorrectionRecord>), WorkError> {
    let newest = latest(store, RecordKind::Discussion, id)?;
    let record = read_discussion(store, id, version.unwrap_or(newest))?;
    let corrections = corrections_for(store, RecordKind::Discussion, id)?;
    Ok((record, newest, corrections))
}

pub fn discussion_history(
    store: &impl RecordStore,
    id: &RecordId,
) -> Result<Vec<VersionSummary>, WorkError> {
    let newest = latest(store, RecordKind::Discussion, id)?;
    (1..=newest)
        .map(|version| read_discussion(store, id, version).map(|record| summarise(&record)))
        .collect()
}

pub fn show_progress(
    store: &impl RecordStore,
    id: &RecordId,
) -> Result<DiscussionRecord, WorkError> {
    let newest = latest(store, RecordKind::Discussion, id)?;
    read_discussion(store, id, newest)
}

pub fn status(store: &impl RecordStore) -> Result<Vec<RecordSummary>, WorkError> {
    let mut records = Vec::new();
    for kind in RecordKind::ALL {
        for id in store.ids(kind)? {
            if let Some(latest_version) = store.latest_version(kind, &id)? {
                records.push(RecordSummary {
                    kind,
                    id,
                    latest_version,
                });
            }
        }
    }
    Ok(records)
}

fn correction_candidate(
    store: &impl RecordStore,
    draft: CorrectionDraftRequest,
) -> Result<(CorrectionRecord, String, u32), WorkError> {
    check_schema(&draft.schema, DRAFT_REQUEST_SCHEMA)?;
    let target = draft.target.clone();
    let record = new_correction(draft)?;
    let newest = latest(store, target.kind, &target.id)?;
    if target.version > newest {
        return Err(WorkError::new(
            Reason::NotFound,
            format!(
                "{} {} has no version {}",
                target.kind.code(),
                target.id.as_str(),
                target.version
            ),
        ));
    }
    let text = encode(&record)?;
    Ok((record, text, newest))
}

/// Review step: returns the exact candidate text and the target version it
/// was reviewed against. Nothing is written.
pub fn correction_preview(
    store: &impl RecordStore,
    draft: CorrectionDraftRequest,
) -> Result<(String, u32), WorkError> {
    if store
        .latest_version(RecordKind::Correction, &draft.id)?
        .is_some()
    {
        return Err(WorkError::new(
            Reason::AlreadyExists,
            format!("correction already exists: {}", draft.id.as_str()),
        ));
    }
    let (_, text, newest) = correction_candidate(store, draft)?;
    Ok((text, newest))
}

/// Apply step: the approval must equal the candidate rebuilt from the draft and
/// the target must still be at the reviewed version.
pub fn correction_apply(
    store: &impl RecordStore,
    request: CorrectionApplyRequest,
) -> Result<RecordRef, WorkError> {
    check_schema(&request.schema, APPLY_REQUEST_SCHEMA)?;
    let target = request.draft.target.clone();
    let (record, text, newest) = correction_candidate(store, request.draft)?;
    if text != request.approved_candidate {
        return Err(WorkError::new(
            Reason::ApprovalMismatch,
            "the approved candidate differs from the candidate to apply",
        ));
    }
    if newest != request.reviewed_target_version {
        return Err(WorkError::new(
            Reason::VersionConflict,
            format!(
                "target moved from reviewed version {} to {newest}",
                request.reviewed_target_version
            ),
        ));
    }
    let stored = StoredRecord::Correction(record);
    store.commit(&WritePlan {
        preconditions: vec![
            Precondition {
                kind: RecordKind::Correction,
                id: stored.id().clone(),
                latest: None,
            },
            Precondition {
                kind: target.kind,
                id: target.id,
                latest: Some(newest),
            },
        ],
        writes: vec![stored.clone()],
    })?;
    Ok(reference(&stored))
}

pub fn recover(store: &impl RecordStore) -> Result<Vec<String>, WorkError> {
    store.recover()
}
