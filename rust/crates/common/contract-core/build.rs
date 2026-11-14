use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

#[path = "src/resource_refs.rs"]
mod resource_refs;
#[path = "src/validation.rs"]
mod validation;

fn read_json(path: &Path) -> Value {
    validation::read_json_file(path).unwrap_or_else(|error| panic!("{error}"))
}

fn directory_json(root: &Path, nested: bool) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let entries = fs::read_dir(root).unwrap_or_else(|error| panic!("{}: {error}", root.display()));
    for entry in entries {
        let path = entry.expect("resource entry").path();
        if path.is_dir() {
            if nested
                && !matches!(
                    path.file_name().and_then(|name| name.to_str()),
                    Some("schemas" | "examples" | "fixtures")
                )
            {
                files.extend(directory_json(&path, true));
            }
        } else if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            files.push(path);
        }
    }
    files.sort();
    files
}

fn index_command(node: &Value, parent: &str, index: &mut Map<String, Value>) {
    let name = node["name"].as_str().expect("command name");
    assert!(
        !name.is_empty() && !name.contains(char::is_whitespace),
        "invalid command name"
    );
    let path = if parent.is_empty() {
        name.to_owned()
    } else {
        format!("{parent} {name}")
    };
    let args = node["arguments"].as_array().expect("command arguments");
    let mut names = BTreeSet::new();
    let mut flags = BTreeSet::new();
    for arg in args {
        let name = arg["name"].as_str().expect("argument name");
        assert!(names.insert(name), "duplicate argument {path}: {name}");
        for flag in arg["flags"].as_array().expect("argument flags") {
            let flag = flag.as_str().expect("argument flag string");
            assert!(flags.insert(flag), "duplicate flag {path}: {flag}");
        }
    }
    for group in node["exclusive_groups"]
        .as_array()
        .expect("exclusive groups")
    {
        for member in group["members"].as_array().expect("group members") {
            let member = member.as_str().expect("group member");
            assert!(
                names.contains(member),
                "unknown exclusive group member {path}: {member}"
            );
        }
    }
    assert!(
        index.insert(path.clone(), node.clone()).is_none(),
        "duplicate command path: {path}"
    );
    for child in node["children"].as_array().expect("command children") {
        index_command(child, &path, index);
    }
}

fn expand_command_refs(node: &mut Value, source_dir: &Path) {
    let children = node["children"].as_array_mut().expect("command children");
    let mut expanded = Vec::new();
    for mut child in std::mem::take(children) {
        if let Some(reference) = child.get("ref").and_then(Value::as_str) {
            let relative = Path::new(reference);
            assert!(
                !relative.is_absolute()
                    && relative
                        .components()
                        .all(|part| matches!(part, std::path::Component::Normal(_))),
                "invalid command reference: {reference}"
            );
            let source = source_dir.join(relative);
            println!("cargo:rerun-if-changed={}", source.display());
            child = read_json(&source);
        }
        if let Some(group) = child.as_array_mut() {
            for mut command in std::mem::take(group) {
                expand_command_refs(&mut command, source_dir);
                expanded.push(command);
            }
        } else {
            expand_command_refs(&mut child, source_dir);
            expanded.push(child);
        }
    }
    *children = expanded;
}

fn main() {
    let crate_root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("crate root"));
    let source_file = crate_root.join("resources/registry.json");
    println!("cargo:rerun-if-changed={}", source_file.display());
    let sources = read_json(&source_file);
    assert_eq!(sources["schema"], "public-contract-registry-sources");
    let resolve = |name: &str| crate_root.join(sources[name].as_str().expect("source path"));

    let command_path = resolve("work_commands");
    println!("cargo:rerun-if-changed={}", command_path.display());
    let mut work_commands = read_json(&command_path);
    assert_eq!(work_commands["schema"], "work-command-tree");
    expand_command_refs(
        &mut work_commands["root"],
        command_path.parent().expect("command directory"),
    );
    let mut commands = Map::new();
    index_command(&work_commands["root"], "", &mut commands);
    let tooling_command_path = resolve("tooling_commands");
    println!("cargo:rerun-if-changed={}", tooling_command_path.display());
    let mut tooling_commands = read_json(&tooling_command_path);
    assert_eq!(tooling_commands["schema"], "tooling-command-tree");
    expand_command_refs(
        &mut tooling_commands["root"],
        tooling_command_path.parent().expect("command directory"),
    );
    index_command(&tooling_commands["root"], "", &mut commands);

    let contracts_root = resolve("work_contracts");
    println!("cargo:rerun-if-changed={}", contracts_root.display());
    let mut contracts = Map::new();
    for file in directory_json(&contracts_root, true) {
        if file.parent() == Some(contracts_root.as_path()) {
            continue; // catalog.json and scaffold-orders.json are indexes, not contracts.
        }
        println!("cargo:rerun-if-changed={}", file.display());
        let mut record = read_json(&file);
        resource_refs::expand(&mut record, &contracts_root);
        let id = record["description"]["id"]
            .as_str()
            .expect("contract ID")
            .to_owned();
        assert_eq!(
            file.file_stem().and_then(|stem| stem.to_str()),
            Some(id.as_str()),
            "contract file name mismatch"
        );
        assert!(
            contracts.insert(id.clone(), record).is_none(),
            "duplicate contract ID: {id}"
        );
    }
    let tooling_contracts_root = resolve("tooling_contracts");
    println!(
        "cargo:rerun-if-changed={}",
        tooling_contracts_root.display()
    );
    for file in directory_json(&tooling_contracts_root, true) {
        if file.file_name().and_then(|name| name.to_str()) == Some("example-catalog.json") {
            continue;
        }
        let relative = file
            .strip_prefix(&tooling_contracts_root)
            .expect("tooling contract path");
        if relative
            .components()
            .next()
            .is_some_and(|part| part.as_os_str() == "examples")
        {
            continue;
        }
        println!("cargo:rerun-if-changed={}", file.display());
        let record = read_json(&file);
        let id = record["description"]["id"]
            .as_str()
            .expect("contract ID")
            .to_owned();
        let expected = format!(
            "tooling-{}",
            relative
                .with_extension("")
                .components()
                .map(|part| part.as_os_str().to_str().expect("UTF-8 contract path"))
                .collect::<Vec<_>>()
                .join("-")
        );
        assert_eq!(id, expected, "tooling contract path and ID mismatch");
        assert!(
            contracts.insert(id.clone(), record).is_none(),
            "duplicate contract ID: {id}"
        );
    }
    for (id, record) in &contracts {
        let description = &record["description"];
        assert!(description["kind"].as_str().is_some(), "{id}: missing kind");
        assert!(
            description["caller_constructible"].is_boolean(),
            "{id}: missing caller_constructible"
        );
        for field in description["fields"].as_array().expect("contract fields") {
            if let Some(reference) = field["reference"].as_str() {
                assert!(
                    contracts.contains_key(reference),
                    "{id}: missing reference {reference}"
                );
            }
        }
    }
    for (path, command) in &commands {
        for link in ["request_contract", "response_contract"] {
            if let Some(id) = command[link].as_str() {
                assert!(contracts.contains_key(id), "{path}: missing {link} {id}");
            }
        }
    }
    let catalog = read_json(&contracts_root.join("catalog.json"));
    let mut catalog_ids = BTreeSet::new();
    for entry in catalog["catalog"]["contracts"]
        .as_array()
        .expect("catalog contracts")
    {
        let id = entry["id"].as_str().expect("catalog ID");
        assert!(catalog_ids.insert(id), "duplicate catalog ID: {id}");
        assert!(
            contracts.contains_key(id),
            "catalog references missing contract: {id}"
        );
    }

    let examples_root = resolve("tooling_examples");
    println!("cargo:rerun-if-changed={}", examples_root.display());
    let mut examples = Map::new();
    for file in directory_json(&examples_root, false) {
        println!("cargo:rerun-if-changed={}", file.display());
        let name = file
            .file_stem()
            .and_then(|stem| stem.to_str())
            .expect("example name");
        assert!(
            examples.insert(name.to_owned(), read_json(&file)).is_none(),
            "duplicate example: {name}"
        );
    }
    let example_catalog_path = resolve("tooling_example_catalog");
    println!("cargo:rerun-if-changed={}", example_catalog_path.display());
    let example_catalog = read_json(&example_catalog_path);
    validation::validate_example_catalog(&example_catalog).expect("valid Tooling example catalog");
    let work_example_policy_path = resolve("work_example_policy");
    println!(
        "cargo:rerun-if-changed={}",
        work_example_policy_path.display()
    );
    let work_example_policy = read_json(&work_example_policy_path);
    assert_eq!(work_example_policy["schema"], "work-public-example-policy");
    let mut fixture_only = BTreeSet::new();
    for id in work_example_policy["fixture_only"]
        .as_array()
        .expect("fixture-only examples")
    {
        let id = id.as_str().expect("fixture-only contract ID");
        assert!(
            id.starts_with("work-") && contracts.contains_key(id),
            "unknown fixture-only contract: {id}"
        );
        assert!(
            fixture_only.insert(id),
            "duplicate fixture-only contract: {id}"
        );
    }
    let mut public_examples = Map::new();
    for (id, record) in &contracts {
        if id.starts_with("work-") && !fixture_only.contains(id.as_str()) {
            public_examples.insert(
                id.clone(),
                json!({"default":{"valid":true,"example":record["description"]["example"]}}),
            );
        }
    }
    let mut used_files = BTreeSet::new();
    for entry in example_catalog.as_array().expect("Tooling example catalog") {
        let id = entry["contract_id"].as_str().expect("example contract ID");
        let scenario = entry["scenario"].as_str().expect("scenario");
        let file = entry["file"].as_str().expect("example file");
        assert!(
            contracts.contains_key(id),
            "example references missing contract: {id}"
        );
        assert!(
            !file.contains('/') && !file.contains('\\'),
            "example file must be a name"
        );
        let stem = file.strip_suffix(".json").expect("example JSON filename");
        let mut value = examples
            .get(stem)
            .cloned()
            .expect("example catalog references missing file");
        used_files.insert(stem.to_owned());
        if entry["extract"] == "data" {
            value = value.get("data").cloned().expect("response example data");
        } else {
            assert!(entry.get("extract").is_none(), "invalid example extraction");
        }
        let scenarios = public_examples.entry(id).or_insert_with(|| json!({}));
        assert!(scenarios.as_object_mut().expect("scenarios").insert(
            scenario.to_owned(),
            json!({"valid":entry["valid"].as_bool().expect("example validity"),"example":value}),
        ).is_none(), "duplicate example scenario: {id}/{scenario}");
    }
    assert_eq!(
        used_files.len(),
        examples.len(),
        "unclassified Tooling example file"
    );
    let query_schema_path = resolve("query_data_schemas");
    println!("cargo:rerun-if-changed={}", query_schema_path.display());
    let query_data_schemas = read_json(&query_schema_path);
    for id in [
        "contract-list-data",
        "contract-describe-data",
        "contract-scaffold-data",
        "contract-example-data",
    ] {
        assert!(
            query_data_schemas[id].is_object(),
            "missing query data schema: {id}"
        );
    }
    let output = json!({"schema":"public-contract-registry", "commands":commands, "contracts":contracts, "examples":public_examples,
        "command_trees":{"work":work_commands,"tooling":tooling_commands},"query_data_schemas":query_data_schemas});
    validation::validate_registry(&output).expect("valid public contract registry");
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("output directory"));
    fs::write(
        out_dir.join("registry.json"),
        serde_json::to_vec(&output).expect("registry JSON"),
    )
    .expect("write validated registry");
}
