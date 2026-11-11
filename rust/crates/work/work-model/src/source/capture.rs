//! Caller-constructible Source capture request.

use serde::{Deserialize, Serialize};

use super::snapshot::{SnapshotSource, SourceContentPath};

/// Request body for `source capture`; the original bytes travel separately.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceCaptureRequest {
    pub requirement_id: String,
    pub captured_at: String,
    pub source: SnapshotSource,
    pub content_path: SourceContentPath,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue_comment_count: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn example() -> serde_json::Value {
        crate::contract::data::registry_value()["items"]["work-source-capture-request"]
            ["description"]["example"]
            .clone()
    }

    #[test]
    fn capture_request_example_matches_its_model_without_generated_fields() {
        let value = example();
        let request: SourceCaptureRequest = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(request).unwrap(), value);
        for generated in ["schema", "source_id", "content"] {
            let mut stale = value.clone();
            stale[generated] = json!({});
            assert!(
                serde_json::from_value::<SourceCaptureRequest>(stale).is_err(),
                "{generated}"
            );
        }
    }

    #[test]
    fn capture_request_keeps_each_source_variant_strict() {
        let mut file = example();
        file["source"] = json!({"kind":"file","path":"input.pdf","media_type":"application/pdf"});
        assert!(serde_json::from_value::<SourceCaptureRequest>(file.clone()).is_ok());
        let mut missing_media_type = file.clone();
        missing_media_type["source"]
            .as_object_mut()
            .unwrap()
            .remove("media_type");
        assert!(serde_json::from_value::<SourceCaptureRequest>(missing_media_type).is_err());

        let mut issue = example();
        issue["source"] = json!({"kind":"github_issue","repository":"owner/repo",
            "number":68,"url":"https://github.com/owner/repo/issues/68"});
        issue["issue_comment_count"] = json!(2);
        let parsed: SourceCaptureRequest = serde_json::from_value(issue.clone()).unwrap();
        assert_eq!(parsed.issue_comment_count, Some(2));

        let mut text = example();
        text["source"] = json!({"kind":"user_text"});
        text.as_object_mut().unwrap().remove("issue_comment_count");
        assert!(serde_json::from_value::<SourceCaptureRequest>(text).is_ok());
    }

    #[test]
    fn capture_request_rejects_non_portable_content_paths() {
        for path in [
            "source",
            "source.",
            "prefix.txt",
            "source.a/b",
            "source.txt ",
            "source.txt.",
            "source.a:b",
        ] {
            let mut value = example();
            value["content_path"] = json!(path);
            assert!(
                serde_json::from_value::<SourceCaptureRequest>(value).is_err(),
                "{path}"
            );
        }
    }
}
