//! Pure filesystem target validation; Host retains tool selection and authorization.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetError {
    Project,
    Target,
}

pub fn validate_target(project_root: &Path, target: &Path) -> Result<PathBuf, TargetError> {
    let root = project_root
        .canonicalize()
        .map_err(|_| TargetError::Project)?;
    if !root.is_dir() {
        return Err(TargetError::Project);
    }
    if !target.starts_with(project_root) {
        return Err(TargetError::Target);
    }
    let canonical_target = target.canonicalize().map_err(|_| TargetError::Target)?;
    if !canonical_target.starts_with(root) {
        return Err(TargetError::Target);
    }
    Ok(canonical_target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn target_must_exist_below_the_declared_project_root() {
        let base = std::env::temp_dir().join(format!(
            "tooling-target-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let project = base.join("project");
        let other = base.join("other");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(&other).unwrap();
        fs::write(project.join("source.rs"), "source").unwrap();
        fs::write(other.join("source.rs"), "other").unwrap();

        assert_eq!(
            validate_target(&project, &project.join("source.rs")),
            Ok(project.join("source.rs").canonicalize().unwrap())
        );
        assert_eq!(
            validate_target(&project, &other.join("source.rs")),
            Err(TargetError::Target)
        );
        assert_eq!(
            validate_target(&project, &project.join("missing.rs")),
            Err(TargetError::Target)
        );
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(other.join("source.rs"), project.join("escape.rs")).unwrap();
            assert_eq!(
                validate_target(&project, &project.join("escape.rs")),
                Err(TargetError::Target)
            );
        }
    }
}
