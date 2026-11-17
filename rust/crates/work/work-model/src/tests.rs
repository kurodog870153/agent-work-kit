use super::*;

#[test]
fn identifiers_are_restricted_to_stable_characters() {
    assert!(RecordId::parse("task-01").is_ok());
    assert_eq!(RecordId::parse(""), Err(ModelError::EmptyId));
    assert!(matches!(
        RecordId::parse("task 01"),
        Err(ModelError::InvalidId(_))
    ));
    assert!(RecordId::parse("../escape").is_err());
}

#[test]
fn required_text_rejects_blank_values() {
    assert!(RequiredText::parse("goal", "ship it").is_ok());
    assert_eq!(
        RequiredText::parse("goal", "   "),
        Err(ModelError::EmptyText("goal"))
    );
}

#[test]
fn deserialising_an_invalid_id_fails() {
    assert!(serde_json_free_parse("a b").is_err());
}

fn serde_json_free_parse(value: &str) -> Result<RecordId, ModelError> {
    RecordId::try_from(value.to_owned())
}
