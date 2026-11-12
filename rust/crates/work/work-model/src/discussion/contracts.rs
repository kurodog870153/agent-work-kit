//! Public Session contract records and command definitions.

pub const COMMANDS: &[&str] = &[
    "init",
    "read",
    "status",
    "update",
    "question",
    "answer",
    "history",
    "recover",
    "preview",
    "apply",
    "recover-publication",
];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_records_round_trip_with_exact_schema() {
        let records = crate::contract::data::registry().items;
        serde_json::from_value::<super::super::DiscussionSession>(
            records["work-discussion-session"]
                .description
                .example
                .clone(),
        )
        .unwrap();
        serde_json::from_value::<super::super::request::DiscussionRequest>(
            records["work-discussion-request"]
                .description
                .example
                .clone(),
        )
        .unwrap();
        assert_eq!(records.len(), 92);
        serde_json::from_value::<super::super::response::DiscussionResult>(
            records["work-discussion-result"]
                .description
                .example
                .clone(),
        )
        .unwrap();
        assert!(records["work-discussion-request"].scaffold.is_some());
        assert_eq!(COMMANDS.len(), 11);
        let example = records["work-discussion-request"]
            .description
            .example
            .clone();
        let parsed: super::super::request::DiscussionRequest =
            serde_json::from_value(example.clone()).unwrap();
        assert_eq!(
            serde_json::to_value(&parsed).unwrap(),
            example,
            "the request example round-trips without drift"
        );
        let request = &records["work-discussion-request"].description;
        let command = request
            .fields
            .iter()
            .find(|field| field.name == "command")
            .unwrap();
        assert_eq!(command.constraints["variants"], serde_json::json!(COMMANDS));
    }

    #[test]
    fn every_command_branch_is_publicly_described_and_parses() {
        use serde_json::Value;
        let records = crate::contract::data::registry().items;
        let command = records["work-discussion-request"]
            .description
            .fields
            .iter()
            .find(|field| field.name == "command")
            .unwrap();
        let schema = &command.constraints["schema"];
        let mapping = schema["discriminator"]["mapping"].as_object().unwrap();
        let branches: Vec<&String> = mapping.keys().collect();
        assert_eq!(branches.len(), COMMANDS.len());
        for kind in COMMANDS {
            assert!(mapping.contains_key(*kind), "{kind}: mapping");
            let name = mapping[*kind]
                .as_str()
                .unwrap()
                .strip_prefix("#/$defs/")
                .unwrap();
            let def = &schema["$defs"][name];
            assert_eq!(def["properties"]["kind"]["const"], *kind, "{kind}: kind");
            assert_eq!(def["additionalProperties"], false, "{kind}: strict");
            let examples = def["examples"].as_array().unwrap();
            assert!(!examples.is_empty(), "{kind}: example");
            for example in examples {
                let request = serde_json::json!({
                    "schema": "work-discussion-request",
                    "requirement_id": "example",
                    "command": example.clone(),
                });
                let parsed: super::super::request::DiscussionRequest =
                    serde_json::from_value(request)
                        .unwrap_or_else(|error| panic!("{kind}: {error}"));
                assert_eq!(parsed.command.name(), *kind);
            }
            let unknown: Value = {
                let mut command = serde_json::json!({"kind": kind});
                command["unknown_field"] = serde_json::json!(true);
                command
            };
            assert!(
                serde_json::from_value::<super::super::request::DiscussionCommand>(unknown)
                    .is_err(),
                "{kind}: unknown field"
            );
        }
        assert!(schema["oneOf"].as_array().unwrap().len() == COMMANDS.len());
        assert!(
            serde_json::from_value::<super::super::request::DiscussionCommand>(
                serde_json::json!({"kind": "save"})
            )
            .is_err()
        );
        assert!(
            records["work-discussion-session"].scaffold.is_none()
                && records["work-discussion-session"].scaffold_error.is_some()
        );
    }

    #[test]
    fn contract_branch_names_are_the_serde_tags_of_each_variant() {
        let records = crate::contract::data::registry().items;
        let command = &records["work-discussion-request"].description.fields
            [records["work-discussion-request"]
                .description
                .fields
                .iter()
                .position(|field| field.name == "command")
                .unwrap()];
        let schema = &command.constraints["schema"];
        let mapping = schema["discriminator"]["mapping"].as_object().unwrap();
        let mut tags = Vec::new();
        for name in COMMANDS {
            let branch = mapping[*name]
                .as_str()
                .unwrap()
                .strip_prefix("#/$defs/")
                .unwrap();
            let example = schema["$defs"][branch]["examples"][0].clone();
            let parsed: super::super::request::DiscussionCommand =
                serde_json::from_value(example.clone())
                    .unwrap_or_else(|error| panic!("{name}: {error}"));
            let serialized = serde_json::to_value(&parsed).unwrap();
            assert_eq!(serialized["kind"], *name, "{name}: serde tag");
            assert_eq!(serialized, example, "{name}: example round-trips");
            tags.push(name.to_string());
        }
        assert_eq!(tags, COMMANDS, "serde tags must match COMMANDS order");
    }
}
