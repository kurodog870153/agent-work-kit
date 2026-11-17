//! File-system behaviour tests. Every tree, symlink and junction is created by
//! the test itself inside a unique temporary directory; no fixture files exist.

use std::fs;
use std::path::{Path, PathBuf};

use skill_feature::{Request, Selection, SourceStore, discover, resolve, validate_all};
use skill_infrastructure::FsStore;
use skill_model::{Reason, SourceRef, SourceRoot};

struct Tree {
    root: PathBuf,
}

impl Tree {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "skill-infra-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_nanos())
        ));
        fs::create_dir_all(&root).expect("temp root");
        Self { root }
    }

    fn install(&self) -> PathBuf {
        self.root.join("install")
    }

    fn write(&self, relative: &str, bytes: &[u8]) {
        let path = self.install().join(relative);
        fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        fs::write(path, bytes).expect("write");
    }

    /// Write a file beside the installation, outside every source boundary.
    fn outside(&self, relative: &str, text: &str) {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        fs::write(path, text).expect("write");
    }

    fn text(&self, relative: &str, text: &str) {
        self.write(relative, text.as_bytes());
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        // Remove links first so cleanup never follows one out of the temp dir.
        remove_links(&self.root);
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn remove_links(path: &Path) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = fs::symlink_metadata(&path).map(|metadata| metadata.file_type()) else {
            continue;
        };
        if kind.is_symlink() || is_junction(&path) {
            let _ = fs::remove_dir(&path).or_else(|_| fs::remove_file(&path));
        } else if kind.is_dir() {
            remove_links(&path);
        }
    }
}

#[cfg(windows)]
fn is_junction(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_attributes() & 0x400 != 0)
}

#[cfg(not(windows))]
fn is_junction(_path: &Path) -> bool {
    false
}

/// Create a directory link (symlink on Unix, junction on Windows).
fn link_dir(target: &Path, link: &Path) -> bool {
    fs::create_dir_all(link.parent().expect("parent")).expect("dirs");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).is_ok()
    }
    #[cfg(windows)]
    {
        let plain = |path: &Path| path.to_string_lossy().replace('/', "\\");
        std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J", &plain(link), &plain(target)])
            .output()
            .is_ok_and(|output| output.status.success())
    }
}

/// Create a file symlink; `false` when the platform denies it (Windows without privilege).
fn link_file(target: &Path, link: &Path) -> bool {
    fs::create_dir_all(link.parent().expect("parent")).expect("dirs");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).is_ok()
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_file(target, link).is_ok()
    }
}

const BOOTSTRAP: &str = "- [Required] 解析 CLI 入口。\n";

fn node(name: &str) -> String {
    format!(
        "---\nname: {name}\ndescription: 適用時使用。\nmetadata:\n  tags:\n    - capability\n---\n\n- [Required] 遵守規範。\n"
    )
}

fn base(tree: &Tree) {
    tree.text("shared/bootstrap.md", BOOTSTRAP);
    tree.text("work/SKILL.md", "entry");
    tree.text("work/instructions/instructions.md", &node("根"));
    tree.text("work/instructions/task/instructions.md", &node("任務"));
    tree.text("tooling/SKILL.md", "entry");
    tree.text("tooling/instructions/instructions.md", &node("根"));
}

fn request(skill: &str, nodes: &[&str]) -> Request {
    Request {
        selections: vec![Selection {
            skill: skill.to_owned(),
            nodes: nodes.iter().map(|node| (*node).to_owned()).collect(),
            references: Vec::new(),
        }],
        modules: Vec::new(),
    }
}

fn reason_of(store: &FsStore, source: &SourceRef) -> Reason {
    store.read(source).expect_err("must be rejected").reason
}

#[test]
fn a_normal_installation_validates_and_resolves() {
    let tree = Tree::new("normal");
    base(&tree);
    let store = FsStore::open(tree.install()).expect("open");
    assert_eq!(store.skills().unwrap(), ["tooling", "work"]);
    let report = validate_all(&store).expect("validation runs");
    assert!(report.is_valid(), "{:?}", report.failures);
    let resolution = resolve(&store, &request("work", &["task"])).expect("resolves");
    assert_eq!(resolution.sources.len(), 3);
    let found = discover(&store, "work", "").expect("discovers");
    assert_eq!(found.children.len(), 1);
}

#[test]
fn encoding_rules_apply_to_real_files() {
    let tree = Tree::new("encoding");
    base(&tree);
    let doc = node("根");
    let mut bom = vec![0xEF, 0xBB, 0xBF];
    bom.extend_from_slice(doc.as_bytes());
    tree.write("work/instructions/bom/instructions.md", &bom);
    let mut twice = vec![0xEF, 0xBB, 0xBF, 0xEF, 0xBB, 0xBF];
    twice.extend_from_slice(doc.as_bytes());
    tree.write("work/instructions/twice.md", &twice);
    tree.write("work/instructions/bad.md", &[0xFF, 0xFE, 0x41]);
    let store = FsStore::open(tree.install()).unwrap();
    let source = |path: &str| SourceRef::skill("work", path);
    assert!(
        store
            .read(&source("instructions/bom/instructions.md"))
            .is_ok()
    );
    assert_eq!(
        reason_of(&store, &source("instructions/twice.md")),
        Reason::MultipleBom
    );
    assert_eq!(
        reason_of(&store, &source("instructions/bad.md")),
        Reason::InvalidUtf8
    );
    assert_eq!(
        reason_of(&store, &source("instructions/missing.md")),
        Reason::Unreadable
    );
    assert_eq!(
        reason_of(&store, &source("instructions/task")),
        Reason::NotRegularFile
    );
}

#[test]
fn path_escapes_are_rejected_before_touching_the_disk() {
    let tree = Tree::new("paths");
    base(&tree);
    tree.outside("secret.md", "outside");
    let store = FsStore::open(tree.install()).unwrap();
    for bad in [
        "../secret.md",
        "/etc/passwd",
        "a/../../x",
        "C:/x",
        "a\\b",
        "",
    ] {
        assert_eq!(
            reason_of(&store, &SourceRef::skill("work", bad)),
            Reason::InvalidPath,
            "{bad}"
        );
    }
    assert_eq!(
        store
            .read(&SourceRef::skill("ghost", "a.md"))
            .unwrap_err()
            .reason,
        Reason::UnknownRoot
    );
    assert_eq!(
        store
            .read(&SourceRef::skill("Bad_Name", "a.md"))
            .unwrap_err()
            .reason,
        Reason::UnknownRoot
    );
}

#[test]
fn links_may_alias_inside_a_boundary_but_never_leave_it() {
    let tree = Tree::new("links");
    base(&tree);
    tree.outside("outside/secret.md", "outside");
    tree.text("other/skill.md", "foreign");
    let install = tree.install();
    let store_for = || FsStore::open(&install).unwrap();

    // Inside one Skill: allowed, and the alias shares its identity.
    assert!(link_dir(
        &install.join("work/instructions/task"),
        &install.join("work/instructions/alias")
    ));
    let store = store_for();
    let direct = store
        .read(&SourceRef::skill(
            "work",
            "instructions/task/instructions.md",
        ))
        .unwrap();
    let aliased = store
        .read(&SourceRef::skill(
            "work",
            "instructions/alias/instructions.md",
        ))
        .unwrap();
    assert_eq!(direct.id, aliased.id);

    // Out of the installation: link escape.
    assert!(link_dir(
        &tree.root.join("outside"),
        &install.join("work/instructions/leak")
    ));
    assert_eq!(
        reason_of(
            &store_for(),
            &SourceRef::skill("work", "instructions/leak/secret.md")
        ),
        Reason::LinkEscape
    );
    assert_eq!(
        store_for()
            .entries(&SourceRoot::Skill("work".to_owned()), "instructions/leak")
            .unwrap_err()
            .reason,
        Reason::LinkEscape
    );

    // Into another Skill or Shared: boundary violation.
    assert!(link_dir(
        &install.join("tooling"),
        &install.join("work/instructions/peer")
    ));
    assert_eq!(
        reason_of(
            &store_for(),
            &SourceRef::skill("work", "instructions/peer/SKILL.md")
        ),
        Reason::BoundaryViolation
    );
    assert!(link_dir(
        &install.join("shared"),
        &install.join("work/instructions/shared")
    ));
    assert_eq!(
        reason_of(
            &store_for(),
            &SourceRef::skill("work", "instructions/shared/bootstrap.md")
        ),
        Reason::BoundaryViolation
    );
    assert!(store_for().read(&SourceRef::shared("bootstrap.md")).is_ok());
}

#[test]
fn a_boundary_that_resolves_outside_the_installation_is_a_mixed_install() {
    let tree = Tree::new("mixed");
    base(&tree);
    tree.outside("elsewhere/foreign/SKILL.md", "foreign");
    assert!(link_dir(
        &tree.root.join("elsewhere/foreign"),
        &tree.install().join("foreign")
    ));
    let error = FsStore::open(tree.install()).unwrap_err();
    assert_eq!(error.reason, Reason::MixedInstall);
}

#[test]
fn file_links_are_validated_like_directory_links() {
    let tree = Tree::new("filelinks");
    base(&tree);
    tree.outside("outside/secret.md", "outside");
    let install = tree.install();
    if !link_file(
        &tree.root.join("outside/secret.md"),
        &install.join("work/instructions/secret.md"),
    ) {
        eprintln!("file symlinks are unavailable here; directory links cover this boundary");
        return;
    }
    let store = FsStore::open(&install).unwrap();
    assert_eq!(
        reason_of(&store, &SourceRef::skill("work", "instructions/secret.md")),
        Reason::LinkEscape
    );
}

#[test]
fn failures_report_scope_and_never_fall_back_to_a_stale_set() {
    let tree = Tree::new("scope");
    base(&tree);
    let store = FsStore::open(tree.install()).unwrap();
    let active = resolve(&store, &request("work", &["task"])).unwrap();
    tree.text("work/instructions/task/instructions.md", "broken");
    let error = resolve(&store, &request("work", &["task"])).unwrap_err();
    assert_eq!(error.reason, Reason::MissingFrontmatter);
    assert_eq!(error.scope, Some(skill_model::FailureScope::Branch));
    assert_eq!(
        active.sources.len(),
        3,
        "the old set is untouched but not reused"
    );
    assert!(
        resolve(&store, &request("tooling", &[""])).is_ok(),
        "other Skills keep working"
    );
}
