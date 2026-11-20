//! Use cases the Skill CLI exposes.

use skill_feature::{
    Loaded, ReferenceSelection, Request, Resolution, Selection, SourceStore,
    discover as discover_children, plan_switch, resolve as resolve_set, validate_all,
};
use skill_model::{
    ChildData, DISCOVER_RESPONSE, DiscoverData, DocumentKind, ErrorData, RESOLVE_REQUEST,
    RESOLVE_RESPONSE, Reason, RequestDto, ResolveData, SWITCH_RESPONSE, SkillError, SourceData,
    SwitchData, VALIDATE_RESPONSE, ValidateData,
};

/// Convert to the Feature request after checking the declared schema.
pub fn into_request(dto: RequestDto) -> Result<Request, SkillError> {
    if dto.schema != RESOLVE_REQUEST {
        return Err(SkillError::new(
            Reason::InvalidRequest,
            format!("schema must be {RESOLVE_REQUEST}"),
        ));
    }
    Ok(Request {
        selections: dto
            .selections
            .into_iter()
            .map(|selection| Selection {
                skill: selection.skill,
                nodes: selection.nodes,
                references: selection
                    .references
                    .into_iter()
                    .map(|reference| ReferenceSelection {
                        node: reference.node,
                        name: reference.name,
                    })
                    .collect(),
            })
            .collect(),
        modules: dto.modules,
    })
}

const fn kind_code(kind: DocumentKind) -> &'static str {
    match kind {
        DocumentKind::Instruction => "instruction",
        DocumentKind::Reference => "reference",
        DocumentKind::Bootstrap => "bootstrap",
        DocumentKind::Module => "module",
    }
}

// Per-source failures are part of a successful command response, so Flow
// converts them at the response boundary; CLI maps command-level failures.
fn source_error(error: &SkillError) -> ErrorData {
    ErrorData {
        reason: error.reason.code().to_owned(),
        message: error.message.clone(),
        source: error.source.clone(),
        scope: error.scope.map(|scope| scope.code().to_owned()),
        selection: error.selection.clone(),
    }
}

fn source_data(loaded: &Loaded) -> SourceData {
    SourceData {
        source: loaded.source.to_string(),
        id: loaded.id.as_str().to_owned(),
        kind: kind_code(loaded.kind),
    }
}

/// Read the direct children of one node.
pub fn discover(
    store: &impl SourceStore,
    skill: &str,
    node: &str,
) -> Result<DiscoverData, SkillError> {
    let found = discover_children(store, skill, node)?;
    Ok(DiscoverData {
        schema: DISCOVER_RESPONSE,
        skill: skill.to_owned(),
        node: node.to_owned(),
        children: found
            .children
            .into_iter()
            .map(|child| ChildData {
                name: child.frontmatter.name,
                description: child.frontmatter.description,
                tags: child.frontmatter.metadata.tags,
                node: child.node,
            })
            .collect(),
        failures: found.failures.iter().map(source_error).collect(),
    })
}

fn resolve_dto(store: &impl SourceStore, dto: RequestDto) -> Result<Resolution, SkillError> {
    resolve_set(store, &into_request(dto)?)
}

/// Validate a complete candidate set and list its de-duplicated sources.
pub fn resolve(store: &impl SourceStore, dto: RequestDto) -> Result<ResolveData, SkillError> {
    let resolution = resolve_dto(store, dto)?;
    Ok(ResolveData {
        schema: RESOLVE_RESPONSE,
        sources: resolution.sources.iter().map(source_data).collect(),
    })
}

/// Validate both candidates fully, then plan the incremental replacement.
///
/// Either candidate being invalid rejects the switch; the active set is never
/// kept as a fallback for an invalid or unknown required source.
pub fn switch(
    store: &impl SourceStore,
    active: RequestDto,
    next: RequestDto,
) -> Result<SwitchData, SkillError> {
    let active = resolve_dto(store, active)?;
    let next = resolve_dto(store, next)?;
    let delta = plan_switch(&active, &next);
    let text =
        |sources: Vec<skill_model::SourceRef>| sources.iter().map(ToString::to_string).collect();
    Ok(SwitchData {
        schema: SWITCH_RESPONSE,
        reused: text(delta.reused),
        added: text(delta.added),
        dropped: text(delta.dropped),
    })
}

/// Validate every source of the installation.
pub fn validate(store: &impl SourceStore) -> Result<ValidateData, SkillError> {
    let report = validate_all(store)?;
    Ok(ValidateData {
        schema: VALIDATE_RESPONSE,
        valid: report.is_valid(),
        checked: report.checked.iter().map(ToString::to_string).collect(),
        failures: report.failures.iter().map(source_error).collect(),
    })
}
