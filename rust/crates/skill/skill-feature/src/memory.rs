//! In-memory `SourceStore` used by Feature tests; no files are involved.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use skill_model::{Reason, SkillError, SourceId, SourceRef, SourceRoot};

use crate::port::{DirEntry, SourceContent, SourceStore};

const SHARED: &str = "@shared";

fn key(root: &SourceRoot) -> String {
    match root {
        SourceRoot::Shared => SHARED.to_owned(),
        SourceRoot::Skill(name) => name.clone(),
    }
}

#[derive(Default)]
pub struct Memory {
    skills: BTreeSet<String>,
    files: BTreeMap<(String, String), String>,
    dirs: BTreeSet<(String, String)>,
    reads: RefCell<BTreeMap<String, usize>>,
}

impl Memory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn skill(&mut self, name: &str) {
        self.skills.insert(name.to_owned());
    }

    pub fn put(&mut self, root: &str, path: &str, text: &str) {
        self.files
            .insert((root.to_owned(), path.to_owned()), text.to_owned());
    }

    pub fn shared(&mut self, path: &str, text: &str) {
        self.put(SHARED, path, text);
    }

    pub fn dir(&mut self, root: &str, path: &str) {
        self.dirs.insert((root.to_owned(), path.to_owned()));
    }

    pub fn reads(&self, path: &str) -> usize {
        self.reads
            .borrow()
            .iter()
            .filter(|(read, _)| read.ends_with(path))
            .map(|(_, count)| *count)
            .sum()
    }
}

impl SourceStore for Memory {
    fn skills(&self) -> Result<Vec<String>, SkillError> {
        Ok(self.skills.iter().cloned().collect())
    }

    fn read(&self, source: &SourceRef) -> Result<SourceContent, SkillError> {
        let at = (key(&source.root), source.path.clone());
        *self
            .reads
            .borrow_mut()
            .entry(format!("{}/{}", at.0, at.1))
            .or_default() += 1;
        let text = self
            .files
            .get(&at)
            .ok_or_else(|| SkillError::new(Reason::Unreadable, "source does not exist"))?;
        Ok(SourceContent {
            id: SourceId::new(format!("{}/{}", at.0, at.1)),
            text: text.clone(),
        })
    }

    fn entries(&self, root: &SourceRoot, dir: &str) -> Result<Vec<DirEntry>, SkillError> {
        let root = key(root);
        let prefix = if dir.is_empty() {
            String::new()
        } else {
            format!("{dir}/")
        };
        let mut found: BTreeMap<String, bool> = BTreeMap::new();
        for (owner, path) in self.files.keys().chain(self.dirs.iter()) {
            if *owner != root {
                continue;
            }
            let Some(rest) = path.strip_prefix(&prefix) else {
                continue;
            };
            match rest.split_once('/') {
                Some((head, _)) => {
                    found.insert(head.to_owned(), true);
                }
                None => {
                    let is_dir = self.dirs.contains(&(root.clone(), path.clone()));
                    found.entry(rest.to_owned()).or_insert(is_dir);
                }
            }
        }
        Ok(found
            .into_iter()
            .map(|(name, is_dir)| DirEntry { name, is_dir })
            .collect())
    }
}

/// A valid Root, Node or Reference document with an optional extra body.
pub fn instruction(name: &str, extra: &str) -> String {
    format!(
        "---\nname: {name}\ndescription: 適用時使用。\nmetadata:\n  tags:\n    - capability\n---\n\n- [Required] 遵守規範。\n\n{extra}"
    )
}

/// `## References` table for an instruction document.
pub fn references(rows: &[(&str, &str)]) -> String {
    table("References", "Reference", rows)
}

/// `## Modules` table for a Bootstrap document.
pub fn modules(rows: &[(&str, &str)]) -> String {
    table("Modules", "Module", rows)
}

fn table(heading: &str, column: &str, rows: &[(&str, &str)]) -> String {
    let mut text = format!("## {heading}\n\n| {column} | Load condition |\n| --- | --- |\n");
    for (name, condition) in rows {
        text.push_str(&format!("| {name} | {condition} |\n"));
    }
    text
}

/// A valid Bootstrap document.
pub fn bootstrap(extra: &str) -> String {
    format!("- [Required] 解析 CLI 入口。\n\n{extra}")
}

/// A valid Module document.
pub fn module() -> String {
    "- [Default] 處理來源失敗。\n".to_owned()
}
