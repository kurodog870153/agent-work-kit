//! Filesystem instruction source loader.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::json;
use work_feature::error::{ExitCode, WorkError};
use work_feature::instruction::InstructionSourceRepository;
use work_operations::hierarchy::Hierarchy;
use work_operations::instruction::{LoadedSource, SourceSet, from_sources};

use crate::hierarchy_catalog::LocalHierarchyCatalog;

fn error(code: ExitCode, reason: &str, message: &str, details: serde_json::Value) -> WorkError {
    WorkError::new(code, reason, message, details)
}

fn load_file(
    root: &Path,
    path: PathBuf,
    kind: &str,
    logical_name: &str,
    boundary: Option<&Path>,
) -> Result<LoadedSource, WorkError> {
    use skill_api::source::SourceLoadError;
    #[cfg(test)]
    let tag_field = crate::fixture_support::historical_instruction_tag_field(&path);
    #[cfg(not(test))]
    let tag_field = "tags";
    #[cfg(test)]
    let historical_reference = tag_field == "work-tags" && kind == "reference";
    #[cfg(not(test))]
    let historical_reference = false;
    let loaded = skill_api::source::load_file(
        root,
        &path,
        if historical_reference {
            "instruction"
        } else {
            kind
        },
        logical_name,
        boundary,
        tag_field,
    )
    .map_err(|failure| match failure {
        SourceLoadError::Missing => error(
            ExitCode::ArtifactIntegrity,
            "instruction_source_missing",
            "A required instruction source does not exist.",
            json!({"logical_name": logical_name, "path": path}),
        ),
        SourceLoadError::NotFile => error(
            ExitCode::ArtifactIntegrity,
            "instruction_source_not_file",
            "A required instruction source is not a regular file.",
            json!({"logical_name": logical_name, "path": path}),
        ),
        SourceLoadError::EscapesRoot(resolved) => error(
            ExitCode::ArtifactIntegrity,
            "instruction_source_escapes_skill_root",
            "An instruction source resolves outside the Work skill root.",
            json!({"logical_name": logical_name, "declared_path": path, "resolved_path": resolved}),
        ),
        SourceLoadError::EscapesBoundary(resolved) => error(
            ExitCode::ArtifactIntegrity,
            "instruction_reference_escapes_hierarchy",
            "An instruction reference resolves outside its declaring hierarchy.",
            json!({"logical_name": logical_name, "declared_path": path, "resolved_path": resolved}),
        ),
        SourceLoadError::ReadFailed => error(
            ExitCode::IoFailure,
            "file_read_failed",
            "The file could not be read.",
            json!({"path": path}),
        ),
        SourceLoadError::InvalidUtf8(byte_offset) => error(
            ExitCode::InputFormat,
            "invalid_utf8",
            "The input is not valid UTF-8.",
            json!({"source": path, "byte_offset": byte_offset}),
        ),
        SourceLoadError::InvalidBom => error(
            ExitCode::InputFormat,
            "invalid_instruction_source_bom",
            "The instruction source has more than one leading UTF-8 BOM.",
            json!({"source": path}),
        ),
        SourceLoadError::ReferenceMetadata(reason) => {
            let message = match reason {
                "unterminated" => "Reference YAML frontmatter is not terminated.",
                "missing" => "Reference YAML frontmatter is missing.",
                "invalid_yaml" => "Reference YAML frontmatter is invalid.",
                "invalid_object" => "Reference YAML frontmatter must be an object.",
                _ => {
                    "Reference YAML metadata must contain the expected name, description and tags."
                }
            };
            let details = if reason == "invalid_fields" {
                json!({"source": path, "logical_name": logical_name})
            } else {
                json!({"source": path})
            };
            error(
                ExitCode::InputFormat,
                "invalid_instruction_reference_metadata",
                message,
                details,
            )
        }
    })?;
    if historical_reference {
        Ok(LoadedSource {
            summary: work_operations::instruction::source_summary(
                "reference",
                logical_name,
                &loaded.canonical_content,
            ),
            canonical_content: loaded.canonical_content,
        })
    } else {
        Ok(loaded)
    }
}

#[cfg(test)]
fn validate_reference_metadata(
    content: &[u8],
    logical_name: &str,
    path: &Path,
) -> Result<(), WorkError> {
    skill_api::source::validate_reference_metadata(content, logical_name, "tags").map_err(
        |failure| {
            use skill_api::source::SourceLoadError;
            let reason = match failure {
                SourceLoadError::ReferenceMetadata(reason) => reason,
                _ => unreachable!("metadata validation returns only metadata errors"),
            };
            let message = match reason {
                "unterminated" => "Reference YAML frontmatter is not terminated.",
                "missing" => "Reference YAML frontmatter is missing.",
                "invalid_yaml" => "Reference YAML frontmatter is invalid.",
                "invalid_object" => "Reference YAML frontmatter must be an object.",
                _ => {
                    "Reference YAML metadata must contain the expected name, description and tags."
                }
            };
            let details = if reason == "invalid_fields" {
                json!({"source": path, "logical_name": logical_name})
            } else {
                json!({"source": path})
            };
            error(
                ExitCode::InputFormat,
                "invalid_instruction_reference_metadata",
                message,
                details,
            )
        },
    )
}

fn valid_reference_name(name: &str) -> bool {
    work_operations::hierarchy::valid_segment(name)
}

fn route(
    mode: &str,
    hierarchy: &Hierarchy,
    references: &[String],
) -> Result<BTreeMap<String, Vec<(String, String)>>, WorkError> {
    let mut seen = HashSet::new();
    let mut routes: BTreeMap<_, Vec<_>> = hierarchy
        .resolved_paths
        .iter()
        .map(|path| (path.clone(), Vec::new()))
        .collect();
    let mut candidates = hierarchy.resolved_paths.clone();
    candidates.sort_by_key(|path| std::cmp::Reverse(path.split('/').count()));
    for name in references {
        if name.is_empty() {
            return Err(error(
                ExitCode::Contract,
                "invalid_instruction_reference",
                "An instruction reference logical name must be a non-empty string.",
                json!({}),
            ));
        }
        if !seen.insert(name) {
            return Err(error(
                ExitCode::Contract,
                "duplicate_instruction_reference",
                "Instruction reference logical names must be unique.",
                json!({}),
            ));
        }
        let Some((path, suffix)) = candidates.iter().find_map(|path| {
            let prefix = format!("{mode}.{}.", path.replace('/', "."));
            name.strip_prefix(&prefix).map(|suffix| (path, suffix))
        }) else {
            return Err(error(
                ExitCode::Contract,
                "unroutable_instruction_reference",
                "The instruction reference does not belong to a selected hierarchy.",
                json!({"logical_name": name}),
            ));
        };
        if !valid_reference_name(suffix) {
            return Err(error(
                ExitCode::Contract,
                "invalid_instruction_reference",
                "The instruction reference name must be lowercase kebab-case.",
                json!({"logical_name": name}),
            ));
        }
        routes
            .get_mut(path)
            .expect("candidate is a route")
            .push((name.clone(), suffix.into()));
    }
    Ok(routes)
}

impl InstructionSourceRepository for LocalHierarchyCatalog {
    fn load_sources(
        &self,
        mode: &str,
        hierarchy: &Hierarchy,
        references: &[String],
    ) -> Result<SourceSet, WorkError> {
        let routes = route(mode, hierarchy, references)?;
        let root = self.skill_root.canonicalize().map_err(|_| {
            error(
                ExitCode::IoFailure,
                "skill_root_missing",
                "The Work skill root does not exist.",
                json!({"path": self.skill_root}),
            )
        })?;
        let mut sources = vec![
            load_file(
                &root,
                self.skill_root.join("references/instruction-loading.md"),
                "workflow",
                "work.instruction-loading",
                None,
            )?,
            load_file(
                &root,
                self.skill_root
                    .join("references/workflows")
                    .join(format!("{mode}.md")),
                "workflow",
                &format!("work.workflow.{mode}"),
                None,
            )?,
        ];
        for path in &hierarchy.resolved_paths {
            let directory = self
                .skill_root
                .join("references/instructions")
                .join(mode)
                .join(path);
            sources.push(load_file(
                &root,
                directory.join("instructions.md"),
                "instruction",
                &format!("{mode}.{}", path.replace('/', ".")),
                None,
            )?);
            let declared = skill_api::bootstrap::reference_routes(
                std::str::from_utf8(&sources.last().unwrap().canonical_content)
                    .expect("loaded source is UTF-8"),
            )
            .map_err(|failure| {
                error(
                    ExitCode::InputFormat,
                    "invalid_instruction_reference_route",
                    "The instruction Reference table is invalid.",
                    json!({"mode": mode, "path": path, "failure": format!("{failure:?}")}),
                )
            })?;
            #[cfg(test)]
            let historical =
                crate::fixture_support::historical_instruction_tag_field(&directory) == "work-tags";
            #[cfg(not(test))]
            let historical = false;
            if let Some(references) = routes.get(path) {
                let boundary = directory.canonicalize().map_err(|_| {
                    error(
                        ExitCode::ArtifactIntegrity,
                        "instruction_source_missing",
                        "A required instruction source does not exist.",
                        json!({"path": directory}),
                    )
                })?;
                for (logical_name, name) in references {
                    if !historical
                        && !declared
                            .iter()
                            .any(|(declared_name, _)| declared_name == name)
                    {
                        return Err(error(
                            ExitCode::Contract,
                            "instruction_reference_not_declared",
                            "A selected reference is not declared by its instruction node.",
                            json!({"mode": mode, "path": path, "logical_name": logical_name}),
                        ));
                    }
                    sources.push(load_file(
                        &root,
                        directory.join("references").join(format!("{name}.md")),
                        "reference",
                        logical_name,
                        Some(&boundary),
                    )?);
                }
            }
        }
        Ok(from_sources(mode, hierarchy.clone(), sources))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use work_feature::instruction::{
        load, parse_selection, select, task_document_selection, validate_selection,
        validate_task_document_selection, validate_work_selection_value,
    };
    use work_operations::instruction::selection;

    #[test]
    fn reference_yaml_identity_comes_from_path_and_rejects_legacy_field() {
        let path = Path::new("task/general/references/task-records.md");
        let valid = b"---\nname: Records\ndescription: Task records.\nmetadata:\n  tags:\n    - task-records\n---\n\nBody.\n";
        validate_reference_metadata(valid, "task.general.task-records", path).unwrap();
        validate_reference_metadata(valid, "task.general.other", path).unwrap();
        assert_eq!(
            validate_reference_metadata(
                b"---\nname: Records\ndescription: Task records.\nreference-name: task.other.task-records\nmetadata:\n  tags:\n    - task-records\n---\n\nBody.\n",
                "task.general.task-records",
                path,
            )
            .unwrap_err()
            .reason_code,
            "invalid_instruction_reference_metadata"
        );
    }

    #[test]
    fn task_owned_selection_checks_confirmed_hierarchy_and_current_sources() {
        let repository = LocalHierarchyCatalog {
            skill_root: PathBuf::from(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../../skills/work"
            )),
        };
        let confirmed = work_feature::hierarchy::build_selection(
            &repository,
            &json!({"decision":"instruction_paths","selections":[
                {"path":"web/backend","recommendation_reason":"Backend requirement."}]}),
        )
        .unwrap();
        let paths = ["web/backend".into()];
        let references = ["task.general.task-records".into()];
        let selected =
            work_feature::instruction::select_task(&repository, &confirmed, &paths, &references)
                .unwrap();
        assert_eq!(
            work_feature::instruction::validate_task_selection(&repository, &confirmed, &selected)
                .unwrap()
                .mode,
            "task"
        );
        assert_eq!(
            work_feature::instruction::select_task(
                &repository,
                &confirmed,
                &["web/frontend".into()],
                &[],
            )
            .unwrap_err()
            .reason_code,
            "task_hierarchy_path_not_authorized"
        );
        let mut stale = selected.clone();
        stale.sources[0].canonical_sha256 = "0".repeat(64);
        assert_eq!(
            work_feature::instruction::validate_task_selection(&repository, &confirmed, &stale)
                .unwrap_err()
                .reason_code,
            "instruction_selection_sources_mismatch"
        );
        {
            let mode = "execute";
            let mut foreign = load(&repository, mode, &[], &[]).unwrap();
            assert_eq!(
                task_document_selection(&[foreign.clone()])
                    .unwrap_err()
                    .reason_code,
                "task_instruction_source_mode_mismatch"
            );
            foreign.mode = "task".into();
            assert_eq!(
                task_document_selection(&[foreign]).unwrap_err().reason_code,
                "task_instruction_source_mode_mismatch"
            );
        }
    }

    #[test]
    fn work_instruction_selection_validates_topology_and_fingerprint() {
        let repository = LocalHierarchyCatalog {
            skill_root: PathBuf::from(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../../skills/work"
            )),
        };
        let selected = select(&repository, "task", &[], &[]).unwrap();
        let value = serde_json::to_value(&selected).unwrap();
        assert_eq!(value["selected_paths"], json!([]));
        assert_eq!(value["resolved_paths"], json!(["general"]));
        assert_eq!(
            validate_work_selection_value(
                &repository,
                "task",
                &value,
                &[],
                "instruction_selection"
            )
            .unwrap()
            .instructions_sha256,
            selected.instructions_sha256
        );
        let mut wrong_paths = value.clone();
        wrong_paths["selected_paths"] = json!(["web"]);
        assert_eq!(
            validate_work_selection_value(
                &repository,
                "task",
                &wrong_paths,
                &[],
                "instruction_selection"
            )
            .unwrap_err()
            .reason_code,
            "work_instruction_selection_selected_paths_mismatch"
        );
        let mut stale = value;
        stale["instructions_sha256"] = json!("0".repeat(64));
        assert_eq!(
            validate_work_selection_value(
                &repository,
                "task",
                &stale,
                &[],
                "instruction_selection"
            )
            .unwrap_err()
            .reason_code,
            "work_instructions_fingerprint_mismatch"
        );
    }

    #[test]
    fn installed_sources_keep_fingerprint_and_order() {
        let repository = LocalHierarchyCatalog {
            skill_root: PathBuf::from(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../../skills/work"
            )),
        };
        let loaded = load(
            &repository,
            "task",
            &["programming-language/java/spring-boot".into()],
            &[],
        )
        .unwrap();
        assert!(work_operations::protocol::valid_sha256(
            &loaded.instructions_sha256
        ));
        assert_eq!(
            loaded
                .sources
                .iter()
                .map(|source| source.summary.logical_name.as_str())
                .collect::<Vec<_>>(),
            [
                "work.instruction-loading",
                "work.workflow.task",
                "task.general",
                "task.programming-language",
                "task.programming-language.java",
                "task.programming-language.java.spring-boot"
            ]
        );
        assert!(
            serde_json::to_value(&loaded.sources[0].summary)
                .unwrap()
                .get("compatibility_revision")
                .is_none()
        );
        validate_selection(&repository, "task", &selection(&loaded)).unwrap();
        let mut stale = selection(&loaded);
        stale.sources[0].canonical_sha256 = "0".repeat(64);
        assert_eq!(
            validate_selection(&repository, "task", &stale)
                .unwrap_err()
                .reason_code,
            "instruction_selection_sources_mismatch"
        );
        let parsed = parse_selection(
            &serde_json::to_value(selection(&loaded)).unwrap(),
            "instruction_selection",
        )
        .unwrap();
        assert_eq!(parsed, selection(&loaded));
        let union = task_document_selection(&[loaded.clone(), loaded.clone()]).unwrap();
        assert_eq!(union["instructions_sha256"], loaded.instructions_sha256);
        assert_eq!(
            union["sources"].as_array().unwrap().len(),
            loaded.sources.len()
        );
        assert_eq!(
            load(
                &repository,
                "task",
                &["programming-language/java/spring-boot".into()],
                &["task.programming-language.java.swagger".into()]
            )
            .unwrap()
            .references,
            ["task.programming-language.java.swagger"]
        );
        assert_eq!(
            load(&repository, "task", &[], &["task.unknown.reference".into()])
                .unwrap_err()
                .reason_code,
            "unroutable_instruction_reference"
        );
    }

    #[test]
    fn typescript_language_and_frontend_paths_load_independently() {
        let repository = LocalHierarchyCatalog {
            skill_root: PathBuf::from(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../../skills/work"
            )),
        };
        let language = load(
            &repository,
            "task",
            &["programming-language/typescript".into()],
            &[],
        )
        .unwrap();
        assert_eq!(
            language
                .sources
                .iter()
                .map(|source| source.summary.logical_name.as_str())
                .collect::<Vec<_>>(),
            [
                "work.instruction-loading",
                "work.workflow.task",
                "task.general",
                "task.programming-language",
                "task.programming-language.typescript"
            ]
        );

        let combined = load(
            &repository,
            "task",
            &[
                "web/frontend".into(),
                "programming-language/typescript".into(),
            ],
            &[],
        )
        .unwrap();
        assert_eq!(
            combined
                .sources
                .iter()
                .map(|source| source.summary.logical_name.as_str())
                .collect::<Vec<_>>(),
            [
                "work.instruction-loading",
                "work.workflow.task",
                "task.general",
                "task.web",
                "task.web.frontend",
                "task.programming-language",
                "task.programming-language.typescript"
            ]
        );
    }

    #[test]
    fn independent_paths_load_in_order_and_project_plan_to_available_ancestors() {
        let root = std::env::temp_dir().join(format!(
            "work-independent-paths-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let write = |relative: &str, content: &str| {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        };
        let metadata = "---\nname: Test\ndescription: Test instructions.\nmetadata:\n  tags:\n    - test-tag\n---\n\nBody.\n";
        let persistence_metadata = format!(
            "{metadata}\n## References\n\n| Reference | Load condition |\n| --- | --- |\n| `references/relational-data.md` | Relevant. |\n"
        );
        write("references/instruction-loading.md", "loading\n");
        for mode in ["task", "execute"] {
            write(&format!("references/workflows/{mode}.md"), "workflow\n");
            for path in [
                "general",
                "web",
                "web/backend",
                "programming-language",
                "programming-language/java",
            ] {
                write(
                    &format!("references/instructions/{mode}/{path}/instructions.md"),
                    metadata,
                );
            }
        }
        for mode in ["task", "execute"] {
            for path in [
                "programming-language/java/spring-boot",
                "programming-language/java/persistence",
                "programming-language/java/persistence/jpa",
            ] {
                write(
                    &format!("references/instructions/{mode}/{path}/instructions.md"),
                    if mode == "task" && path == "programming-language/java/persistence" {
                        &persistence_metadata
                    } else {
                        metadata
                    },
                );
            }
        }
        write(
            "references/instructions/task/programming-language/java/persistence/references/relational-data.md",
            "---\nname: Relational data\ndescription: Relational data.\nmetadata:\n  tags: [relational-data]\n---\nrelational data\n",
        );
        let repository = LocalHierarchyCatalog { skill_root: root };
        let paths = [
            "web/backend".into(),
            "programming-language/java/spring-boot".into(),
            "programming-language/java/persistence/jpa".into(),
        ];
        let task = load(
            &repository,
            "task",
            &paths,
            &["task.programming-language.java.persistence.relational-data".into()],
        )
        .unwrap();
        assert_eq!(task.hierarchy.selected_paths, paths);
        assert_eq!(
            task.sources
                .iter()
                .map(|source| source.summary.logical_name.as_str())
                .collect::<Vec<_>>(),
            [
                "work.instruction-loading",
                "work.workflow.task",
                "task.general",
                "task.web",
                "task.web.backend",
                "task.programming-language",
                "task.programming-language.java",
                "task.programming-language.java.spring-boot",
                "task.programming-language.java.persistence",
                "task.programming-language.java.persistence.relational-data",
                "task.programming-language.java.persistence.jpa"
            ]
        );
        assert_eq!(
            task.sources
                .iter()
                .filter(|source| source.summary.logical_name == "task.programming-language.java")
                .count(),
            1
        );
        let execute = load(&repository, "execute", &paths, &[]).unwrap();
        assert_eq!(
            execute.hierarchy.resolved_paths,
            task.hierarchy.resolved_paths
        );
        assert_eq!(
            load(
                &repository,
                "task",
                &[
                    "programming-language/java".into(),
                    "programming-language/java/spring-boot".into()
                ],
                &[]
            )
            .unwrap_err()
            .reason_code,
            "redundant_hierarchy_path"
        );
    }

    #[test]
    fn instruction_selection_round_trips_and_rejects_legacy_or_stale_values() {
        let repository = LocalHierarchyCatalog {
            skill_root: PathBuf::from(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../../skills/work"
            )),
        };
        let selected = select(
            &repository,
            "task",
            &["programming-language/java/spring-boot".into()],
            &[
                "task.programming-language.java.swagger".into(),
                "task.general.task-records".into(),
            ],
        )
        .unwrap();
        assert_eq!(
            selected.references,
            [
                "task.general.task-records",
                "task.programming-language.java.swagger"
            ]
        );
        assert_eq!(
            selected.selected_paths,
            ["programming-language/java/spring-boot"]
        );
        assert_eq!(
            selected.resolved_paths,
            [
                "general",
                "programming-language",
                "programming-language/java",
                "programming-language/java/spring-boot"
            ]
        );
        assert_eq!(
            validate_selection(&repository, "task", &selected)
                .unwrap()
                .instructions_sha256,
            selected.instructions_sha256
        );
        let mut legacy = serde_json::to_value(&selected).unwrap();
        legacy["sources"][0]["layer"] = json!("project");
        let error = parse_selection(&legacy, "instruction_selection").unwrap_err();
        assert_eq!(error.reason_code, "invalid_object_fields");
        assert_eq!(error.details["unknown"], json!(["layer"]));
        let mut changed = selected.clone();
        changed.references.reverse();
        assert_eq!(
            validate_selection(&repository, "task", &changed)
                .unwrap_err()
                .reason_code,
            "instruction_selection_references_mismatch"
        );
        let mut changed = selected.clone();
        changed.sources.swap(0, 1);
        assert_eq!(
            validate_selection(&repository, "task", &changed)
                .unwrap_err()
                .reason_code,
            "instruction_selection_sources_mismatch"
        );
        let mut changed = selected;
        changed.instructions_sha256 = "0".repeat(64);
        assert_eq!(
            validate_selection(&repository, "task", &changed)
                .unwrap_err()
                .reason_code,
            "instructions_fingerprint_mismatch"
        );
        let leaves = select(
            &repository,
            "task",
            &[
                "programming-language/java/persistence/jpa".into(),
                "programming-language/java/persistence/mybatis".into(),
            ],
            &[],
        )
        .unwrap();
        assert_eq!(
            leaves.selected_paths,
            [
                "programming-language/java/persistence/jpa",
                "programming-language/java/persistence/mybatis"
            ]
        );
        assert_eq!(
            leaves.resolved_paths,
            [
                "general",
                "programming-language",
                "programming-language/java",
                "programming-language/java/persistence",
                "programming-language/java/persistence/jpa",
                "programming-language/java/persistence/mybatis"
            ]
        );
        validate_selection(&repository, "task", &leaves).unwrap();
    }

    #[test]
    fn temporary_sources_preserve_order_and_reject_missing_or_invalid_content() {
        let root = std::env::temp_dir().join(format!(
            "work-instruction-sources-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let write = |relative: &str, content: &[u8]| {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        };
        let metadata = b"---\nname: Test\ndescription: Test instructions.\nmetadata:\n  tags:\n    - test-tag\n---\n\nBody.\n";
        let general_metadata = "---\nname: Test\ndescription: Test instructions.\nmetadata:\n  tags:\n    - test-tag\n---\n\nBody.\n## References\n\n| Reference | Load condition |\n| --- | --- |\n| `references/task-records.md` | Relevant. |\n| `references/missing.md` | Missing source case. |\n| `references/invalid.md` | Invalid source case. |\n".as_bytes();
        let backend_metadata = "---\nname: Test\ndescription: Test instructions.\nmetadata:\n  tags:\n    - test-tag\n---\n\nBody.\n## References\n\n| Reference | Load condition |\n| --- | --- |\n| `references/resilience.md` | Relevant. |\n| `references/security.md` | Relevant. |\n".as_bytes();
        write(
            "references/instruction-loading.md",
            b"\xef\xbb\xbfloading\r\n\r\n",
        );
        write("references/workflows/task.md", b"workflow\n");
        for path in ["general", "web", "web/backend"] {
            write(
                &format!("references/instructions/task/{path}/instructions.md"),
                match path {
                    "general" => general_metadata,
                    "web/backend" => backend_metadata,
                    _ => metadata,
                },
            );
        }
        {
            let mode = "execute";
            write(
                &format!("references/instructions/{mode}/general/instructions.md"),
                metadata,
            );
        }
        for (name, content) in [
            (
                "references/instructions/task/general/references/task-records.md",
                b"---\nname: Records\ndescription: Records.\nmetadata:\n  tags: [records]\n---\nrecords\n".as_slice(),
            ),
            (
                "references/instructions/task/web/backend/references/resilience.md",
                b"---\nname: Resilience\ndescription: Resilience.\nmetadata:\n  tags: [resilience]\n---\nresilience\n".as_slice(),
            ),
            (
                "references/instructions/task/web/backend/references/security.md",
                b"---\nname: Security\ndescription: Security.\nmetadata:\n  tags: [security]\n---\nsecurity\n".as_slice(),
            ),
        ] {
            write(name, content);
        }
        let repository = LocalHierarchyCatalog {
            skill_root: root.clone(),
        };
        let references = [
            "task.web.backend.resilience".into(),
            "task.general.task-records".into(),
            "task.web.backend.security".into(),
        ];
        let loaded = load(&repository, "task", &["web/backend".into()], &references).unwrap();
        let catalog_selection = work_feature::hierarchy::build_selection(
            &repository,
            &json!({"decision":"general_only","selections":[]}),
        )
        .unwrap();
        assert_eq!(loaded.sources[0].canonical_content, b"loading\n");
        assert_eq!(
            loaded.sources[0].summary.canonical_sha256,
            work_operations::derivation::fingerprint::raw(b"loading\n")
        );
        assert_eq!(loaded.instructions_sha256.len(), 64);
        assert_eq!(
            loaded
                .sources
                .iter()
                .map(|source| source.summary.logical_name.as_str())
                .collect::<Vec<_>>(),
            [
                "work.instruction-loading",
                "work.workflow.task",
                "task.general",
                "task.general.task-records",
                "task.web",
                "task.web.backend",
                "task.web.backend.resilience",
                "task.web.backend.security"
            ]
        );
        assert_eq!(
            loaded.references,
            [
                "task.general.task-records",
                "task.web.backend.resilience",
                "task.web.backend.security"
            ]
        );
        assert_eq!(
            serde_json::to_value(&loaded.sources[0].summary)
                .unwrap()
                .as_object()
                .unwrap()
                .len(),
            3
        );
        assert_eq!(
            load(
                &repository,
                "task",
                &[],
                &[
                    "task.general.task-records".into(),
                    "task.general.task-records".into()
                ]
            )
            .unwrap_err()
            .reason_code,
            "duplicate_instruction_reference"
        );
        assert_eq!(
            load(
                &repository,
                "task",
                &[],
                &["task.web.backend.security".into()]
            )
            .unwrap_err()
            .reason_code,
            "unroutable_instruction_reference"
        );
        assert_eq!(
            load(
                &repository,
                "task",
                &[],
                &["task.general.undeclared".into()]
            )
            .unwrap_err()
            .reason_code,
            "instruction_reference_not_declared"
        );
        let missing = load(&repository, "task", &[], &["task.general.missing".into()]).unwrap_err();
        assert_eq!(missing.reason_code, "instruction_source_missing");
        assert_eq!(missing.details["logical_name"], "task.general.missing");
        write(
            "references/instructions/task/general/references/invalid.md",
            b"\xff",
        );
        assert_eq!(
            load(&repository, "task", &[], &["task.general.invalid".into()])
                .unwrap_err()
                .reason_code,
            "invalid_utf8"
        );
        write("references/workflows/task.md", b"changed workflow\n");
        let changed = load(&repository, "task", &["web/backend".into()], &references).unwrap();
        assert_ne!(loaded.instructions_sha256, changed.instructions_sha256);
        assert_eq!(
            validate_selection(&repository, "task", &selection(&loaded))
                .unwrap_err()
                .reason_code,
            "instruction_selection_sources_mismatch"
        );
        write(
            "references/instructions/task/general/instructions.md",
            b"---\nname: Test\ndescription: Changed tasks.\nmetadata:\n  tags:\n    - test-tag\n---\n\nBody.\n",
        );
        assert_eq!(
            work_feature::hierarchy::validate_selection(&repository, &catalog_selection)
                .unwrap_err()
                .reason_code,
            "instruction_catalog_snapshot_mismatch"
        );

        write(
            "references/instructions/execute/general/instructions.md",
            metadata,
        );
        let missing = load(&repository, "execute", &[], &[]).unwrap_err();
        assert_eq!(missing.reason_code, "instruction_source_missing");
        assert_eq!(missing.details["logical_name"], "work.workflow.execute");
        write(
            "references/workflows/execute.md",
            b"<!-- work-compatibility-revision: 3 -->\nworkflow\n",
        );
        let plan = load(&repository, "execute", &[], &[]).unwrap();
        assert_eq!(
            plan.sources[1].summary.canonical_sha256,
            work_operations::derivation::fingerprint::raw(
                b"<!-- work-compatibility-revision: 3 -->\nworkflow\n"
            )
        );
        write("references/workflows/execute.md", b"\xff");
        assert_eq!(
            load(&repository, "execute", &[], &[])
                .unwrap_err()
                .reason_code,
            "invalid_utf8"
        );
    }

    #[cfg(unix)]
    #[test]
    fn source_symlinks_cannot_escape_skill_or_declaring_hierarchy() {
        use std::os::unix::fs::symlink;

        let parent = std::env::temp_dir().join(format!(
            "work-instruction-source-links-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let metadata = b"---\nname: Test\ndescription: Test instructions.\nmetadata:\n  tags:\n    - test-tag\n---\n\nBody.\n";
        let general_metadata = "---\nname: Test\ndescription: Test instructions.\nmetadata:\n  tags:\n    - test-tag\n---\n\nBody.\n## References\n\n| Reference | Load condition |\n| --- | --- |\n| `references/task-records.md` | Relevant. |\n".as_bytes();
        let create = |label: &str| {
            let root = parent.join(label);
            for (relative, content) in [
                ("references/instruction-loading.md", b"loading\n".as_slice()),
                (
                    "references/instructions/task/general/instructions.md",
                    general_metadata,
                ),
                (
                    "references/instructions/task/web/instructions.md",
                    metadata.as_slice(),
                ),
            ] {
                let path = root.join(relative);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, content).unwrap();
            }
            fs::create_dir_all(root.join("references/workflows")).unwrap();
            root
        };
        let workflow_root = create("workflow");
        let outside = parent.join("outside.md");
        fs::write(&outside, b"outside\n").unwrap();
        symlink(&outside, workflow_root.join("references/workflows/task.md")).unwrap();
        assert_eq!(
            load(
                &LocalHierarchyCatalog {
                    skill_root: workflow_root,
                },
                "task",
                &[],
                &[]
            )
            .unwrap_err()
            .reason_code,
            "instruction_source_escapes_skill_root"
        );

        let reference_root = create("reference");
        fs::write(
            reference_root.join("references/workflows/task.md"),
            b"workflow\n",
        )
        .unwrap();
        let target =
            reference_root.join("references/instructions/task/web/references/task-records.md");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, b"outside hierarchy\n").unwrap();
        let source =
            reference_root.join("references/instructions/task/general/references/task-records.md");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        symlink(&target, source).unwrap();
        assert_eq!(
            load(
                &LocalHierarchyCatalog {
                    skill_root: reference_root,
                },
                "task",
                &[],
                &["task.general.task-records".into()]
            )
            .unwrap_err()
            .reason_code,
            "instruction_reference_escapes_hierarchy"
        );
    }

    #[test]
    fn task_document_union_and_validation_match_current_contract_selection_cases() {
        let repository = LocalHierarchyCatalog {
            skill_root: PathBuf::from(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../../skills/work"
            )),
        };
        let jpa = load(
            &repository,
            "task",
            &["programming-language/java/persistence/jpa".into()],
            &[
                "task.general.task-records".into(),
                "task.programming-language.java.persistence.relational-data".into(),
            ],
        )
        .unwrap();
        let mybatis = load(
            &repository,
            "task",
            &["programming-language/java/persistence/mybatis".into()],
            &[
                "task.general.task-records".into(),
                "task.programming-language.java.swagger".into(),
            ],
        )
        .unwrap();
        let expected = task_document_selection(&[jpa, mybatis]).unwrap();
        assert_eq!(
            expected["sources"]
                .as_array()
                .unwrap()
                .iter()
                .map(|source| source["logical_name"].as_str().unwrap())
                .collect::<Vec<_>>(),
            [
                "work.instruction-loading",
                "work.workflow.task",
                "task.general",
                "task.general.task-records",
                "task.programming-language",
                "task.programming-language.java",
                "task.programming-language.java.persistence",
                "task.programming-language.java.persistence.relational-data",
                "task.programming-language.java.persistence.jpa",
                "task.programming-language.java.swagger",
                "task.programming-language.java.persistence.mybatis"
            ]
        );
        assert_eq!(
            expected["references"],
            json!([
                "task.general.task-records",
                "task.programming-language.java.persistence.relational-data",
                "task.programming-language.java.swagger"
            ])
        );
        assert_eq!(expected["instructions_sha256"].as_str().unwrap().len(), 64);
        assert_eq!(
            validate_task_document_selection(&expected, &expected).unwrap(),
            expected
        );
        let mut invalid = expected.clone();
        invalid["sources"][0]["layer"] = json!("project");
        let error = validate_task_document_selection(&invalid, &expected).unwrap_err();
        assert_eq!(error.reason_code, "invalid_object_fields");
        assert_eq!(error.details["unknown"], json!(["layer"]));
        let mut invalid = expected.clone();
        invalid["sources"].as_array_mut().unwrap().swap(0, 1);
        assert_eq!(
            validate_task_document_selection(&invalid, &expected)
                .unwrap_err()
                .reason_code,
            "task_document_instruction_sources_mismatch"
        );
        let mut invalid = expected.clone();
        invalid["references"].as_array_mut().unwrap().reverse();
        assert_eq!(
            validate_task_document_selection(&invalid, &expected)
                .unwrap_err()
                .reason_code,
            "task_document_instruction_references_mismatch"
        );
        let mut invalid = expected.clone();
        invalid["instructions_sha256"] = json!("0".repeat(64));
        assert_eq!(
            validate_task_document_selection(&invalid, &expected)
                .unwrap_err()
                .reason_code,
            "task_document_instructions_fingerprint_mismatch"
        );
    }

    #[test]
    fn routed_reference_documents_keep_small_unlinked_modules() {
        let references = PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../skills/work/references"
        ));
        let bootstrap = fs::read_to_string(references.join("instruction-loading.md")).unwrap();
        let routes = skill_api::bootstrap::validate_modules(
            &bootstrap,
            &references.join("instruction-loading"),
        )
        .unwrap();
        assert_eq!(routes.len(), 20);
        assert!(
            routes
                .iter()
                .any(|(name, _)| name == "tool-policy-integration.md")
        );
        assert!(bootstrap.contains("source_order"));
        assert!(bootstrap.contains("REVIEW_REQUIRED"));

        let markdown_files = |directory: &std::path::Path| {
            let mut paths: Vec<_> = fs::read_dir(directory)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
                .collect();
            paths.sort();
            paths
        };
        let has_markdown_link = |content: &str| {
            content.split("](").skip(1).any(|tail| {
                tail.split(')')
                    .next()
                    .is_some_and(|target| target.contains(".md"))
            })
        };
        let common = markdown_files(&references.join("instruction-loading"));
        assert_eq!(common.len(), 20);
        for path in common {
            let content = fs::read_to_string(&path).unwrap();
            assert_eq!(
                content
                    .lines()
                    .filter(|line| line.starts_with("# "))
                    .count(),
                1
            );
            assert!(!has_markdown_link(&content), "{}", path.display());
        }
        for entry in markdown_files(&references.join("workflows")) {
            let content = fs::read_to_string(&entry).unwrap();
            assert!(content.lines().count() <= 15, "{}", entry.display());
            let modules = markdown_files(&entry.with_extension(""));
            assert!(!modules.is_empty(), "{}", entry.display());
            for module in modules {
                let content = fs::read_to_string(&module).unwrap();
                assert!(!has_markdown_link(&content), "{}", module.display());
            }
        }
    }
}
