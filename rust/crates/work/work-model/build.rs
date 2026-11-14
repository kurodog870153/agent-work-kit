use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[path = "../../common/contract-core/src/resource_refs.rs"]
mod resource_refs;

fn collect_json(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) {
    println!("cargo:rerun-if-changed={}", directory.display());
    for entry in fs::read_dir(directory).expect("read contract resource directory") {
        let path = entry.expect("read contract resource entry").path();
        if path.is_dir() {
            if !matches!(
                path.file_name().and_then(|name| name.to_str()),
                Some("schemas" | "examples" | "fixtures")
            ) {
                collect_json(root, &path, files);
            }
        } else if path
            .extension()
            .is_some_and(|extension| extension == "json")
            && directory != root
        {
            files.push(
                path.strip_prefix(root)
                    .expect("contract path under resource root")
                    .to_owned(),
            );
        }
    }
}

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest directory"));
    let resources = manifest.join("resources/contract");
    let mut files = Vec::new();
    collect_json(&resources, &resources, &mut files);
    files.sort();

    let mut ids = BTreeSet::new();
    let mut generated = String::from("const SEGMENTS: &[(&str, &str)] = &[\n");
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("build output directory"));
    for relative in files {
        let id = relative
            .file_stem()
            .and_then(|name| name.to_str())
            .expect("UTF-8 contract ID");
        assert!(
            id.starts_with("work-")
                && id
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'),
            "invalid contract ID filename: {id}"
        );
        assert!(
            ids.insert(id.to_owned()),
            "duplicate contract ID filename: {id}"
        );
        let relative = relative
            .to_str()
            .expect("UTF-8 contract path")
            .replace('\\', "/");
        let source = resources.join(&relative);
        let mut record: serde_json::Value =
            serde_json::from_slice(&fs::read(&source).expect("read contract segment"))
                .expect("parse contract segment");
        resource_refs::expand(&mut record, &resources);
        let materialized = output.join(format!("{id}.json"));
        fs::write(
            &materialized,
            serde_json::to_vec(&record).expect("serialize contract segment"),
        )
        .expect("write materialized contract segment");
        generated.push_str(&format!(
            "    ({id:?}, include_str!({:?})),\n",
            materialized.to_str().expect("UTF-8 materialized path")
        ));
    }
    generated.push_str("];\n");
    fs::write(output.join("contract_segments.rs"), generated)
        .expect("write contract segment manifest");
}
