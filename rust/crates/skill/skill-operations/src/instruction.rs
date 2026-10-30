//! Generic source identity and reference extraction.

use skill_model::instruction::{LoadedSource, SourceSummary};

pub fn source_summary(kind: &str, logical_name: &str, canonical_content: &[u8]) -> SourceSummary {
    SourceSummary {
        kind: kind.into(),
        logical_name: logical_name.into(),
        canonical_sha256: crate::sha256_hex(canonical_content),
    }
}

pub fn references(sources: &[LoadedSource]) -> Vec<String> {
    sources
        .iter()
        .filter(|source| source.summary.kind == "reference")
        .map(|source| source.summary.logical_name.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_hash_preserves_bytes_and_only_references_are_listed() {
        let first = source_summary("reference", "guide", b"a\r\nb");
        let second = source_summary("instruction", "root", b"a\nb");
        assert_ne!(first.canonical_sha256, second.canonical_sha256);
        let sources = vec![
            LoadedSource {
                summary: first,
                canonical_content: b"a\r\nb".to_vec(),
            },
            LoadedSource {
                summary: second,
                canonical_content: b"a\nb".to_vec(),
            },
        ];
        assert_eq!(references(&sources), ["guide"]);
    }
}
