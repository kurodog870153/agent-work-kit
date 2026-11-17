//! Flow behaviour and contract consistency tests (in-memory store, no files).

use std::collections::BTreeMap;

use skill_feature::{DirEntry, SourceContent, SourceStore};
use skill_flow::{discover, into_request, resolve, switch, validate};
use skill_model::{
    Envelope, ErrorData, RESOLVE_REQUEST, Reason, RequestDto, SelectionDto, SkillError, SourceId,
    SourceRef, SourceRoot,
};

#[derive(Default)]
struct Store(BTreeMap<(String, String), String>);

impl Store {
    fn put(&mut self, root: &str, path: &str, text: &str) {
        self.0
            .insert((root.to_owned(), path.to_owned()), text.to_owned());
    }
}

fn key(root: &SourceRoot) -> String {
    match root {
        SourceRoot::Shared => "@shared".to_owned(),
        SourceRoot::Skill(name) => name.clone(),
    }
}

impl SourceStore for Store {
    fn skills(&self) -> Result<Vec<String>, SkillError> {
        Ok(vec!["work".to_owned()])
    }

    fn read(&self, source: &SourceRef) -> Result<SourceContent, SkillError> {
        let at = (key(&source.root), source.path.clone());
        let text = self
            .0
            .get(&at)
            .ok_or_else(|| SkillError::new(Reason::Unreadable, "missing"))?;
        Ok(SourceContent {
            id: SourceId::new(format!("{}/{}", at.0, at.1)),
            text: text.clone(),
        })
    }

    fn entries(&self, root: &SourceRoot, dir: &str) -> Result<Vec<DirEntry>, SkillError> {
        let root = key(root);
        let prefix = format!("{dir}/");
        let mut found: BTreeMap<String, bool> = BTreeMap::new();
        for (owner, path) in self.0.keys() {
            if *owner != root {
                continue;
            }
            if let Some(rest) = path.strip_prefix(&prefix) {
                match rest.split_once('/') {
                    Some((head, _)) => {
                        found.insert(head.to_owned(), true);
                    }
                    None => {
                        found.entry(rest.to_owned()).or_insert(false);
                    }
                }
            }
        }
        Ok(found
            .into_iter()
            .map(|(name, is_dir)| DirEntry { name, is_dir })
            .collect())
    }
}

fn node(name: &str) -> String {
    format!(
        "---\nname: {name}\ndescription: 適用時使用。\nmetadata:\n  tags:\n    - capability\n---\n\n- [Required] 遵守。\n"
    )
}

fn store() -> Store {
    let mut store = Store::default();
    store.put("@shared", "bootstrap.md", "- [Required] 解析入口。\n");
    store.put("work", "instructions/instructions.md", &node("根"));
    store.put("work", "instructions/task/instructions.md", &node("任務"));
    store.put(
        "work",
        "instructions/execute/instructions.md",
        &node("執行"),
    );
    store
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
fn unknown_request_fields_and_wrong_schemas_are_rejected() {
    assert!(
        serde_json::from_str::<RequestDto>(r#"{"schema":"x","selections":[],"extra":1}"#).is_err()
    );
    let mut wrong = dto(&[""]);
    wrong.schema = "other".to_owned();
    assert_eq!(
        into_request(wrong).unwrap_err().reason,
        Reason::InvalidRequest
    );
}

#[test]
fn use_cases_report_children_sources_switches_and_validation() {
    let store = store();
    let found = discover(&store, "work", "").unwrap();
    let nodes: Vec<_> = found
        .children
        .iter()
        .map(|child| child.node.as_str())
        .collect();
    assert_eq!(nodes, ["execute", "task"]);

    let resolved = resolve(&store, dto(&["task"])).unwrap();
    assert_eq!(resolved.sources.len(), 3);
    assert_eq!(resolved.sources[0].kind, "bootstrap");

    let switched = switch(&store, dto(&["task"]), dto(&["execute"])).unwrap();
    assert_eq!(
        switched.added,
        ["skill(work):instructions/execute/instructions.md"]
    );
    assert_eq!(
        switched.dropped,
        ["skill(work):instructions/task/instructions.md"]
    );
    assert_eq!(switched.reused.len(), 2);

    let report = validate(&store).unwrap();
    assert!(report.valid, "{:?}", report.failures);
}

#[test]
fn an_invalid_next_set_rejects_the_switch() {
    let mut broken = store();
    broken.put("work", "instructions/execute/instructions.md", "broken");
    let error = switch(&broken, dto(&["task"]), dto(&["execute"])).unwrap_err();
    assert_eq!(error.reason, Reason::MissingFrontmatter);
    let envelope: Envelope<()> = Envelope::Error {
        error: ErrorData {
            reason: error.reason.code().to_owned(),
            message: error.message.clone(),
            source: error.source.clone(),
            scope: error.scope.map(|scope| scope.code().to_owned()),
            selection: error.selection.clone(),
        },
    };
    let text = serde_json::to_string(&envelope).unwrap();
    assert!(text.contains(r#""status":"error""#) && text.contains("missing_frontmatter"));
}
