//! Bind Host-observed Tooling operations to filesystem targets.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedTool {
    /// Display identity only; it grants no capability.
    pub name: String,
    pub interfaces: HashSet<String>,
    pub capabilities: HashSet<String>,
    /// Operation semantics verified by the Host, independently of capability claims.
    pub supported_operations: HashSet<String>,
    pub project_root: PathBuf,
    pub session: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationNeed {
    pub operation: String,
    pub interface: String,
    pub capability: String,
    pub project_root: PathBuf,
    pub target: PathBuf,
    pub session: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingError {
    NoTool,
    Interface,
    Capability,
    Operation,
    Project,
    Target,
    Session,
}

pub fn bind<'a>(
    observed: &'a [ObservedTool],
    need: &OperationNeed,
) -> Result<&'a ObservedTool, BindingError> {
    if observed.is_empty() {
        return Err(BindingError::NoTool);
    }
    let root = canonical_dir(&need.project_root).ok_or(BindingError::Project)?;
    let target = need
        .target
        .canonicalize()
        .map_err(|_| BindingError::Target)?;
    if !target.starts_with(&root) || !need.target.starts_with(&need.project_root) {
        return Err(BindingError::Target);
    }
    let mut rejection = BindingError::Interface;
    for tool in observed {
        if !tool.interfaces.contains(&need.interface) {
            continue;
        }
        rejection = BindingError::Capability;
        if !tool.capabilities.contains(&need.capability) {
            continue;
        }
        rejection = BindingError::Operation;
        if !tool.supported_operations.contains(&need.operation) {
            continue;
        }
        rejection = BindingError::Project;
        if canonical_dir(&tool.project_root).as_ref() != Some(&root) {
            continue;
        }
        rejection = BindingError::Session;
        if tool.session != need.session {
            continue;
        }
        return Ok(tool);
    }
    Err(rejection)
}

fn canonical_dir(path: &Path) -> Option<PathBuf> {
    let resolved = path.canonicalize().ok()?;
    resolved.is_dir().then_some(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture() -> (PathBuf, PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!(
            "skill-host-binding-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let first = base.join("first");
        let second = base.join("second");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        fs::write(first.join("source.rs"), "fn main() {}\n").unwrap();
        fs::write(second.join("source.rs"), "fn other() {}\n").unwrap();
        (base, first, second)
    }

    fn tool(root: &Path) -> ObservedTool {
        ObservedTool {
            name: "renamed-tool".into(),
            interfaces: HashSet::from(["mcp".into()]),
            capabilities: HashSet::from(["code-analysis".into(), "new-capability".into()]),
            supported_operations: HashSet::from(["find-references".into()]),
            project_root: root.to_path_buf(),
            session: Some("session-a".into()),
        }
    }

    fn need(root: &Path) -> OperationNeed {
        OperationNeed {
            operation: "find-references".into(),
            interface: "mcp".into(),
            capability: "code-analysis".into(),
            project_root: root.to_path_buf(),
            target: root.join("source.rs"),
            session: Some("session-a".into()),
        }
    }

    #[test]
    fn observed_semantics_and_project_identity_govern_binding() {
        let (_, first, second) = fixture();
        let observed = [tool(&first)];
        assert_eq!(bind(&observed, &need(&first)).unwrap().name, "renamed-tool");
        assert_eq!(bind(&[], &need(&first)), Err(BindingError::NoTool));
        let mut requested = need(&first);
        requested.capability = "new-capability".into();
        requested.operation = "unobserved-operation".into();
        assert_eq!(bind(&observed, &requested), Err(BindingError::Operation));
        requested.capability = "unobserved-capability".into();
        assert_eq!(bind(&observed, &requested), Err(BindingError::Capability));
        requested = need(&first);
        requested.interface = "unavailable-interface".into();
        assert_eq!(bind(&observed, &requested), Err(BindingError::Interface));
        requested = need(&first);
        requested.project_root = second.clone();
        requested.target = second.join("source.rs");
        assert_eq!(bind(&observed, &requested), Err(BindingError::Project));
        requested = need(&first);
        requested.session = Some("session-b".into());
        assert_eq!(bind(&observed, &requested), Err(BindingError::Session));
    }

    #[test]
    fn realpath_and_multi_root_targets_cannot_escape_binding() {
        let (base, first, second) = fixture();
        let observed = [tool(&first)];
        let mut requested = need(&first);
        requested.target = second.join("source.rs");
        assert_eq!(bind(&observed, &requested), Err(BindingError::Target));
        requested.target = first.join("missing.rs");
        assert_eq!(bind(&observed, &requested), Err(BindingError::Target));
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink(second.join("source.rs"), first.join("escape.rs")).unwrap();
            requested.target = first.join("escape.rs");
            assert_eq!(bind(&observed, &requested), Err(BindingError::Target));
        }
        // Another root remains independently usable when the Host reports it.
        let both = [tool(&first), tool(&second)];
        assert_eq!(bind(&both, &need(&second)).unwrap().project_root, second);
        assert!(base.is_dir());
    }
}
