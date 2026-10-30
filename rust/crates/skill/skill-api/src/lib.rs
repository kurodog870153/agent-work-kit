//! Stable Skill API for Rust consumers.
//!
//! Callers depend on this crate instead of the internal model, operations,
//! feature, and infrastructure crates. Each module exposes only supported
//! capabilities, so internal layering can change without changing callers.

pub mod model {
    pub use skill_model::hierarchy::HierarchyCatalog;
    pub use skill_model::instruction::{
        InstructionCatalogData, InstructionFrontmatter, InstructionFrontmatterMetadata,
        LoadedSource, SourceSummary,
    };
    pub use skill_model::{
        SkillBundle, SkillBundleFile, SkillCatalog, SkillIdentity, SkillMetadata, SkillSnapshot,
        verified,
    };
}

pub mod catalog {
    pub use skill_feature::{
        MetadataHeader, MetadataHeaderError, MetadataListError, SkillCatalogRepository,
        SkillPathError, SkillRoot, catalog, metadata_header, string_list, validate_root,
        validate_source,
    };
    pub use skill_infrastructure::{
        FrontmatterError, MetadataFileError, has_entrypoint, list_directories, read_frontmatter,
        read_metadata_file, resolve_directory,
    };
}

pub mod hierarchy {
    pub use skill_feature::{hierarchy_children, missing_ancestor};
    pub use skill_infrastructure::hierarchy::{
        HierarchyScanError, MetadataReadError, parse_metadata_bytes, read_metadata,
        scan_entrypoints,
    };
    pub use skill_operations::hierarchy::{
        PathSelectionError, ancestors, resolved_paths, sorted_paths, valid_segment,
        validate_selection,
    };
}

pub mod discovery {
    pub use skill_infrastructure::discovery::{
        DiscoveredChild, DiscoveryError, direct_children, read_metadata_only,
    };
}

pub mod bootstrap {
    pub use skill_infrastructure::bootstrap::{
        ModuleRouteError, ReferenceRouteError, reference_routes, validate_modules,
        validate_reference_path,
    };
}

pub mod source {
    pub use skill_infrastructure::hierarchy_sources::{
        HierarchySource, HierarchySourceError, load_selected,
    };
    pub use skill_infrastructure::source::{
        SourceLoadError, load_file, strip_single_bom, validate_reference_metadata,
    };
    pub use skill_operations::instruction::{references, source_summary};
}

pub mod bundle {
    pub use skill_infrastructure::bundle::{BundleReadError, read_bundle_files};
    pub use skill_operations::{BundleEntry, bundle, identity, selection};
}

pub mod canonical {
    pub use skill_operations::canonical::{
        canonical_sha256, canonical_text, raw_sha256, valid_sha256_hex,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn public_entrypoint_loads_installed_skill_and_rejects_invalid_input() {
        let root = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../skills/tooling"
        ));
        let header = catalog::read_frontmatter(&root.join("SKILL.md")).unwrap();
        assert_eq!(catalog::metadata_header(&header).unwrap().name, "tooling");
        assert!(catalog::validate_source("../SKILL.md").is_err());
        let hierarchy_root = root.join("references/instructions");
        let children =
            discovery::direct_children(&hierarchy_root, &hierarchy_root, "tags").unwrap();
        assert_eq!(children.len(), 3);
        assert!(children.iter().all(|child| child.metadata.is_ok()));
        let sources = source::load_selected(
            &hierarchy_root,
            &["interfaces/mcp"],
            |_, _, _| false,
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(sources[0].relative_path, "instructions.md");
        assert!(
            source::load_selected(
                &hierarchy_root,
                &["../outside"],
                |_, _, _| false,
                |_| Ok(())
            )
            .is_err()
        );
        assert!(
            bundle::read_bundle_files(root)
                .unwrap()
                .iter()
                .any(|file| file.path == "SKILL.md")
        );
    }

    #[test]
    fn public_model_rejects_invalid_frontmatter() {
        let template = include_str!("../../skill-model/resources/instruction-frontmatter.yaml");
        let content = format!("---\n{template}---\nBody\n");
        let valid = hierarchy::parse_metadata_bytes(content.as_bytes(), "tags").unwrap();
        assert!(!valid.tags.is_empty());
        assert!(hierarchy::parse_metadata_bytes(content.as_bytes(), "work-tags").is_err());
    }
}
