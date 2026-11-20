//! Resolve a candidate set, and plan an incremental switch between two sets.

use std::collections::BTreeSet;

use skill_model::{DocumentKind, FailureScope, Reason, RouteKind, SkillError, SourceId, SourceRef};
use skill_operations::path::{
    BOOTSTRAP_FILE, ancestor_chain, node_file, route_path, validate_node,
};

use crate::loader::{Loaded, load};
use crate::port::SourceStore;

/// A Reference selected on one node of one Skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceSelection {
    pub node: String,
    pub name: String,
}

/// What the Host selected for one Skill.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Selection {
    pub skill: String,
    /// Selected nodes; `""` is the Root. Branches combine independently.
    pub nodes: Vec<String>,
    pub references: Vec<ReferenceSelection>,
}

/// The complete candidate the Host wants to load.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Request {
    pub selections: Vec<Selection>,
    /// Bootstrap Modules, by declared file name.
    pub modules: Vec<String>,
}

/// A fully validated candidate set, ordered and de-duplicated by source identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    pub sources: Vec<Loaded>,
}

impl Resolution {
    pub fn ids(&self) -> BTreeSet<&SourceId> {
        self.sources.iter().map(|loaded| &loaded.id).collect()
    }
}

struct Collector {
    sources: Vec<Loaded>,
    seen: BTreeSet<SourceId>,
}

impl Collector {
    fn add(&mut self, loaded: Loaded) {
        if self.seen.insert(loaded.id.clone()) {
            self.sources.push(loaded);
        }
    }
}

/// Validate the whole candidate; any invalid required source rejects the set.
pub fn resolve(store: &impl SourceStore, request: &Request) -> Result<Resolution, SkillError> {
    if request.selections.is_empty() {
        return Err(SkillError::new(
            Reason::InvalidRequest,
            "at least one Skill must be selected",
        ));
    }
    let installed = store.skills()?;
    let mut collector = Collector {
        sources: Vec::new(),
        seen: BTreeSet::new(),
    };
    let bootstrap = load(
        store,
        SourceRef::shared(BOOTSTRAP_FILE),
        DocumentKind::Bootstrap,
    )
    .map_err(|error| error.with_scope(FailureScope::SharedBootstrap))?;
    let declared_modules = bootstrap.document.routes.clone();
    collector.add(bootstrap);
    for name in &request.modules {
        let Some(route) = declared_modules.iter().find(|route| route.name == *name) else {
            return Err(undeclared(name, BOOTSTRAP_FILE).with_scope(FailureScope::Branch));
        };
        let source = SourceRef::shared(route_path(RouteKind::Module, "", &route.name));
        collector.add(
            load(store, source, DocumentKind::Module)
                .map_err(|error| error.with_scope(FailureScope::Branch))?,
        );
    }
    let mut skills = BTreeSet::new();
    for selection in &request.selections {
        if !skills.insert(selection.skill.as_str()) {
            return Err(SkillError::new(
                Reason::InvalidRequest,
                format!("Skill selected twice: {}", selection.skill),
            ));
        }
        if !installed.contains(&selection.skill) {
            return Err(SkillError::new(
                Reason::UnknownRoot,
                format!(
                    "Skill is not part of this installation: {}",
                    selection.skill
                ),
            )
            .with_selection(selection.skill.clone()));
        }
        resolve_selection(store, selection, &mut collector)?;
    }
    Ok(Resolution {
        sources: collector.sources,
    })
}

fn undeclared(name: &str, owner: &str) -> SkillError {
    SkillError::new(
        Reason::UndeclaredSource,
        format!("{name} is not declared by {owner}"),
    )
}

fn resolve_selection(
    store: &impl SourceStore,
    selection: &Selection,
    collector: &mut Collector,
) -> Result<(), SkillError> {
    let skill = selection.skill.as_str();
    let tag = |error: SkillError, scope| error.with_scope(scope).with_selection(skill);
    let mut chains: Vec<Loaded> = Vec::new();
    let nodes: Vec<&str> = if selection.nodes.is_empty() {
        vec![""]
    } else {
        selection.nodes.iter().map(String::as_str).collect()
    };
    for node in nodes {
        validate_node(node).map_err(|error| tag(error, FailureScope::Branch))?;
        let chain = ancestor_chain(node);
        let last = chain.len() - 1;
        for (depth, step) in chain.iter().enumerate() {
            let source = SourceRef::skill(skill, node_file(step));
            let loaded = load(store, source, DocumentKind::Instruction).map_err(|error| {
                let scope = if depth < last || step.is_empty() {
                    FailureScope::RootOrAncestor
                } else {
                    FailureScope::Branch
                };
                let error = if error.reason == Reason::Unreadable && depth > 0 {
                    SkillError::new(
                        Reason::AncestorMissing,
                        format!("node chain is incomplete at {step:?}"),
                    )
                    .with_source(error.source.clone().unwrap_or_default())
                } else {
                    error
                };
                tag(error, scope)
            })?;
            chains.push(loaded);
        }
    }
    for reference in &selection.references {
        let owner = chains
            .iter()
            .find(|loaded| loaded.source.path == node_file(&reference.node));
        let Some(owner) = owner else {
            return Err(tag(
                SkillError::new(
                    Reason::UndeclaredSource,
                    format!("Reference owner node is not selected: {:?}", reference.node),
                ),
                FailureScope::Branch,
            ));
        };
        let declared = owner
            .document
            .routes
            .iter()
            .any(|route| route.name == reference.name);
        if !declared {
            return Err(tag(
                undeclared(&reference.name, &owner.source.to_string()),
                FailureScope::Branch,
            ));
        }
        let source = SourceRef::skill(
            skill,
            route_path(RouteKind::Reference, &reference.node, &reference.name),
        );
        let loaded = load(store, source, DocumentKind::Reference)
            .map_err(|error| tag(error, FailureScope::Branch))?;
        chains.push(loaded);
    }
    for loaded in chains {
        collector.add(loaded);
    }
    Ok(())
}

/// Incremental difference between the active set and a validated next set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delta {
    /// Still valid and applicable: bodies stay in context.
    pub reused: Vec<SourceRef>,
    /// New bodies to bring into context.
    pub added: Vec<SourceRef>,
    /// Bodies that leave the active set.
    pub dropped: Vec<SourceRef>,
}

/// Plan the switch from `active` to `next`. Both sets are already validated as
/// whole candidates, so the active set is replaced atomically.
pub fn plan_switch(active: &Resolution, next: &Resolution) -> Delta {
    let active_ids = active.ids();
    let next_ids = next.ids();
    let pick = |from: &Resolution, keep: &dyn Fn(&SourceId) -> bool| {
        from.sources
            .iter()
            .filter(|loaded| keep(&loaded.id))
            .map(|loaded| loaded.source.clone())
            .collect()
    };
    Delta {
        reused: pick(next, &|id| active_ids.contains(id)),
        added: pick(next, &|id| !active_ids.contains(id)),
        dropped: pick(active, &|id| !next_ids.contains(id)),
    }
}

#[cfg(test)]
mod tests;
