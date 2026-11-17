//! Direct-child metadata discovery along one branch.

use skill_model::{
    DocumentKind, FailureScope, Frontmatter, Reason, SkillError, SourceRef, SourceRoot,
};
use skill_operations::path::{REFERENCES_DIR, is_kebab, node_dir, node_file, validate_node};

use crate::loader::load;
use crate::port::SourceStore;

/// Metadata of one direct child node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Child {
    /// Node path from the Root, such as `task/planning`.
    pub node: String,
    pub frontmatter: Frontmatter,
}

/// Valid children plus the children that failed, so one bad branch does not
/// hide its valid siblings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Discovery {
    pub children: Vec<Child>,
    pub failures: Vec<SkillError>,
}

/// Read the direct children of `node` without loading any deeper level.
pub fn discover(
    store: &impl SourceStore,
    skill: &str,
    node: &str,
) -> Result<Discovery, SkillError> {
    validate_node(node)?;
    let scope = if node.is_empty() {
        FailureScope::RootOrAncestor
    } else {
        FailureScope::Branch
    };
    load(
        store,
        SourceRef::skill(skill, node_file(node)),
        DocumentKind::Instruction,
    )
    .map_err(|error| error.with_scope(scope).with_selection(skill))?;
    let entries = store
        .entries(&SourceRoot::Skill(skill.to_owned()), &node_dir(node))
        .map_err(|error| error.with_scope(scope).with_selection(skill))?;
    let mut discovery = Discovery {
        children: Vec::new(),
        failures: Vec::new(),
    };
    for entry in entries.into_iter().filter(|entry| entry.is_dir) {
        if entry.name == REFERENCES_DIR {
            continue;
        }
        let child = if node.is_empty() {
            entry.name.clone()
        } else {
            format!("{node}/{}", entry.name)
        };
        if !is_kebab(&entry.name) {
            discovery.failures.push(
                SkillError::new(
                    Reason::InvalidName,
                    format!("node directory must be lower kebab-case: {child}"),
                )
                .with_scope(FailureScope::Branch)
                .with_selection(skill),
            );
            continue;
        }
        match load(
            store,
            SourceRef::skill(skill, node_file(&child)),
            DocumentKind::Instruction,
        ) {
            Ok(loaded) => {
                if let Some(frontmatter) = loaded.document.frontmatter {
                    discovery.children.push(Child {
                        node: child,
                        frontmatter,
                    });
                }
            }
            Err(error) => {
                let error = if error.reason == Reason::Unreadable {
                    SkillError::new(
                        Reason::AncestorMissing,
                        format!("node has no readable instructions.md: {child}"),
                    )
                    .with_source(error.source.unwrap_or_default())
                } else {
                    error
                };
                discovery
                    .failures
                    .push(error.with_scope(FailureScope::Branch).with_selection(skill));
            }
        }
    }
    discovery.children.sort_by(|a, b| a.node.cmp(&b.node));
    Ok(discovery)
}

#[cfg(test)]
mod tests;
