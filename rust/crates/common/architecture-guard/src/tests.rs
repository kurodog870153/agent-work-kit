//! Architecture guard: dependency matrix, domain isolation and forbidden APIs.
//!
//! The checks run against the real manifests and sources of this workspace, so
//! a violation fails CI rather than relying on review alone.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Layers allowed to be depended on, per declaring layer, inside one domain.
fn allowed_dependencies(layer: &str) -> &'static [&'static str] {
    match layer {
        "model" => &[],
        "operations" => &["model"],
        "feature" => &["model", "operations"],
        "infrastructure" => &["model", "operations", "feature"],
        "flow" => &["model", "feature"],
        "cli" => &["model", "flow", "infrastructure"],
        _ => &[],
    }
}

const LAYERS: [&str; 6] = [
    "model",
    "operations",
    "feature",
    "infrastructure",
    "flow",
    "cli",
];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("workspace root")
}

/// Split `<domain>-<layer>` into parts; the layer is always the suffix.
fn split_crate_name(name: &str) -> Option<(&str, &str)> {
    let (domain, layer) = name.rsplit_once('-')?;
    LAYERS.contains(&layer).then_some((domain, layer))
}

fn crate_directories() -> Vec<PathBuf> {
    let mut found = Vec::new();
    for domain in ["common", "work", "skill"] {
        let root = workspace_root().join("crates").join(domain);
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            if entry.path().join("Cargo.toml").is_file() {
                found.push(entry.path());
            }
        }
    }
    found.sort();
    found
}

/// Normalised dependency entries of one manifest: name and optional path.
fn manifest_dependencies(manifest: &str, section: &str) -> Vec<(String, Option<String>)> {
    let mut dependencies = Vec::new();
    let Some(start) = manifest.find(&format!("[{section}]")) else {
        return dependencies;
    };
    let body = &manifest[start + section.len() + 2..];
    let body = body.split("\n[").next().unwrap_or(body);
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        let name = name.trim().to_owned();
        let path = value
            .split("path")
            .nth(1)
            .and_then(|rest| rest.split('"').nth(1))
            .map(str::to_owned);
        dependencies.push((name, path));
    }
    dependencies
}

fn dependency_sections() -> [&'static str; 4] {
    [
        "dependencies",
        "build-dependencies",
        "dev-dependencies",
        "target.'cfg(windows)'.dependencies",
    ]
}

/// Map crate name to its declaring domain and layer.
fn crate_index() -> BTreeMap<String, (String, String, PathBuf)> {
    let mut index = BTreeMap::new();
    for directory in crate_directories() {
        let Ok(manifest) = std::fs::read_to_string(directory.join("Cargo.toml")) else {
            continue;
        };
        let Some(name) = manifest
            .lines()
            .find_map(|line| line.trim().strip_prefix("name = "))
            .map(|value| value.trim().trim_matches('"').to_owned())
        else {
            continue;
        };
        let domain = directory
            .parent()
            .and_then(Path::file_name)
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or_default()
            .to_owned();
        let layer = name.rsplit_once('-').map(|(_, layer)| layer.to_owned());
        let Some(layer) = layer else {
            continue;
        };
        index.insert(name, (domain, layer, directory));
    }
    index
}

#[test]
fn every_domain_has_exactly_the_six_layers() {
    let index = crate_index();
    for domain in ["work", "skill"] {
        let layers: BTreeSet<&str> = index
            .values()
            .filter(|(owner, _, _)| owner == domain)
            .map(|(_, layer, _)| layer.as_str())
            .collect();
        assert_eq!(
            layers,
            LAYERS.into_iter().collect::<BTreeSet<_>>(),
            "{domain} must declare exactly the six layers"
        );
    }
}

#[test]
fn obsolete_crates_are_gone() {
    let index = crate_index();
    for name in [
        "tooling-cli",
        "tooling-feature",
        "tooling-flow",
        "tooling-model",
        "skill-api",
    ] {
        assert!(
            !index.contains_key(name),
            "obsolete crate still present: {name}"
        );
    }
}

#[test]
fn dependencies_follow_the_matrix_and_stay_inside_the_domain() {
    let index = crate_index();
    let mut violations = Vec::new();
    for (name, (domain, layer, directory)) in &index {
        let manifest = std::fs::read_to_string(directory.join("Cargo.toml")).expect("manifest");
        for section in dependency_sections() {
            for (dependency, _) in manifest_dependencies(&manifest, section) {
                let Some((other_domain, other_layer, _)) = index.get(&dependency) else {
                    continue;
                };
                if other_domain != "common" && other_domain != domain {
                    violations.push(format!(
                        "{name} crosses into {other_domain} via {dependency}"
                    ));
                    continue;
                }
                if other_domain == "common" {
                    continue;
                }
                if !allowed_dependencies(layer).contains(&other_layer.as_str()) {
                    violations.push(format!(
                        "{name} ({layer}) may not depend on {dependency} ({other_layer})"
                    ));
                }
            }
        }
    }
    assert!(violations.is_empty(), "{violations:#?}");
}

#[test]
fn work_and_skill_do_not_share_library_crates() {
    let index = crate_index();
    for (name, (domain, _, directory)) in &index {
        if domain == "common" {
            continue;
        }
        let manifest = std::fs::read_to_string(directory.join("Cargo.toml")).expect("manifest");
        for section in dependency_sections() {
            for (dependency, _) in manifest_dependencies(&manifest, section) {
                let Some((other, _, _)) = index.get(&dependency) else {
                    continue;
                };
                if other != "common" && other != domain {
                    panic!("{name} depends on another domain's crate: {dependency}");
                }
            }
        }
    }
}

/// Forbidden dynamic types. Built at run time so this file never matches itself.
fn forbidden_patterns() -> [String; 3] {
    [
        ["serde_json", "::", "Value"].concat(),
        ["serde_json", "::", "Map"].concat(),
        ["Box<dyn ", "Any>"].concat(),
    ]
}

fn find_forbidden(text: &str) -> Vec<String> {
    forbidden_patterns()
        .into_iter()
        .filter(|pattern| text.contains(pattern.as_str()))
        .collect()
}

#[test]
fn no_crate_uses_serde_json_value_or_a_dynamic_stand_in() {
    let mut files = Vec::new();
    for directory in crate_directories() {
        collect_sources(&directory, &mut files);
    }
    let mut hits = Vec::new();
    for file in files {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        hits.extend(
            find_forbidden(&text)
                .into_iter()
                .map(|pattern| format!("{}: {pattern}", file.display())),
        );
    }
    assert!(hits.is_empty(), "{hits:#?}");
}

#[test]
fn forbidden_dynamic_types_are_detected() {
    let value = ["let v: serde_json", "::", "Value = todo!();"].concat();
    assert_eq!(find_forbidden(&value).len(), 1);
    let boxed = ["x: Box<dyn ", "Any>"].concat();
    assert_eq!(find_forbidden(&boxed).len(), 1);
    assert!(find_forbidden("let v: String = String::new();").is_empty());
}

fn collect_sources(directory: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_sources(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

#[test]
fn no_fixture_directories_exist() {
    let mut offenders = Vec::new();
    for directory in crate_directories() {
        find_directories(&directory, "fixtures", &mut offenders);
    }
    assert!(offenders.is_empty(), "{offenders:#?}");
}

fn find_directories(directory: &Path, name: &str, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if path.file_name().and_then(std::ffi::OsStr::to_str) == Some(name) {
            found.push(path.clone());
        }
        find_directories(&path, name, found);
    }
}

#[test]
fn workspace_members_exclude_the_tooling_domain() {
    let manifest = std::fs::read_to_string(workspace_root().join("Cargo.toml")).expect("manifest");
    assert!(
        !manifest.contains("crates/tooling"),
        "tooling domain still a member"
    );
    assert!(manifest.contains("crates/common/*"));
    assert!(manifest.contains("crates/work/*"));
    assert!(manifest.contains("crates/skill/*"));
}

#[test]
fn guard_rules_themselves_reject_violations() {
    assert!(!allowed_dependencies("flow").contains(&"infrastructure"));
    assert!(!allowed_dependencies("cli").contains(&"operations"));
    assert!(!allowed_dependencies("model").contains(&"model"));
    assert_eq!(split_crate_name("work-flow"), Some(("work", "flow")));
    assert_eq!(split_crate_name("contract-core"), None);
}

/// Public response types are owned by the Model. The Flow layer may convert
/// requests and assemble responses but must not define a competing public DTO.
fn declares_public_flow_data(line: &str) -> bool {
    let line = line.trim_start();
    ["pub struct ", "pub enum ", "pub type "]
        .iter()
        .any(|prefix| line.starts_with(prefix))
}

#[test]
fn flow_layers_do_not_define_public_data_types() {
    for domain in ["work", "skill"] {
        let directory = workspace_root()
            .join("crates")
            .join(domain)
            .join(format!("{domain}-flow"))
            .join("src");
        let mut files = Vec::new();
        collect_sources(&directory, &mut files);
        assert!(!files.is_empty(), "{domain} flow sources must exist");
        for file in files {
            let text = std::fs::read_to_string(&file).expect("flow source");
            for (line_number, line) in text.lines().enumerate() {
                assert!(
                    !declares_public_flow_data(line),
                    "public response or DTO type belongs to {domain}-model, not {}:{}",
                    file.display(),
                    line_number + 1
                );
            }
        }
    }
}

#[test]
fn flow_public_data_guard_detects_regressions() {
    assert!(declares_public_flow_data("pub struct OutputDto {"));
    assert!(declares_public_flow_data("    pub enum Envelope<T> {"));
    assert!(declares_public_flow_data("pub type ResultDto = String;"));
    assert!(!declares_public_flow_data("pub fn execute() {}"));
    assert!(!declares_public_flow_data("struct LocalContext {}"));
}

/// Public response schemas are assigned at Flow/CLI boundaries, never by
/// Operations or Feature. Model owns the constants and data shapes.
fn assigns_public_response_schema(line: &str) -> bool {
    let line = line.trim();
    line.starts_with("schema:") && line.contains("_RESPONSE")
}

#[test]
fn business_layers_do_not_assemble_public_responses() {
    for domain in ["work", "skill"] {
        for layer in ["operations", "feature"] {
            let directory = workspace_root()
                .join("crates")
                .join(domain)
                .join(format!("{domain}-{layer}"))
                .join("src");
            let mut files = Vec::new();
            collect_sources(&directory, &mut files);
            for file in files {
                let source = std::fs::read_to_string(&file).expect("source");
                for (index, line) in source.lines().enumerate() {
                    assert!(
                        !assigns_public_response_schema(line),
                        "public response schema belongs at the Flow/CLI boundary: {}:{}",
                        file.display(),
                        index + 1
                    );
                }
            }
        }
    }
}

#[test]
fn response_schema_guard_detects_regressions() {
    assert!(assigns_public_response_schema("schema: TRACE_RESPONSE,"));
    assert!(assigns_public_response_schema(
        "    schema: PROGRESS_RESPONSE,"
    ));
    assert!(!assigns_public_response_schema(
        "schema: DISCUSSION_SCHEMA.to_owned(),"
    ));
}
