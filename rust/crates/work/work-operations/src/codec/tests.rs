use super::*;
use work_model::{Origin, OriginSource, RecordId, SOURCE_SCHEMA, SourceContent, SourceRecord};

fn record() -> SourceRecord {
    SourceRecord {
        schema: SOURCE_SCHEMA.to_owned(),
        id: RecordId::parse("src-1").unwrap(),
        version: 1,
        title: "t".to_owned(),
        locator: "l".to_owned(),
        content: SourceContent::Text {
            text: "a\r\n  b\t".to_owned(),
        },
        content_length: "a\r\n  b\t".len() as u64,
        content_digest: "codec-test-digest".to_owned(),
        origin: Origin {
            source: OriginSource::External,
            reference: None,
        },
    }
}

#[test]
fn encoding_round_trips_without_changing_bytes() {
    let text = encode(&record()).unwrap();
    assert!(text.ends_with('\n'));
    let decoded: SourceRecord = decode(&text).unwrap();
    assert_eq!(decoded, record());
    assert_eq!(encode(&decoded).unwrap(), text);
}

#[test]
fn unknown_fields_and_malformed_text_are_rejected() {
    let text = encode(&record())
        .unwrap()
        .replace("\"title\"", "\"extra\": 1, \"title\"");
    assert_eq!(
        decode::<SourceRecord>(&text).unwrap_err().reason,
        Reason::InvalidRecord
    );
    assert!(decode::<SourceRecord>("{").is_err());
}
