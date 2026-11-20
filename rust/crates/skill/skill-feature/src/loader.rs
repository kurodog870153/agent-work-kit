//! Load one source through the port and validate its document contract.

use skill_model::{Document, DocumentKind, SkillError, SourceId, SourceRef};
use skill_operations::parse_document;
use skill_operations::path::validate_relative;

use crate::port::SourceStore;

/// A source read through the port and parsed against its document role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    pub source: SourceRef,
    pub id: SourceId,
    pub kind: DocumentKind,
    pub document: Document,
    pub text: String,
}

pub(crate) fn load(
    store: &impl SourceStore,
    source: SourceRef,
    kind: DocumentKind,
) -> Result<Loaded, SkillError> {
    let label = source.to_string();
    validate_relative(&source.path).map_err(|error| error.with_source(label.clone()))?;
    let content = store
        .read(&source)
        .map_err(|error| error.with_source(label.clone()))?;
    let document = parse_document(kind, &content.text).map_err(|error| error.with_source(label))?;
    Ok(Loaded {
        source,
        id: content.id,
        kind,
        document,
        text: content.text,
    })
}
