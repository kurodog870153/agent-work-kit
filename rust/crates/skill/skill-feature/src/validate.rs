//! Full-tree validation for maintenance and tests (not used at Runtime load).

use std::collections::BTreeSet;

use skill_model::{DocumentKind, Reason, RouteKind, SkillError, SourceRef, SourceRoot};
use skill_operations::path::{
    BOOTSTRAP_FILE, MODULES_DIR, REFERENCES_DIR, is_kebab, is_route_file, node_dir, node_file,
    route_path,
};

use crate::loader::{Loaded, load};
use crate::port::SourceStore;

/// Outcome of validating a whole installation.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Report {
    pub checked: Vec<SourceRef>,
    pub failures: Vec<SkillError>,
}

impl Report {
    pub fn is_valid(&self) -> bool {
        self.failures.is_empty()
    }

    fn fail(&mut self, error: SkillError) {
        self.failures.push(error);
    }

    fn check(&mut self, loaded: Result<Loaded, SkillError>) -> Option<Loaded> {
        match loaded {
            Ok(loaded) => {
                self.checked.push(loaded.source.clone());
                Some(loaded)
            }
            Err(error) => {
                self.fail(error);
                None
            }
        }
    }
}

/// Validate Shared, every Skill, every node and every declared source.
pub fn validate_all(store: &impl SourceStore) -> Result<Report, SkillError> {
    let mut report = Report::default();
    validate_shared(store, &mut report)?;
    for skill in store.skills()? {
        validate_skill(store, &skill, &mut report)?;
    }
    Ok(report)
}

fn validate_shared(store: &impl SourceStore, report: &mut Report) -> Result<(), SkillError> {
    let bootstrap = report.check(load(
        store,
        SourceRef::shared(BOOTSTRAP_FILE),
        DocumentKind::Bootstrap,
    ));
    let declared: BTreeSet<String> = bootstrap
        .iter()
        .flat_map(|loaded| {
            loaded
                .document
                .routes
                .iter()
                .map(|route| route.name.clone())
        })
        .collect();
    for name in &declared {
        let source = SourceRef::shared(route_path(RouteKind::Module, "", name));
        report.check(load(store, source, DocumentKind::Module));
    }
    check_undeclared(
        store,
        &SourceRoot::Shared,
        MODULES_DIR,
        &declared,
        "Module",
        report,
    )
}

fn validate_skill(
    store: &impl SourceStore,
    skill: &str,
    report: &mut Report,
) -> Result<(), SkillError> {
    let root = SourceRoot::Skill(skill.to_owned());
    for entry in store.entries(&root, "")? {
        if entry.is_dir && entry.name == REFERENCES_DIR {
            report.fail(
                SkillError::new(
                    Reason::BoundaryViolation,
                    format!("{skill} keeps a first-level references directory"),
                )
                .with_selection(skill),
            );
        }
    }
    walk(store, skill, "", report)
}

fn walk(
    store: &impl SourceStore,
    skill: &str,
    node: &str,
    report: &mut Report,
) -> Result<(), SkillError> {
    let root = SourceRoot::Skill(skill.to_owned());
    let source = SourceRef::skill(skill, node_file(node));
    let loaded = report.check(
        load(store, source, DocumentKind::Instruction).map_err(|error| {
            if error.reason == Reason::Unreadable && !node.is_empty() {
                SkillError::new(
                    Reason::AncestorMissing,
                    format!("node has no readable instructions.md: {node}"),
                )
                .with_source(error.source.unwrap_or_default())
            } else {
                error
            }
        }),
    );
    let declared: BTreeSet<String> = loaded
        .iter()
        .flat_map(|loaded| {
            loaded
                .document
                .routes
                .iter()
                .map(|route| route.name.clone())
        })
        .collect();
    for name in &declared {
        let source = SourceRef::skill(skill, route_path(RouteKind::Reference, node, name));
        report.check(load(store, source, DocumentKind::Reference));
    }
    let dir = node_dir(node);
    check_undeclared(
        store,
        &root,
        &format!("{dir}/{REFERENCES_DIR}"),
        &declared,
        "Reference",
        report,
    )?;
    for entry in store.entries(&root, &dir)? {
        if !entry.is_dir || entry.name == REFERENCES_DIR {
            continue;
        }
        let child = if node.is_empty() {
            entry.name.clone()
        } else {
            format!("{node}/{}", entry.name)
        };
        if !is_kebab(&entry.name) {
            report.fail(
                SkillError::new(
                    Reason::InvalidName,
                    format!("node directory must be lower kebab-case: {child}"),
                )
                .with_selection(skill),
            );
            continue;
        }
        walk(store, skill, &child, report)?;
    }
    Ok(())
}

/// Files in a route directory that no table declares are structural errors.
fn check_undeclared(
    store: &impl SourceStore,
    root: &SourceRoot,
    dir: &str,
    declared: &BTreeSet<String>,
    label: &str,
    report: &mut Report,
) -> Result<(), SkillError> {
    for entry in store.entries(root, dir)? {
        let valid = !entry.is_dir && is_route_file(&entry.name) && declared.contains(&entry.name);
        if !valid {
            report.fail(SkillError::new(
                Reason::UndeclaredSource,
                format!(
                    "{label} source is not a declared direct file: {dir}/{}",
                    entry.name
                ),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
