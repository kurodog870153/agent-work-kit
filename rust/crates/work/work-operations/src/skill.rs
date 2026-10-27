//! Pure skill identity, bundle, and selection fingerprints.

pub use skill_api::bundle::BundleEntry;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_and_bundle_are_order_sensitive() {
        assert_ne!(
            crate::derivation::fingerprint::skill_identity("ui", "repo", "a", "ui/SKILL.md"),
            crate::derivation::fingerprint::skill_identity("ui", "user", "a", "ui/SKILL.md")
        );
        let a = BundleEntry {
            path: "SKILL.md",
            normalization: "canonical-text",
            content_sha256: "a",
        };
        let b = BundleEntry {
            path: "references/guide.md",
            normalization: "canonical-text",
            content_sha256: "b",
        };
        assert_ne!(
            crate::derivation::fingerprint::skill_bundle(&[a, b]),
            crate::derivation::fingerprint::skill_bundle(&[b, a])
        );
        assert_eq!(
            crate::derivation::fingerprint::skill_bundle(&[a, b]),
            "526e9b3ff4527d5a376090ccbc6d2d1ea195c5c95c0f933ad3358b00b282973d"
        );
        assert_eq!(
            crate::derivation::fingerprint::skill_identity("ui", "repo", "a", "ui/SKILL.md"),
            "6522c3fb86097210a45bad7a15c0fe369731e6cfb421d7309b08c32c065829ed"
        );
        assert_eq!(
            crate::derivation::fingerprint::skill_selection("base_only", &[]),
            crate::derivation::fingerprint::structured(
                &serde_json::json!({"decision": "base_only", "skills": []})
            )
            .unwrap()
        );
        assert_eq!(
            crate::derivation::fingerprint::skill_selection("base_only", &[]),
            "a09357ef9f22c43dca16b489da61b287838bf64963a5956bf52ae0327e04a959"
        );
    }
}
