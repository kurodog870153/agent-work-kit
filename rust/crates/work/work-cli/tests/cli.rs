//! CLI contract tests: Parser, Help, Schema and Serde stay consistent, and the
//! record commands behave end to end on temporary roots built by the test.

use std::fs;
use std::path::PathBuf;

use contract_core::clap_tree;
use work_cli::{Output, execute, registry};
use work_model::{
    CorrectionApplyRequest, CorrectionDraftRequest, DiscussionCreateRequest,
    DiscussionRoundRequest, ExecutionRecordRequest, RecordRef, RequirementAcceptanceRecordRequest,
    SourceCreateRequest, TaskAcceptanceRecordRequest, TaskApplyRequest, TaskCompleteRequest,
    TaskCreateRequest, TaskPreviewRequest, TaskReviseRequest,
};

struct Dir(PathBuf);

impl Dir {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "work-cli-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_nanos())
        ));
        fs::create_dir_all(&path).expect("temp dir");
        Self(path)
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

fn run_stdin(args: &[&str], input: &str) -> Output {
    let mut all = vec!["work".to_owned()];
    all.extend(args.iter().map(|item| (*item).to_owned()));
    let input = input.to_owned();
    execute(&all, &mut || input.clone())
}

fn run(args: &[&str]) -> Output {
    run_stdin(args, "")
}

/// Run a record command against `root`, feeding `request` on standard input.
fn send(root: &str, command: &[&str], request: &str) -> Output {
    let mut args = command.to_vec();
    args.extend(["--root", root, "--request", "-"]);
    run_stdin(&args, request)
}

fn ok(output: &Output) -> &str {
    assert_eq!(output.code, 0, "{}{}", output.stdout, output.stderr);
    &output.stdout
}

fn reason(output: &Output) -> String {
    assert_eq!(output.code, 1, "{}", output.stdout);
    let marker = "\"reason\": \"";
    let start = output.stdout.find(marker).expect("reason") + marker.len();
    output.stdout[start..]
        .split('"')
        .next()
        .expect("reason end")
        .to_owned()
}

const SOURCE: &str = r#"{"schema":"work-source-create-request","id":"src-1","title":"Issue","locator":"https://example.test/1","content":{"kind":"text","text":"  raw\r\nbody "},"origin":{"source":"external"}}"#;
const DISCUSSION: &str = r#"{"schema":"work-discussion-create-request","id":"disc-1","original_request":"Add export","source_ids":["src-1"]}"#;
const ASK: &str = r#"{"schema":"work-discussion-round-request","id":"disc-1","expected_version":1,"ask":{"id":"q1","text":"Format?","options":[{"id":"1","text":"CSV"},{"id":"2","text":"PDF"}],"recommendation":"1","reason":"simple"}}"#;
const ANSWER: &str = r#"{"schema":"work-discussion-round-request","id":"disc-1","expected_version":2,"answer":{"question_id":"q1","option_id":"2"},"decisions":[{"id":"dec1","text":"Use PDF","question_id":"q1"}]}"#;
const DRAFT: &str = r#"{"schema":"work-correction-draft-request","id":"cor-1","target":{"kind":"source","id":"src-1","version":1},"statement":"Locator is https://example.test/2","reason":"typo","origin":{"source":"user"}}"#;

#[test]
fn the_clap_tree_is_consistent_with_the_registry_declaration() {
    let registry = registry().unwrap();
    let root = &registry.command("work").unwrap().node;
    let tree = clap_tree::build(root);
    tree.clone().debug_assert();
    fn check(node: &contract_core::model::CommandNode, command: &clap::Command) {
        for argument in &node.arguments {
            assert!(
                command
                    .get_arguments()
                    .any(|candidate| candidate.get_id().as_str() == argument.name),
                "{} --{}",
                node.name,
                argument.name
            );
        }
        for child in &node.children {
            check(
                child,
                command.find_subcommand(&child.name).expect(&child.name),
            );
        }
    }
    check(root, &tree);
}

#[test]
fn help_is_derived_from_the_declared_commands_and_has_no_migration() {
    let output = run(&["--help"]);
    assert_eq!(output.code, 0);
    for name in [
        "source",
        "discussion",
        "correction",
        "task",
        "execution",
        "acceptance",
        "trace",
        "status",
        "recover",
        "list",
        "describe",
        "scaffold",
        "example",
    ] {
        assert!(output.stdout.contains(name), "help lacks {name}");
    }
    assert!(!output.stdout.to_lowercase().contains("migrat"));
    let sub = run(&["discussion", "round", "--help"]);
    assert_eq!(sub.code, 0);
    assert!(sub.stdout.contains("--root") && sub.stdout.contains("--request"));
    assert!(run(&["--version"]).stdout.contains("3.0.0"));
    assert!(run(&["migration"]).code == 2);
}

#[test]
fn parser_rejects_bad_usage_with_exit_code_two() {
    for args in [
        vec![],
        vec!["unknown"],
        vec!["source"],
        vec!["source", "create"],
        vec!["status"],
        vec![
            "correction",
            "list",
            "--root",
            "x",
            "--kind",
            "correction",
            "--id",
            "a",
        ],
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
fn list_describe_scaffold_and_example_serve_the_work_contract() {
    let listed = run(&["list"]);
    assert!(
        ok(&listed).contains("work source create")
            && listed.stdout.contains("work-source-create-request")
    );
    assert!(
        ok(&run(&["describe", "--command", "work discussion round"]))
            .contains("work-discussion-round-request")
    );
    assert!(
        ok(&run(&[
            "describe",
            "--contract",
            "work-source-create-request"
        ]))
        .contains("locator")
    );
    for id in [
        "work-source-create-request",
        "work-discussion-create-request",
        "work-discussion-round-request",
        "work-correction-draft-request",
        "work-correction-apply-request",
        "work-task-create-request",
        "work-task-revise-request",
        "work-task-preview-request",
        "work-task-apply-request",
        "work-task-complete-request",
        "work-execution-record-request",
        "work-task-acceptance-record-request",
        "work-requirement-acceptance-record-request",
    ] {
        ok(&run(&["scaffold", "--contract", id]));
        ok(&run(&[
            "example",
            "--contract",
            id,
            "--scenario",
            "default",
        ]));
    }
    let refused = run(&["scaffold", "--contract", "work-write-response"]);
    assert_eq!(reason(&refused), "contract_scaffold_requires_request");
    assert_eq!(
        reason(&run(&["describe", "--command", "work migrate"])),
        "unknown_command"
    );
}

#[test]
fn declared_examples_deserialise_into_the_request_types() {
    fn example(id: &str) -> String {
        let registry = registry().unwrap();
        serde_json::to_string(&registry.example(id, "default").expect(id).example).unwrap()
    }
    serde_json::from_str::<SourceCreateRequest>(&example("work-source-create-request")).unwrap();
    serde_json::from_str::<DiscussionCreateRequest>(&example("work-discussion-create-request"))
        .unwrap();
    serde_json::from_str::<DiscussionRoundRequest>(&example("work-discussion-round-request"))
        .unwrap();
    serde_json::from_str::<CorrectionDraftRequest>(&example("work-correction-draft-request"))
        .unwrap();
    serde_json::from_str::<CorrectionApplyRequest>(&example("work-correction-apply-request"))
        .unwrap();
    serde_json::from_str::<TaskCreateRequest>(&example("work-task-create-request")).unwrap();
    serde_json::from_str::<TaskReviseRequest>(&example("work-task-revise-request")).unwrap();
    serde_json::from_str::<TaskPreviewRequest>(&example("work-task-preview-request")).unwrap();
    serde_json::from_str::<TaskApplyRequest>(&example("work-task-apply-request")).unwrap();
    serde_json::from_str::<TaskCompleteRequest>(&example("work-task-complete-request")).unwrap();
    serde_json::from_str::<ExecutionRecordRequest>(&example("work-execution-record-request"))
        .unwrap();
    serde_json::from_str::<TaskAcceptanceRecordRequest>(&example(
        "work-task-acceptance-record-request",
    ))
    .unwrap();
    serde_json::from_str::<RequirementAcceptanceRecordRequest>(&example(
        "work-requirement-acceptance-record-request",
    ))
    .unwrap();
}

#[test]
fn source_and_discussion_flow_end_to_end_with_resume_and_history() {
    let dir = Dir::new("flow");
    let root = dir.path("data");

    ok(&send(&root, &["source", "create"], SOURCE));
    assert_eq!(
        reason(&send(&root, &["source", "create"], SOURCE)),
        "already_exists"
    );
    ok(&send(&root, &["discussion", "create"], DISCUSSION));
    ok(&send(&root, &["discussion", "round"], ASK));

    // A later session sees the open question and its numbered options.
    let progress = run(&["discussion", "progress", "--root", &root, "--id", "disc-1"]);
    let text = ok(&progress);
    assert!(text.contains("\"outstanding_question\"") && text.contains("\"text\": \"PDF\""));

    assert_eq!(
        reason(&send(&root, &["discussion", "round"], ASK)),
        "version_conflict"
    );
    let wrong = ANSWER.replace("\"option_id\":\"2\"", "\"option_id\":\"9\"");
    assert_eq!(
        reason(&send(&root, &["discussion", "round"], &wrong)),
        "rule_violation"
    );
    ok(&send(&root, &["discussion", "round"], ANSWER));
    assert_eq!(
        reason(&send(
            &root,
            &["discussion", "round"],
            &ANSWER.replace("\"expected_version\":2", "\"expected_version\":3")
        )),
        "rule_violation"
    );

    let history = run(&["discussion", "history", "--root", &root, "--id", "disc-1"]);
    let text = ok(&history);
    assert_eq!(text.matches("\"version\"").count(), 3, "{text}");

    let old = run(&[
        "discussion",
        "show",
        "--root",
        &root,
        "--id",
        "disc-1",
        "--version",
        "2",
    ]);
    assert!(ok(&old).contains("\"latest_version\": 3") && !old.stdout.contains("Use PDF"));
    let latest = run(&["discussion", "show", "--root", &root, "--id", "disc-1"]);
    assert!(ok(&latest).contains("Use PDF"));

    let source = run(&["source", "show", "--root", &root, "--id", "src-1"]);
    let text = ok(&source);
    assert!(text.contains("\"referenced_by\"") && text.contains("disc-1"));
    assert!(text.contains(r"  raw\r\nbody "));

    let status = run(&["status", "--root", &root]);
    assert!(ok(&status).contains("\"latest_version\": 3"));
    assert_eq!(
        reason(&run(&["source", "show", "--root", &root, "--id", "ghost"])),
        "not_found"
    );
    assert_eq!(
        reason(&run(&[
            "discussion",
            "show",
            "--root",
            &root,
            "--id",
            "bad id"
        ])),
        "invalid_request"
    );
    assert_eq!(
        reason(&run(&[
            "source",
            "show",
            "--root",
            &root,
            "--id",
            "src-1",
            "--version",
            "0"
        ])),
        "invalid_request"
    );
}

#[test]
fn bad_requests_are_rejected_without_writing() {
    let dir = Dir::new("bad");
    let root = dir.path("data");
    assert_eq!(
        reason(&send(&root, &["source", "create"], "{not json")),
        "invalid_request"
    );
    assert_eq!(
        reason(&send(
            &root,
            &["source", "create"],
            &SOURCE.replace("work-source-create-request", "other")
        )),
        "invalid_request"
    );
    assert_eq!(
        reason(&send(
            &root,
            &["source", "create"],
            &SOURCE.replace("\"title\":\"Issue\",", "")
        )),
        "invalid_request"
    );
    assert_eq!(
        reason(&send(
            &root,
            &["source", "create"],
            &SOURCE.replace("\"origin\"", "\"extra\":1,\"origin\"")
        )),
        "invalid_request"
    );
    assert_eq!(
        reason(&send(&root, &["discussion", "create"], DISCUSSION)),
        "not_found"
    );
    let missing = run(&[
        "source",
        "create",
        "--root",
        &root,
        "--request",
        &dir.path("absent.json"),
    ]);
    assert_eq!(reason(&missing), "invalid_request");
    assert!(ok(&run(&["status", "--root", &root])).contains("\"records\": []"));
}

#[test]
fn published_history_is_changed_only_by_an_approved_correction() {
    let dir = Dir::new("correction");
    let root = dir.path("data");
    ok(&send(&root, &["source", "create"], SOURCE));

    let preview = send(&root, &["correction", "preview"], DRAFT);
    let text = ok(&preview).to_owned();
    assert!(text.contains("\"reviewed_target_version\": 1"));
    assert!(
        ok(&run(&[
            "correction",
            "list",
            "--root",
            &root,
            "--kind",
            "source",
            "--id",
            "src-1"
        ]))
        .contains("\"corrections\": []")
    );

    // The candidate is the string value of the preview; extract it as typed data.
    let candidate = {
        let marker = "\"candidate\": ";
        let start = text.find(marker).unwrap() + marker.len();
        let end = text[start..].find("\",\n").unwrap() + 1;
        serde_json::from_str::<String>(&text[start..start + end]).unwrap()
    };
    let apply = |approved: &str, version: u32, draft: &str| {
        let request = format!(
            "{{\"schema\":\"work-correction-apply-request\",\"draft\":{draft},\"approved_candidate\":{},\"reviewed_target_version\":{version}}}",
            serde_json::to_string(approved).unwrap()
        );
        send(&root, &["correction", "apply"], &request)
    };
    assert_eq!(reason(&apply("", 1, DRAFT)), "approval_mismatch");
    assert_eq!(
        reason(&apply(
            &candidate,
            1,
            &DRAFT.replace("typo", "other reason")
        )),
        "approval_mismatch"
    );
    assert_eq!(reason(&apply(&candidate, 4, DRAFT)), "version_conflict");
    ok(&apply(&candidate, 1, DRAFT));
    assert_eq!(reason(&apply(&candidate, 1, DRAFT)), "already_exists");
    assert_eq!(
        reason(&send(&root, &["correction", "preview"], DRAFT)),
        "already_exists"
    );

    let shown = run(&["source", "show", "--root", &root, "--id", "src-1"]);
    let text = ok(&shown);
    assert!(text.contains("  raw") && text.contains("Locator is https://example.test/2"));
    let listed = run(&[
        "correction",
        "list",
        "--root",
        &root,
        "--kind",
        "source",
        "--id",
        "src-1",
    ]);
    assert!(ok(&listed).contains("cor-1"));

    let missing_target = DRAFT
        .replace("cor-1", "cor-2")
        .replace("\"version\":1", "\"version\":3");
    assert_eq!(
        reason(&send(&root, &["correction", "preview"], &missing_target)),
        "not_found"
    );
    let on_correction = DRAFT
        .replace("cor-1", "cor-2")
        .replace("\"kind\":\"source\"", "\"kind\":\"correction\"");
    assert_eq!(
        reason(&send(&root, &["correction", "preview"], &on_correction)),
        "rule_violation"
    );
}

#[test]
fn an_interrupted_write_blocks_until_recover_and_recover_restores_service() {
    let dir = Dir::new("recover");
    let root = dir.path("data");
    ok(&send(&root, &["source", "create"], SOURCE));

    // Simulate a crash after the journal was published and one target placed.
    let txn = dir.0.join("data").join("txn");
    fs::create_dir_all(&txn).unwrap();
    let partial = "records/source/half/v0001.json";
    fs::write(txn.join("journal"), format!("{partial}\n")).unwrap();
    let target = dir.0.join("data").join(partial);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, "{\"half\":").unwrap();

    assert_eq!(
        reason(&run(&["status", "--root", &root])),
        "recovery_required"
    );
    assert_eq!(
        reason(&send(
            &root,
            &["source", "create"],
            &SOURCE.replace("src-1", "src-2")
        )),
        "recovery_required"
    );

    let recovered = run(&["recover", "--root", &root]);
    assert!(ok(&recovered).contains(partial));
    assert!(!target.exists());
    let status = run(&["status", "--root", &root]);
    assert!(ok(&status).contains("src-1") && !status.stdout.contains("half"));
    ok(&send(
        &root,
        &["source", "create"],
        &SOURCE.replace("src-1", "src-2"),
    ));
    assert!(ok(&run(&["recover", "--root", &root])).contains("\"rolled_back\": []"));
}

const TASK: &str = r#"{"schema":"work-task-create-request","id":"task-1","title":"Export","goal":"Users can export","requirements":["Export as PDF"],"criteria":[{"id":"c1","text":"PDF opens"},{"id":"c2","text":"Docs updated","required":false}],"discussion_ids":["disc-1"],"source_ids":["src-1"]}"#;
const EXEC: &str = r#"{"schema":"work-execution-record-request","id":"exec-1","task_id":"task-1","attempt":{"id":"at-1","result":"failed","changes":["added button"],"anomalies":["timeout"]}}"#;
const EXEC_RETRY: &str = r#"{"schema":"work-execution-record-request","id":"exec-1","task_id":"task-1","expected_version":1,"attempt":{"id":"at-2","result":"succeeded","changes":["fixed timeout"],"verification":["export test"]}}"#;
const REVISE: &str = r#"{"schema":"work-task-revise-request","id":"task-1","expected_version":2,"reason":"also CSV","requirements":["Export as PDF and CSV"],"criteria":[{"id":"c1","text":"PDF and CSV open"},{"id":"c2","text":"Docs updated","required":false}],"reconfirm_requirement":true}"#;

#[derive(serde::Deserialize)]
struct PreviewEnvelope {
    data: ReviewedTask,
}

#[derive(serde::Deserialize)]
struct ReviewedTask {
    candidate: String,
    reviewed_dependencies: Vec<RecordRef>,
}

fn task_preview(root: &str, operation: &str, draft: &str) -> Output {
    let input = format!(
        r#"{{"schema":"work-task-preview-request","draft":{{"operation":"{operation}","request":{draft}}}}}"#
    );
    send(root, &["task", "preview"], &input)
}

fn task_apply(root: &str, operation: &str, draft: &str, reviewed: &ReviewedTask) -> Output {
    let candidate = serde_json::to_string(&reviewed.candidate).unwrap();
    let versions = serde_json::to_string(&reviewed.reviewed_dependencies).unwrap();
    let input = format!(
        r#"{{"schema":"work-task-apply-request","draft":{{"operation":"{operation}","request":{draft}}},"approved_candidate":{candidate},"reviewed_dependencies":{versions}}}"#
    );
    send(root, &["task", "apply"], &input)
}

fn task_save(root: &str, operation: &str, draft: &str) -> Output {
    let preview = task_preview(root, operation, draft);
    if preview.code != 0 {
        return preview;
    }
    let reviewed: PreviewEnvelope = serde_json::from_str(ok(&preview)).unwrap();
    task_apply(root, operation, draft, &reviewed.data)
}

fn complete(version: u32) -> String {
    format!(
        r#"{{"schema":"work-task-complete-request","id":"task-1","expected_version":{version}}}"#
    )
}

fn task_acceptance(version: u32, expected: Option<u32>, criterion: &str, verdict: &str) -> String {
    let expected = expected.map_or(String::new(), |v| format!(r#","expected_version":{v}"#));
    format!(
        r#"{{"schema":"work-task-acceptance-record-request","id":"acc-1","task_id":"task-1","task_version":{version}{expected},"conclusions":[{{"criterion_id":"{criterion}","verdict":"{verdict}","evidence":"checked","origin":{{"source":"host"}}}}]}}"#
    )
}

fn requirement(expected: Option<u32>, task_version: u32) -> String {
    let expected = expected.map_or(String::new(), |v| format!(r#","expected_version":{v}"#));
    format!(
        r#"{{"schema":"work-requirement-acceptance-record-request","id":"req-1","discussion_id":"disc-1"{expected},"verdict":"passed","evidence":"user confirmed","origin":{{"source":"user"}},"task_refs":[{{"kind":"task","id":"task-1","version":{task_version}}}]}}"#
    )
}

fn seed(root: &str) {
    ok(&send(root, &["source", "create"], SOURCE));
    ok(&send(root, &["discussion", "create"], DISCUSSION));
}

fn show(root: &str, args: &[&str]) -> Output {
    let mut all = args.to_vec();
    all.extend(["--root", root]);
    run(&all)
}

#[test]
fn task_execution_and_acceptance_trace_end_to_end_with_revision() {
    let dir = Dir::new("task");
    let root = dir.path("data");
    seed(&root);
    ok(&task_save(&root, "create", TASK));
    assert_eq!(reason(&task_save(&root, "create", TASK)), "already_exists");

    // Execution keeps both attempts; it never completes or accepts anything.
    ok(&send(&root, &["execution", "record"], EXEC));
    ok(&send(&root, &["execution", "record"], EXEC_RETRY));
    let execution = show(&root, &["execution", "show", "--id", "exec-1"]);
    let text = ok(&execution);
    assert!(text.contains("\"at-1\"") && text.contains("\"at-2\"") && text.contains("timeout"));
    assert_eq!(
        reason(&send(&root, &["task", "complete"], &complete(1))),
        "rule_violation"
    );

    // Required criteria gate completion; the optional one does not.
    ok(&send(
        &root,
        &["acceptance", "task-record"],
        &task_acceptance(1, None, "c1", "failed"),
    ));
    assert_eq!(
        reason(&send(&root, &["task", "complete"], &complete(1))),
        "rule_violation"
    );
    ok(&send(
        &root,
        &["acceptance", "task-record"],
        &task_acceptance(1, Some(1), "c1", "passed"),
    ));
    assert!(
        ok(&show(&root, &["task", "show", "--id", "task-1"])).contains("\"completable\": true")
    );
    ok(&send(&root, &["task", "complete"], &complete(1)));
    assert!(
        ok(&show(&root, &["task", "show", "--id", "task-1"])).contains("\"status\": \"completed\"")
    );

    // Requirement Acceptance is separate: completion did not create it.
    assert_eq!(
        reason(&show(
            &root,
            &["acceptance", "requirement-show", "--id", "req-1"]
        )),
        "not_found"
    );
    ok(&send(
        &root,
        &["acceptance", "requirement-record"],
        &requirement(None, 2),
    ));
    let req = show(&root, &["acceptance", "requirement-show", "--id", "req-1"]);
    assert!(
        ok(&req).contains("\"verdict\": \"passed\"")
            && req.stdout.contains("\"needs_reconfirm\": false")
    );

    // Revise: the changed criterion and the requirement must be confirmed again.
    ok(&task_save(&root, "revise", REVISE));
    let shown = show(&root, &["task", "show", "--id", "task-1"]);
    let text = ok(&shown);
    assert!(
        text.contains("\"status\": \"open\"") && text.contains("needs_reconfirm"),
        "{text}"
    );
    assert_eq!(
        reason(&send(&root, &["task", "complete"], &complete(3))),
        "rule_violation"
    );
    assert!(
        ok(&show(
            &root,
            &["acceptance", "requirement-show", "--id", "req-1"]
        ))
        .contains("\"needs_reconfirm\": true")
    );

    // History is kept: version 2 still shows the completed TASK and old text.
    let old = show(&root, &["task", "show", "--id", "task-1", "--version", "2"]);
    let text = ok(&old);
    assert!(text.contains("\"status\": \"completed\"") && text.contains("\"text\": \"PDF opens\""));

    ok(&send(
        &root,
        &["acceptance", "task-record"],
        &task_acceptance(3, Some(2), "c1", "passed"),
    ));
    ok(&send(&root, &["task", "complete"], &complete(3)));
    ok(&send(
        &root,
        &["acceptance", "requirement-record"],
        &requirement(Some(1), 4),
    ));
    assert!(
        ok(&show(
            &root,
            &["acceptance", "requirement-show", "--id", "req-1"]
        ))
        .contains("\"needs_reconfirm\": false")
    );
    let acceptance = show(&root, &["acceptance", "task-show", "--id", "acc-1"]);
    assert!(ok(&acceptance).matches("\"criterion_id\": \"c1\"").count() >= 3);

    // Source → Discussion → TASK → Execution → Acceptance, from any anchor.
    for (kind, id) in [
        ("source", "src-1"),
        ("discussion", "disc-1"),
        ("task", "task-1"),
        ("execution", "exec-1"),
        ("task_acceptance", "acc-1"),
        ("requirement_acceptance", "req-1"),
    ] {
        let traced = show(&root, &["trace", "--kind", kind, "--id", id]);
        let text = ok(&traced);
        for expected in ["src-1", "disc-1", "task-1", "exec-1", "acc-1", "req-1"] {
            assert!(text.contains(expected), "{kind}/{id} lacks {expected}");
        }
    }
    // Corrections are not trace anchors; the parser refuses the kind.
    let refused = show(&root, &["trace", "--kind", "correction", "--id", "x"]);
    assert_eq!(refused.code, 2);
}

#[test]
fn task_commands_reject_bad_requests_without_writing() {
    let dir = Dir::new("task-bad");
    let root = dir.path("data");
    assert_eq!(reason(&task_save(&root, "create", TASK)), "not_found");
    seed(&root);
    let no_criteria = TASK.replace(
        r#""criteria":[{"id":"c1","text":"PDF opens"},{"id":"c2","text":"Docs updated","required":false}],"#,
        r#""criteria":[],"#,
    );
    for bad in [
        no_criteria,
        TASK.replace("task-1", "bad id"),
        TASK.replace("\"goal\"", "\"extra\":1,\"goal\""),
    ] {
        assert!(
            ["invalid_request", "rule_violation"]
                .contains(&reason(&task_save(&root, "create", &bad)).as_str())
        );
    }
    assert_eq!(
        reason(&send(
            &root,
            &["acceptance", "task-record"],
            &task_acceptance(1, None, "c1", "passed")
        )),
        "not_found"
    );
    ok(&task_save(&root, "create", TASK));
    assert_eq!(
        reason(&send(
            &root,
            &["acceptance", "task-record"],
            &task_acceptance(1, None, "zz", "passed")
        )),
        "rule_violation"
    );
    assert_eq!(
        reason(&send(
            &root,
            &["acceptance", "task-record"],
            &task_acceptance(5, None, "c1", "passed")
        )),
        "version_conflict"
    );
    assert_eq!(
        reason(&send(
            &root,
            &["acceptance", "task-record"],
            &task_acceptance(1, None, "c1", "maybe")
        )),
        "invalid_request"
    );
    assert_eq!(
        reason(&send(
            &root,
            &["execution", "record"],
            &EXEC.replace("task-1", "ghost")
        )),
        "not_found"
    );
    assert_eq!(
        reason(&show(
            &root,
            &["task", "show", "--id", "task-1", "--version", "9"]
        )),
        "not_found"
    );
    let status = show(&root, &["status"]);
    let text = ok(&status);
    assert!(text.contains("task-1") && !text.contains("exec-1") && !text.contains("acc-1"));
}

#[test]
fn a_correction_can_target_a_task_but_never_rewrites_it() {
    let dir = Dir::new("task-correction");
    let root = dir.path("data");
    seed(&root);
    ok(&task_save(&root, "create", TASK));
    let draft = DRAFT.replace(
        r#""kind":"source","id":"src-1""#,
        r#""kind":"task","id":"task-1""#,
    );
    let preview = send(&root, &["correction", "preview"], &draft);
    assert!(ok(&preview).contains("\"reviewed_target_version\": 1"));
    let listed = show(
        &root,
        &["correction", "list", "--kind", "task", "--id", "task-1"],
    );
    assert!(ok(&listed).contains("\"corrections\": []"));
    assert!(ok(&show(&root, &["task", "show", "--id", "task-1"])).contains("PDF opens"));
}

#[test]
fn the_cli_exposes_no_handoff_workflow_selection_or_routing_surface() {
    let listed = run(&["list"]);
    let text = ok(&listed).to_lowercase();
    for banned in [
        "handoff",
        "delegat",
        "invocation",
        "workflow",
        "selection",
        "routing",
        "migrat",
    ] {
        assert!(!text.contains(banned), "contract surface mentions {banned}");
    }
}

#[test]
fn binary_source_from_file_matches_direct_request() {
    use work_model::{SourceContent, SourceRecord};
    #[derive(serde::Deserialize)]
    struct Response {
        data: Body,
    }
    #[derive(serde::Deserialize)]
    struct Body {
        record: SourceRecord,
    }

    let dir = Dir::new("binary-file");
    let root = dir.path("data");
    let file = dir.0.join("attachment.bin");
    let bytes = vec![0, 255, 128, 13, 10, 0, 240];
    fs::write(&file, &bytes).unwrap();
    let metadata = r#"{"schema":"work-source-create-request","id":"from-file","title":"Attachment","locator":"uploaded","origin":{"source":"external"}}"#;
    let output = run_stdin(
        &[
            "source",
            "create",
            "--root",
            &root,
            "--request",
            "-",
            "--from-file",
            &file.to_string_lossy(),
        ],
        metadata,
    );
    ok(&output);

    let direct = r#"{"schema":"work-source-create-request","id":"direct","title":"Attachment","locator":"uploaded","content":{"kind":"binary","bytes":[0,255,128,13,10,0,240]},"origin":{"source":"external"}}"#;
    ok(&send(&root, &["source", "create"], direct));
    let show = |id: &str| -> SourceRecord {
        let output = run(&["source", "show", "--root", &root, "--id", id]);
        serde_json::from_str::<Response>(ok(&output))
            .unwrap()
            .data
            .record
    };
    let from_file = show("from-file");
    let direct = show("direct");
    assert_eq!(
        from_file.content,
        SourceContent::Binary {
            bytes: bytes.clone()
        }
    );
    assert_eq!(from_file.content, direct.content);
    assert_eq!(from_file.content_length, bytes.len() as u64);
    assert_eq!(from_file.content_length, direct.content_length);
    assert_eq!(from_file.content_digest, direct.content_digest);

    assert_eq!(
        reason(&run_stdin(
            &[
                "source",
                "create",
                "--root",
                &root,
                "--request",
                "-",
                "--from-file",
                &dir.path("missing.bin")
            ],
            &metadata.replace("from-file", "missing"),
        )),
        "invalid_request"
    );
    let conflict = r#"{"schema":"work-source-create-request","id":"conflict","title":"Attachment","locator":"uploaded","content":{"kind":"text","text":"already supplied"},"origin":{"source":"external"}}"#;
    assert_eq!(
        reason(&run_stdin(
            &[
                "source",
                "create",
                "--root",
                &root,
                "--request",
                "-",
                "--from-file",
                &file.to_string_lossy()
            ],
            conflict,
        )),
        "invalid_request"
    );
    assert!(run(&["source", "show", "--root", &root, "--id", "missing"]).code != 0);
    assert!(run(&["source", "show", "--root", &root, "--id", "conflict"]).code != 0);
}

#[test]
fn task_requires_reviewed_candidate_and_dependency_versions() {
    let dir = Dir::new("task-review");
    let root = dir.path("data");
    seed(&root);

    assert_eq!(run(&["task", "create"]).code, 2);
    assert_eq!(run(&["task", "revise"]).code, 2);
    let preview = task_preview(&root, "create", TASK);
    let reviewed: PreviewEnvelope = serde_json::from_str(ok(&preview)).unwrap();
    assert_eq!(
        reason(&show(&root, &["task", "show", "--id", "task-1"])),
        "not_found"
    );
    assert!(reviewed.data.candidate.contains("PDF opens"));
    assert!(
        reviewed
            .data
            .reviewed_dependencies
            .iter()
            .any(|d| d.kind == work_model::RecordKind::Discussion
                && d.id.as_str() == "disc-1"
                && d.version == 1)
    );

    let mut altered = ReviewedTask {
        candidate: reviewed.data.candidate.replace("PDF opens", "tampered"),
        reviewed_dependencies: reviewed.data.reviewed_dependencies.clone(),
    };
    assert_eq!(
        reason(&task_apply(&root, "create", TASK, &altered)),
        "approval_mismatch"
    );
    altered.candidate = reviewed.data.candidate.clone();
    altered.reviewed_dependencies.clear();
    assert_eq!(
        reason(&task_apply(&root, "create", TASK, &altered)),
        "version_conflict"
    );
    assert_eq!(
        reason(&show(&root, &["task", "show", "--id", "task-1"])),
        "not_found"
    );

    // A Discussion published after preview invalidates the approval.
    let round = r#"{"schema":"work-discussion-round-request","id":"disc-1","expected_version":1,"add_pending":[{"id":"p1","text":"recheck"}]}"#;
    ok(&send(&root, &["discussion", "round"], round));
    assert_eq!(
        reason(&task_apply(&root, "create", TASK, &reviewed.data)),
        "version_conflict"
    );

    let current: PreviewEnvelope =
        serde_json::from_str(ok(&task_preview(&root, "create", TASK))).unwrap();
    assert_eq!(current.data.candidate, reviewed.data.candidate);
    ok(&task_apply(&root, "create", TASK, &current.data));

    let revision: PreviewEnvelope = serde_json::from_str(ok(&task_preview(
        &root,
        "revise",
        &REVISE.replace(r#""expected_version":2"#, r#""expected_version":1"#),
    )))
    .unwrap();
    // The revised draft does not change the approved source text.
    let modified = REVISE.replace(r#""expected_version":2"#, r#""expected_version":1"#);
    let mut substituted = modified.replace("also CSV", "unapproved");
    assert_eq!(
        reason(&task_apply(&root, "revise", &substituted, &revision.data)),
        "approval_mismatch"
    );
    substituted = modified;
    ok(&task_apply(&root, "revise", &substituted, &revision.data));
    assert_eq!(
        reason(&task_apply(&root, "revise", &substituted, &revision.data)),
        "version_conflict"
    );
}
