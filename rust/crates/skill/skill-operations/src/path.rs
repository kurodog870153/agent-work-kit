//! Pure path rules: kebab-case names, relative paths and hierarchy layout.

use skill_model::{Reason, RouteKind, SkillError};

/// Skill-relative directory of the single instruction tree.
pub const INSTRUCTIONS_DIR: &str = "instructions";
/// File that defines the Root or a Node.
pub const INSTRUCTION_FILE: &str = "instructions.md";
/// Directory name of conditional Reference sources.
pub const REFERENCES_DIR: &str = "references";
/// Shared-relative Bootstrap file.
pub const BOOTSTRAP_FILE: &str = "bootstrap.md";
/// Shared-relative directory of Bootstrap Modules.
pub const MODULES_DIR: &str = "bootstrap";

/// Lower kebab-case: non-empty segments of `[a-z0-9]` joined by single `-`.
pub fn is_kebab(name: &str) -> bool {
    !name.is_empty()
        && name.split('-').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

/// A declared route is a direct, lower kebab-case Markdown file name.
pub fn is_route_file(name: &str) -> bool {
    name.strip_suffix(".md").is_some_and(is_kebab)
}

/// Reject anything that is not a plain forward-slash relative path.
pub fn validate_relative(path: &str) -> Result<(), SkillError> {
    let invalid = || {
        SkillError::new(
            Reason::InvalidPath,
            format!("not a safe relative path: {path:?}"),
        )
    };
    if path.is_empty() || path.contains('\\') || path.contains(':') || path.starts_with('/') {
        return Err(invalid());
    }
    if path
        .split('/')
        .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return Err(invalid());
    }
    Ok(())
}

/// Validate a node path (`""` is the Root) made of kebab-case segments.
pub fn validate_node(node: &str) -> Result<(), SkillError> {
    if node.is_empty() {
        return Ok(());
    }
    if node.split('/').all(is_kebab) {
        Ok(())
    } else {
        Err(SkillError::new(
            Reason::InvalidName,
            format!("node path must be lower kebab-case segments: {node:?}"),
        ))
    }
}

/// Skill-relative directory of a node.
pub fn node_dir(node: &str) -> String {
    if node.is_empty() {
        INSTRUCTIONS_DIR.to_owned()
    } else {
        format!("{INSTRUCTIONS_DIR}/{node}")
    }
}

/// Skill-relative file of a node.
pub fn node_file(node: &str) -> String {
    format!("{}/{INSTRUCTION_FILE}", node_dir(node))
}

/// Node paths from the Root to `node`, inclusive; branches never mix.
pub fn ancestor_chain(node: &str) -> Vec<String> {
    let mut chain = vec![String::new()];
    let mut current = String::new();
    for segment in node.split('/').filter(|segment| !segment.is_empty()) {
        if !current.is_empty() {
            current.push('/');
        }
        current.push_str(segment);
        chain.push(current.clone());
    }
    chain
}

/// Path of a declared route relative to the root that owns its declaration.
///
/// Modules live under the Shared `bootstrap/` directory; References live under
/// the declaring node's `references/` directory.
pub fn route_path(kind: RouteKind, node: &str, name: &str) -> String {
    match kind {
        RouteKind::Module => format!("{MODULES_DIR}/{name}"),
        RouteKind::Reference => format!("{}/{REFERENCES_DIR}/{name}", node_dir(node)),
    }
}

#[cfg(test)]
mod tests;
