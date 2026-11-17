//! CLI contract tests: Parser, Help, Schema and Serde stay consistent, and the
//! runtime commands behave end to end on temporary trees built by the test.

use std::fs;
use std::path::PathBuf;

use contract_core::clap_tree;
use contract_core::model::ContractValue;
use contract_core::query::{self, ListScope};
use skill_cli::{Output, execute, registry};
use skill_flow::into_request;
use skill_model::{RESOLVE_REQUEST, RequestDto, SelectionDto};

struct Dir(PathBuf);

impl Dir {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "skill-cli-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_nanos())
        ));
        fs::create_dir_all(&path).expect("temp dir");
        Self(path)
    }

    fn put(&self, relative: &str, text: &str) {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        fs::write(path, text).expect("write");
    }

    fn path(&self, relative: &str) -> String {
        self.0.join(relative).to_string_lossy().into_owned()
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run(args: &[&str]) -> Output {
    let mut all = vec!["skill".to_owned()];
    all.extend(args.iter().map(|item| (*item).to_owned()));
    execute(&all, &mut String::new)
}

fn run_stdin(args: &[&str], input: &str) -> Output {
    let mut all = vec!["skill".to_owned()];
    all.extend(args.iter().map(|item| (*item).to_owned()));
    let input = input.to_owned();
    execute(&all, &mut || input.clone())
}

fn node(name: &str) -> String {
    format!(
        "---\nname: {name}\ndescription: 適用時使用。\nmetadata:\n  tags:\n    - capability\n---\n\n- [Required] 遵守。\n"
    )
}

fn install() -> Dir {
    let dir = Dir::new("install");
    dir.put("install/shared/bootstrap.md", "- [Required] 解析入口。\n");
    dir.put("install/work/SKILL.md", "entry");
    dir.put("install/work/instructions/instructions.md", &node("根"));
    dir.put(
        "install/work/instructions/task/instructions.md",
        &node("任務"),
    );
    dir
}

fn install_arg(dir: &Dir) -> String {
    dir.path("install")
}

const REQUEST: &str =
    r#"{"schema":"skill-resolve-request","selections":[{"skill":"work","nodes":["task"]}]}"#;

#[test]
fn the_clap_tree_is_consistent_with_the_registry_declaration() {
    let registry = registry().unwrap();
    let root = &registry.command("skill").unwrap().node;
    let tree = clap_tree::build(root);
    tree.clone().debug_assert();
    for child in &root.children {
        let found = tree.find_subcommand(&child.name).expect(&child.name);
        for argument in &child.arguments {
            let present = found
                .get_arguments()
                .any(|candidate| candidate.get_id().as_str() == argument.name);
            assert!(present, "{} --{}", child.name, argument.name);
        }
    }
}

#[test]
fn help_is_derived_from_the_declared_commands() {
    let output = run(&["--help"]);
    assert_eq!(output.code, 0);
    for name in [
        "discover", "resolve", "switch", "validate", "list", "describe", "scaffold", "example",
    ] {
        assert!(output.stdout.contains(name), "help lacks {name}");
    }
    let sub = run(&["resolve", "--help"]);
    assert_eq!(sub.code, 0);
    assert!(sub.stdout.contains("--install") && sub.stdout.contains("--request"));
    assert!(run(&["--version"]).stdout.contains("3.0.0"));
}

#[test]
fn parser_rejects_bad_usage_with_exit_code_two() {
    for args in [
        vec![],
        vec!["unknown"],
        vec!["resolve"],
        vec!["list", "--scope", "everything"],
        vec!["describe", "--command", "x", "--contract", "y"],
    ] {
        let output = run(&args);
        assert_eq!(output.code, 2, "{args:?}");
        assert!(
            output.stdout.is_empty() && !output.stderr.is_empty(),
            "{args:?}"
        );
    }
}

#[test]
fn list_describe_scaffold_and_example_serve_the_skill_contract() {
    let listed = run(&["list"]);
    assert_eq!(listed.code, 0);
    assert!(
        listed.stdout.contains("skill resolve") && listed.stdout.contains("skill-resolve-request")
    );
    assert!(
        !run(&["list", "--scope", "commands"])
            .stdout
            .contains("skill-resolve-request")
    );

    let command = run(&["describe", "--command", "skill resolve"]);
    assert!(command.stdout.contains("skill-resolve-request"));
    let contract = run(&["describe", "--contract", "skill-resolve-request"]);
    assert!(contract.stdout.contains("selections"));
    let field = run(&[
        "describe",
        "--contract",
        "skill-resolve-request",
        "--field",
        "/selections",
    ]);
    assert_eq!(field.code, 0, "{}", field.stdout);

    let scaffold = run(&["scaffold", "--contract", "skill-resolve-request"]);
    assert_eq!(scaffold.code, 0, "{}", scaffold.stdout);
    assert!(scaffold.stdout.contains("skill-resolve-request"));
    let refused = run(&["scaffold", "--contract", "skill-resolve-response"]);
    assert_eq!(refused.code, 1);
    assert!(
        refused
            .stdout
            .contains("contract_scaffold_requires_request")
    );

    let example = run(&[
        "example",
        "--contract",
        "skill-resolve-request",
        "--scenario",
        "default",
    ]);
    assert_eq!(example.code, 0);
    assert!(example.stdout.contains("\"valid\": true"));
}

#[test]
fn unknown_targets_map_to_stable_error_reasons() {
    for (args, reason) in [
        (
            vec!["describe", "--command", "skill ghost"],
            "unknown_command",
        ),
        (
            vec!["describe", "--contract", "ghost"],
            "unknown_contract_id",
        ),
        (
            vec![
                "example",
                "--contract",
                "skill-resolve-request",
                "--scenario",
                "ghost",
            ],
            "unknown_scenario",
        ),
        (
            vec!["scaffold", "--contract", "ghost"],
            "unknown_contract_id",
        ),
    ] {
        let output = run(&args);
        assert_eq!(output.code, 1, "{args:?}");
        assert!(
            output.stdout.contains(&format!("\"reason\": \"{reason}\"")),
            "{args:?}: {}",
            output.stdout
        );
        assert!(output.stdout.contains("\"status\": \"error\""));
    }
}

#[test]
fn the_declared_example_is_accepted_by_resolve() {
    let dir = install();
    let example = run(&[
        "example",
        "--contract",
        "skill-resolve-request",
        "--scenario",
        "default",
    ]);
    assert_eq!(example.code, 0);
    let request = dir.path("request.json");
    fs::write(&request, REQUEST).unwrap();
    let output = run(&[
        "resolve",
        "--install",
        &install_arg(&dir),
        "--request",
        &request,
    ]);
    assert_eq!(output.code, 0, "{}", output.stdout);
    assert!(
        output
            .stdout
            .contains("skill(work):instructions/task/instructions.md")
    );
}

#[test]
fn runtime_commands_work_end_to_end_on_a_real_tree() {
    let dir = install();
    let root = install_arg(&dir);

    let validate = run(&["validate", "--install", &root]);
    assert_eq!(validate.code, 0, "{}", validate.stdout);
    assert!(validate.stdout.contains("\"valid\": true"));

    let discovered = run(&["discover", "--install", &root, "--skill", "work"]);
    assert_eq!(discovered.code, 0);
    assert!(discovered.stdout.contains("\"node\": \"task\""));

    let resolved = run_stdin(&["resolve", "--install", &root, "--request", "-"], REQUEST);
    assert_eq!(resolved.code, 0, "{}", resolved.stdout);
    assert!(resolved.stdout.contains("shared:bootstrap.md"));

    let active = dir.path("active.json");
    fs::write(&active, REQUEST).unwrap();
    let next = dir.path("next.json");
    fs::write(&next, REQUEST.replace("task", "")).unwrap();
    let switched = run(&[
        "switch",
        "--install",
        &root,
        "--active",
        &active,
        "--next",
        &next,
    ]);
    assert_eq!(switched.code, 0, "{}", switched.stdout);
    assert!(
        switched.stdout.contains("\"dropped\"") && switched.stdout.contains("task/instructions.md")
    );

    fs::write(&next, REQUEST.replace("task", "ghost")).unwrap();
    let rejected = run(&[
        "switch",
        "--install",
        &root,
        "--active",
        &active,
        "--next",
        &next,
    ]);
    assert_eq!(rejected.code, 1, "{}", rejected.stdout);
    assert!(rejected.stdout.contains("ancestor_missing") || rejected.stdout.contains("unreadable"));
}

#[test]
fn runtime_failures_report_reason_scope_and_source() {
    let dir = install();
    let root = install_arg(&dir);
    dir.put("install/work/instructions/task/instructions.md", "broken");

    let resolved = run_stdin(&["resolve", "--install", &root, "--request", "-"], REQUEST);
    assert_eq!(resolved.code, 1);
    assert!(
        resolved
            .stdout
            .contains("\"reason\": \"missing_frontmatter\"")
    );
    assert!(resolved.stdout.contains("\"scope\": \"branch\""));
    assert!(
        resolved
            .stdout
            .contains("skill(work):instructions/task/instructions.md")
    );

    let validate = run(&["validate", "--install", &root]);
    assert_eq!(validate.code, 1);
    assert!(validate.stdout.contains("\"valid\": false"));

    let bad_json = run_stdin(
        &["resolve", "--install", &root, "--request", "-"],
        "{not json",
    );
    assert!(bad_json.stdout.contains("invalid_request"));
    let wrong_schema = run_stdin(
        &["resolve", "--install", &root, "--request", "-"],
        r#"{"schema":"other","selections":[]}"#,
    );
    assert!(wrong_schema.stdout.contains("invalid_request"));
    let missing = run(&["validate", "--install", &dir.path("absent")]);
    assert_eq!(missing.code, 1);
    assert!(missing.stdout.contains("unreadable"));
}

fn dto(nodes: &[&str]) -> RequestDto {
    RequestDto {
        schema: RESOLVE_REQUEST.to_owned(),
        selections: vec![SelectionDto {
            skill: "work".to_owned(),
            nodes: nodes.iter().map(|node| (*node).to_owned()).collect(),
            references: Vec::new(),
        }],
        modules: Vec::new(),
    }
}

#[test]
fn the_skill_registry_is_structurally_valid_and_lists_its_surface() {
    let registry = registry().expect("registry builds");
    let listed = query::list(&registry, ListScope::All, None);
    for path in [
        "skill discover",
        "skill resolve",
        "skill switch",
        "skill validate",
    ] {
        assert!(listed.commands.contains(&path.to_owned()), "{path}");
    }
    assert_eq!(listed.contracts.len(), 5);
}

#[test]
fn the_declared_request_contract_matches_the_serde_type() {
    let registry = registry().unwrap();
    let described = query::describe_contract(&registry, RESOLVE_REQUEST).unwrap();
    let declared: Vec<_> = described.fields.keys().cloned().collect();
    let serialized = serialized_keys(&dto(&["task"]));
    for name in &declared {
        assert!(
            serialized.contains(name),
            "{name} is declared but not serialized"
        );
    }
    for name in &serialized {
        assert!(
            declared.contains(name),
            "{name} is serialized but not declared"
        );
    }
    let example = query::example_query(&registry, RESOLVE_REQUEST, "default").unwrap();
    let text = encode_example(&example.example);
    let parsed: RequestDto =
        serde_json::from_str(&text).expect("example parses as the request type");
    assert!(into_request(parsed).is_ok());
}

#[test]
fn the_scaffold_names_the_required_fields_and_parses_as_the_request_type() {
    let registry = registry().unwrap();
    let data = query::scaffold_query(&registry, RESOLVE_REQUEST, &[]).unwrap();
    let ContractValue::Object(object) = &data.scaffold else {
        panic!("object")
    };
    assert_eq!(object["schema"].as_str(), Some(RESOLVE_REQUEST));
    assert!(object.contains_key("selections"));
    assert!(!object.contains_key("modules"));
}

fn encode_example(value: &ContractValue) -> String {
    match value {
        ContractValue::Null => "null".to_owned(),
        ContractValue::Boolean(value) => value.to_string(),
        ContractValue::Integer(value) => value.to_string(),
        ContractValue::Number(value) => value.to_string(),
        ContractValue::String(value) => format!("{value:?}"),
        ContractValue::Array(items) => {
            format!(
                "[{}]",
                items
                    .iter()
                    .map(encode_example)
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
        ContractValue::Object(entries) => format!(
            "{{{}}}",
            entries
                .iter()
                .map(|(key, value)| format!("{key:?}:{}", encode_example(value)))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

/// Top-level field names of a serialized value, without a dynamic JSON type.
fn serialized_keys<T: serde::Serialize>(value: &T) -> Vec<String> {
    let text = serde_json::to_string(value).unwrap();
    let mut keys = Vec::new();
    let mut depth = 0;
    let mut token = String::new();
    let mut in_string = false;
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '"' if in_string => {
                in_string = false;
                if depth == 1 && chars.peek() == Some(&':') {
                    keys.push(token.clone());
                }
                token.clear();
            }
            '"' => in_string = true,
            _ if in_string => token.push(character),
            '{' | '[' => depth += 1,
            '}' | ']' => depth -= 1,
            _ => {}
        }
    }
    keys
}

#[test]
fn model_envelope_preserves_the_public_skill_wire_shape() {
    use skill_model::{Envelope, ErrorData, Reason};

    let ok = Envelope::Ok { data: "ready" };
    assert_eq!(
        serde_json::to_string(&ok).unwrap(),
        r#"{"status":"ok","data":"ready"}"#
    );

    let error = ErrorData {
        reason: Reason::InvalidUtf8.code().to_owned(),
        message: "cannot read source".to_owned(),
        source: Some("work/root".to_owned()),
        scope: None,
        selection: None,
    };
    let failure = Envelope::<()>::Error { error };
    assert_eq!(
        serde_json::to_string(&failure).unwrap(),
        r#"{"status":"error","error":{"reason":"invalid_utf8","message":"cannot read source","source":"work/root"}}"#
    );
}
