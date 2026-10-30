//! Pure Skill identity, bundle, and selection fingerprints.

pub mod canonical;
pub mod hierarchy;
pub mod instruction;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy)]
pub struct BundleEntry<'a> {
    pub path: &'a str,
    pub normalization: &'a str,
    pub content_sha256: &'a str,
}

pub(crate) fn sha256_hex(raw: &[u8]) -> String {
    let digest = Sha256::digest(raw);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write;
        write!(&mut hex, "{byte:02x}").expect("writing to String cannot fail");
    }
    hex
}

pub fn identity(domain: &str, name: &str, scope: &str, root: &str, source: &str) -> String {
    sha256_hex(format!("{domain}\n{name}\n{scope}\n{root}\n{source}\n").as_bytes())
}

pub fn bundle(domain: &str, entries: &[BundleEntry<'_>]) -> String {
    let mut framed = format!("{domain}\n").into_bytes();
    for entry in entries {
        framed.push(b'F');
        for field in [entry.path, entry.normalization, entry.content_sha256] {
            framed.extend_from_slice(field.len().to_string().as_bytes());
            framed.push(b':');
            framed.extend_from_slice(field.as_bytes());
        }
        framed.push(b'\n');
    }
    framed.extend_from_slice(b"END\n");
    sha256_hex(&framed)
}

pub fn selection(decision: &str, skills: &[Value]) -> String {
    sha256_hex(
        &serde_json::to_vec(&json!({"decision": decision, "skills": skills}))
            .expect("JSON values serialize"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprints_bind_domain_identity_and_bundle_order() {
        assert_ne!(
            identity("domain-a", "ui", "repo", "skills", "ui/SKILL.md"),
            identity("domain-b", "ui", "repo", "skills", "ui/SKILL.md")
        );
        let a = BundleEntry {
            path: "SKILL.md",
            normalization: "raw",
            content_sha256: "a",
        };
        let b = BundleEntry {
            path: "references/guide.md",
            normalization: "raw",
            content_sha256: "b",
        };
        assert_ne!(bundle("domain-a", &[a, b]), bundle("domain-a", &[b, a]));
        assert_ne!(bundle("domain-a", &[a]), bundle("domain-b", &[a]));
    }

    #[test]
    fn selection_binds_decision_and_skill_order() {
        let a = json!({"id":"a"});
        let b = json!({"id":"b"});
        assert_ne!(
            selection("external_skills", &[a.clone(), b.clone()]),
            selection("external_skills", &[b, a.clone()])
        );
        assert_ne!(
            selection("base_only", std::slice::from_ref(&a)),
            selection("external_skills", &[a])
        );
    }
}
