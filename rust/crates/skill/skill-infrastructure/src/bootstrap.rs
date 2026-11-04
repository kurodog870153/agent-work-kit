//! Validation of one-level Bootstrap module routes.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleRouteError {
    MissingTable,
    InvalidRow,
    Duplicate(String),
    InvalidName(String),
    Missing(PathBuf),
    Escaped(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReferenceRouteError {
    InvalidTable,
    InvalidRow,
    InvalidPath(String),
    Duplicate(String),
    Missing(PathBuf),
    Escaped(PathBuf),
}

pub fn reference_routes(content: &str) -> Result<Vec<(String, String)>, ReferenceRouteError> {
    let mut sections = content
        .lines()
        .filter(|line| line.trim() == "## References");
    if sections.next().is_none() {
        return Ok(Vec::new());
    }
    if sections.next().is_some() {
        return Err(ReferenceRouteError::InvalidTable);
    }
    let heading = content
        .lines()
        .skip_while(|line| line.trim() != "## References")
        .skip(1)
        .collect::<Vec<_>>();
    let table = heading
        .iter()
        .skip_while(|line| line.trim().is_empty())
        .take_while(|line| line.trim().starts_with('|'))
        .copied()
        .collect::<Vec<_>>();
    if table.len() < 3
        || table[0].trim() != "| Reference | Load condition |"
        || table[1].trim() != "| --- | --- |"
    {
        return Err(ReferenceRouteError::InvalidTable);
    }
    let mut seen = HashSet::new();
    let mut routes = Vec::new();
    for row in table.iter().skip(2) {
        let cells = row
            .trim()
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect::<Vec<_>>();
        if cells.len() != 2 || cells[1].is_empty() {
            return Err(ReferenceRouteError::InvalidRow);
        }
        let relative = cells[0].trim_matches('`');
        let Some(name) = relative
            .strip_prefix("references/")
            .and_then(|name| name.strip_suffix(".md"))
        else {
            return Err(ReferenceRouteError::InvalidPath(relative.into()));
        };
        if !skill_operations::hierarchy::valid_segment(name) {
            return Err(ReferenceRouteError::InvalidPath(relative.into()));
        }
        if !seen.insert(name.to_owned()) {
            return Err(ReferenceRouteError::Duplicate(name.into()));
        }
        routes.push((name.into(), cells[1].into()));
    }
    Ok(routes)
}

pub fn validate_reference_path(
    directory: &Path,
    name: &str,
) -> Result<PathBuf, ReferenceRouteError> {
    if !skill_operations::hierarchy::valid_segment(name) {
        return Err(ReferenceRouteError::InvalidPath(name.into()));
    }
    let path = directory.join("references").join(format!("{name}.md"));
    let resolved = path
        .canonicalize()
        .map_err(|_| ReferenceRouteError::Missing(path.clone()))?;
    let resolved_directory = directory
        .canonicalize()
        .map_err(|_| ReferenceRouteError::Missing(directory.to_path_buf()))?;
    if !resolved.is_file() || !resolved.starts_with(resolved_directory.join("references")) {
        return Err(ReferenceRouteError::Escaped(path));
    }
    Ok(path)
}

pub fn validate_modules(
    bootstrap: &str,
    directory: &Path,
) -> Result<Vec<(String, String)>, ModuleRouteError> {
    let heading = bootstrap
        .lines()
        .skip_while(|line| line.trim() != "## Modules")
        .skip(1)
        .collect::<Vec<_>>();
    let table = heading
        .iter()
        .skip_while(|line| line.trim().is_empty())
        .take_while(|line| line.trim().starts_with('|'))
        .copied()
        .collect::<Vec<_>>();
    if table.len() < 3
        || table[0].trim() != "| Module | Load condition |"
        || table[1].trim() != "| --- | --- |"
    {
        return Err(ModuleRouteError::MissingTable);
    }
    let resolved_directory = directory
        .canonicalize()
        .map_err(|_| ModuleRouteError::Missing(directory.to_path_buf()))?;
    let mut seen = HashSet::new();
    let mut routes = Vec::new();
    for row in table.iter().skip(2) {
        let cells = row
            .trim()
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect::<Vec<_>>();
        if cells.len() != 2 || cells[1].is_empty() {
            return Err(ModuleRouteError::InvalidRow);
        }
        let name = cells[0].trim_matches('`');
        if !name.ends_with(".md")
            || !skill_operations::hierarchy::valid_segment(name.trim_end_matches(".md"))
        {
            return Err(ModuleRouteError::InvalidName(name.into()));
        }
        if !seen.insert(name.to_owned()) {
            return Err(ModuleRouteError::Duplicate(name.into()));
        }
        let path = directory.join(name);
        let resolved = path
            .canonicalize()
            .map_err(|_| ModuleRouteError::Missing(path.clone()))?;
        if !resolved.is_file() || !resolved.starts_with(&resolved_directory) {
            return Err(ModuleRouteError::Escaped(path));
        }
        routes.push((name.into(), cells[1].into()));
    }
    Ok(routes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn modules_require_unique_one_level_regular_sources() {
        let root = std::env::temp_dir().join(format!(
            "skill-modules-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("one.md"), "one").unwrap();
        let table =
            "## Modules\n\n| Module | Load condition |\n| --- | --- |\n| `one.md` | Need one. |\n";
        assert_eq!(validate_modules(table, &root).unwrap().len(), 1);
        assert!(matches!(
            validate_modules("## Modules", &root),
            Err(ModuleRouteError::MissingTable)
        ));
        assert!(matches!(
            validate_modules(&format!("{table}| `one.md` | Again. |\n"), &root),
            Err(ModuleRouteError::Duplicate(_))
        ));
        assert!(matches!(
            validate_modules(&table.replace("one.md", "missing.md"), &root),
            Err(ModuleRouteError::Missing(_))
        ));
        assert!(matches!(
            validate_modules(&table.replace("one.md", "../outside.md"), &root),
            Err(ModuleRouteError::InvalidName(_))
        ));
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let outside = root.with_extension("md");
            fs::write(&outside, "outside").unwrap();
            symlink(&outside, root.join("link.md")).unwrap();
            assert!(matches!(
                validate_modules(&table.replace("one.md", "link.md"), &root),
                Err(ModuleRouteError::Escaped(_))
            ));
        }
    }

    #[test]
    fn installed_work_bootstrap_has_valid_module_routes() {
        let root = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../skills/work/references"
        ));
        let bootstrap = fs::read_to_string(root.join("instruction-loading.md")).unwrap();
        let routes = validate_modules(&bootstrap, &root.join("instruction-loading")).unwrap();
        assert!(!routes.iter().any(|(name, _)| name == "invocation.md"));
        assert!(!routes.iter().any(|(name, _)| name == "private-roles.md"));
        assert!(!routes.iter().any(|(name, _)| name == "handoff.md"));
        assert!(
            routes
                .iter()
                .any(|(name, _)| name == "instruction-maintenance.md")
        );
        assert!(
            routes
                .iter()
                .any(|(name, _)| name == "tool-policy-integration.md")
        );
    }

    #[test]
    fn tooling_bootstrap_is_self_contained_and_blocks_invalid_routes() {
        let default_root = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../skills/tooling"
        ));
        let installed_root = std::env::var_os("TOOLING_SKILL_TEST_ROOT");
        let root = installed_root
            .as_deref()
            .map(Path::new)
            .unwrap_or(default_root);
        let entry = fs::read_to_string(root.join("SKILL.md")).unwrap();
        let bootstrap = fs::read_to_string(root.join("references/instruction-loading.md")).unwrap();
        assert!(entry.contains("references/instruction-loading.md"));
        let modules = root.join("references/instruction-loading");
        let routes = validate_modules(&bootstrap, &modules).unwrap();
        assert_eq!(routes.len(), 4);
        assert!(!bootstrap.contains("skills/work"));
        assert!(!bootstrap.contains("Work TASK"));
        for (name, _) in &routes {
            let content = fs::read_to_string(modules.join(name)).unwrap();
            assert!(!content.contains("skills/work"));
        }
        let missing = bootstrap.replace("`discovery.md`", "`absent.md`");
        assert!(matches!(
            validate_modules(&missing, &modules),
            Err(ModuleRouteError::Missing(_))
        ));
    }

    #[test]
    fn activation_modes_validate_only_their_own_bootstrap() {
        let repository = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../.."));
        let work_root = repository.join("skills/work/references");
        let tooling_root = repository.join("skills/tooling/references");
        let work = fs::read_to_string(work_root.join("instruction-loading.md")).unwrap();
        let tooling = fs::read_to_string(tooling_root.join("instruction-loading.md")).unwrap();
        let work_modules = work_root.join("instruction-loading");
        let tooling_modules = tooling_root.join("instruction-loading");

        for (work_active, tooling_active) in [(true, false), (false, true), (true, true)] {
            let work_routes = work_active.then(|| validate_modules(&work, &work_modules).unwrap());
            let tooling_routes =
                tooling_active.then(|| validate_modules(&tooling, &tooling_modules).unwrap());
            assert_eq!(work_routes.is_some(), work_active);
            assert_eq!(tooling_routes.is_some(), tooling_active);
            if let Some(routes) = work_routes {
                assert!(
                    routes
                        .iter()
                        .any(|(name, _)| name == "formal-task-validation.md")
                );
                assert!(!routes.iter().any(|(name, _)| name == "discovery.md"));
            }
            if let Some(routes) = tooling_routes {
                assert!(routes.iter().any(|(name, _)| name == "discovery.md"));
                assert!(
                    !routes
                        .iter()
                        .any(|(name, _)| name == "formal-task-validation.md")
                );
            }
        }

        let missing_work = work.replace("`formal-task-validation.md`", "`missing.md`");
        assert!(matches!(
            validate_modules(&missing_work, &work_modules),
            Err(ModuleRouteError::Missing(_))
        ));
        assert!(validate_modules(&tooling, &tooling_modules).is_ok());

        let missing_tooling = tooling.replace("`discovery.md`", "`missing.md`");
        assert!(matches!(
            validate_modules(&missing_tooling, &tooling_modules),
            Err(ModuleRouteError::Missing(_))
        ));
        assert!(validate_modules(&work, &work_modules).is_ok());
    }

    #[test]
    fn reference_table_is_one_level_and_selected_path_stays_in_branch() {
        let root = std::env::temp_dir().join(format!(
            "skill-references-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("references")).unwrap();
        fs::write(root.join("references/guide.md"), "guide").unwrap();
        let table = "## References\n\n| Reference | Load condition |\n| --- | --- |\n| `references/guide.md` | Relevant. |\n";
        assert_eq!(reference_routes(table).unwrap()[0].0, "guide");
        assert!(validate_reference_path(&root, "guide").is_ok());
        assert!(matches!(
            reference_routes(&format!("{table}| `references/guide.md` | Again. |\n")),
            Err(ReferenceRouteError::Duplicate(_))
        ));
        assert!(matches!(
            reference_routes(&table.replace("guide.md", "../guide.md")),
            Err(ReferenceRouteError::InvalidPath(_))
        ));
        assert!(matches!(
            validate_reference_path(&root, "missing"),
            Err(ReferenceRouteError::Missing(_))
        ));
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let outside = root.join("outside.md");
            fs::write(&outside, "outside").unwrap();
            symlink(&outside, root.join("references/escaped.md")).unwrap();
            assert!(matches!(
                validate_reference_path(&root, "escaped"),
                Err(ReferenceRouteError::Escaped(_))
            ));
        }
    }

    #[test]
    fn installed_work_references_are_declared_once_by_parent() {
        let root = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../skills/work/references/instructions"
        ));
        let mut pending = vec![root.to_path_buf()];
        let mut routes = 0;
        while let Some(directory) = pending.pop() {
            for entry in fs::read_dir(&directory).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() && path.file_name().unwrap() != "references" {
                    pending.push(path);
                }
            }
            let entrypoint = directory.join("instructions.md");
            if !entrypoint.is_file() {
                continue;
            }
            let content = fs::read_to_string(entrypoint).unwrap();
            for (name, _) in reference_routes(&content).unwrap() {
                validate_reference_path(&directory, &name).unwrap();
                routes += 1;
            }
            if directory.join("references").is_dir() {
                let count = fs::read_dir(directory.join("references")).unwrap().count();
                assert_eq!(count, reference_routes(&content).unwrap().len());
            }
        }
        assert_eq!(routes, 9);
    }
}
