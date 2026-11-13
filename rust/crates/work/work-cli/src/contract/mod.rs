//! Scaffold ordering for Work's canonical JSON transport.

use std::sync::OnceLock;

use serde_json::Value;

pub fn scaffold_order(contract_id: &str) -> Option<&'static Value> {
    static ORDERS: OnceLock<Value> = OnceLock::new();
    let orders = ORDERS.get_or_init(work_model::contract::data::scaffold_orders);
    orders.get(contract_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn registry() -> &'static Value {
        static REGISTRY: OnceLock<Value> = OnceLock::new();
        REGISTRY.get_or_init(work_model::contract::data::registry_value)
    }

    fn snapshot_catalog() -> &'static Value {
        &registry()["catalog"]
    }

    fn snapshot_description(id: &str) -> Option<&'static Value> {
        registry()["items"]
            .get(id)
            .map(|record| &record["description"])
    }

    fn snapshot_scaffold(id: &str) -> Option<&'static Value> {
        registry()["items"]
            .get(id)
            .and_then(|record| record.get("scaffold"))
    }

    fn snapshot_error(id: &str) -> Option<&'static Value> {
        registry()["items"]
            .get(id)
            .and_then(|record| record.get("scaffold_error"))
    }

    #[test]
    fn frozen_catalog_keeps_public_descriptions_and_scaffolds() {
        assert_eq!(
            snapshot_catalog()["contracts"].as_array().unwrap().len(),
            92
        );
        assert_eq!(
            snapshot_description("work-discussion-request").unwrap()["kind"],
            "semantic_request"
        );
        assert_eq!(
            snapshot_scaffold("work-discussion-request").unwrap()["schema"],
            "work-contract-scaffold"
        );
        assert_eq!(
            snapshot_error("work-attempt").unwrap()["reason_code"],
            "contract_scaffold_requires_request"
        );
    }

    #[test]
    fn all_frozen_examples_follow_declared_canonical_fields() {
        for (id, record) in registry()["items"].as_object().unwrap() {
            let description = &record["description"];
            let order = description["canonical_order"].as_array().unwrap();
            let fields: Vec<_> = description["fields"]
                .as_array()
                .unwrap()
                .iter()
                .map(|field| field["name"].clone())
                .collect();
            assert_eq!(order, &fields, "{id}: canonical order");
            let example = description["example"].as_object().unwrap();
            for required in description["required"].as_array().unwrap() {
                assert!(
                    example.contains_key(required.as_str().unwrap()),
                    "{id}: required"
                );
            }
            let projected: Vec<_> = order
                .iter()
                .filter(|field| example.contains_key(field.as_str().unwrap()))
                .collect();
            assert_eq!(projected.len(), example.len(), "{id}: example fields");
            if let Some(schema) = example.get("schema") {
                assert_eq!(schema, id, "{id}: schema");
            }
        }
    }

    #[test]
    fn every_public_work_example_matches_its_declared_shape_and_applicable_validator() {
        fn matches_type(value: &Value, declared: &str) -> bool {
            declared.split(" | ").any(|part| match part {
                "any" | "literal" => true,
                "null" => value.is_null(),
                "string" => value.is_string(),
                "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
                "boolean" => value.is_boolean(),
                _ if part.starts_with("array") => value.is_array(),
                _ if part.starts_with("object") => value.is_object(),
                _ => false,
            })
        }
        let validated = [
            "work-execution-deviation-proposal",
            "work-execution-deviation",
            "work-execution-deviation-preview",
            "work-execution-deviation-record",
            "work-execution-deviation-semantic-request",
            "work-spec-prepare-request",
            "work-spec-verification-request",
        ];
        let mut count = 0;
        let mut model_count = 0;
        let mut presence_mismatches = Vec::new();
        for (id, scenarios) in contract_core::examples() {
            if !id.starts_with("work-") {
                continue;
            }
            let sample = &scenarios["default"];
            assert_eq!(sample["valid"], true, "{id}");
            let record = &registry()["items"][id];
            let description = &record["description"];
            let example = sample["example"].as_object().expect("Work example object");
            for required in description["required"].as_array().expect("required fields") {
                assert!(
                    example.contains_key(required.as_str().unwrap()),
                    "{id}: {required}"
                );
            }
            let declared: std::collections::BTreeSet<_> = description["fields"]
                .as_array()
                .unwrap()
                .iter()
                .map(|field| field["name"].as_str().unwrap())
                .collect();
            assert!(
                example
                    .keys()
                    .all(|field| declared.contains(field.as_str())),
                "{id}: undeclared field"
            );
            for field in description["fields"].as_array().unwrap() {
                let name = field["name"].as_str().unwrap();
                if let Some(value) = example.get(name) {
                    assert!(
                        matches_type(value, field["type"].as_str().unwrap()),
                        "{id}/{name}: example type differs from metadata"
                    );
                }
            }
            macro_rules! model {
                ($type:ty) => {
                    {
                        serde_json::from_value::<$type>(sample["example"].clone())
                            .unwrap_or_else(|error| panic!("{id}: Rust model: {error}"));
                        for field in description["fields"].as_array().unwrap() {
                            let name = field["name"].as_str().unwrap();
                            let mut omitted = sample["example"].clone();
                            omitted.as_object_mut().unwrap().remove(name);
                            let accepts_omission = serde_json::from_value::<$type>(omitted).is_ok();
                            if accepts_omission != (field["required"] == false) {
                                presence_mismatches.push(format!("{id}/{name}: model omission={accepts_omission}, metadata required={}", field["required"]));
                            }
                            if field["type"].as_str().unwrap().split(" | ").any(|part| part == "null") {
                                let mut explicit_null = sample["example"].clone();
                                explicit_null[name] = Value::Null;
                                assert!(
                                    serde_json::from_value::<$type>(explicit_null).is_ok(),
                                    "{id}/{name}: declared nullable but Rust model rejects null"
                                );
                            }
                        }
                        model_count += 1;
                    }
                };
            }
            match id {
                "work-cli-result" => {
                    model!(work_model::contract::CliEnvelope);
                }
                "work-contract-catalog" => {
                    model!(work_model::contract::ContractCatalog);
                }
                "work-contract-description" => {
                    model!(work_model::contract::ContractDescription);
                }
                "work-contract-scaffold" => {
                    model!(work_model::contract::ContractScaffold);
                }
                "work-error" => {
                    model!(work_model::contract::WorkErrorData);
                }
                "work-discussion-request" => {
                    model!(work_model::discussion::request::DiscussionRequest);
                }
                "work-discussion-result" => {
                    model!(work_model::discussion::response::DiscussionResult);
                }
                "work-discussion-session" => {
                    model!(work_model::discussion::DiscussionSession);
                }
                "work-source-capture-request" => {
                    model!(work_model::source::capture::SourceCaptureRequest);
                }
                "work-source-snapshot" => {
                    model!(work_model::source::snapshot::SourceSnapshot);
                }
                "work-source-read" => {
                    model!(work_model::source::snapshot::SourceRead);
                }
                "work-source-validation" => {
                    model!(work_model::source::snapshot::SourceValidation);
                }
                "work-source-impact" => {
                    model!(work_model::source::SourceImpact);
                }
                "work-task-index" => {
                    model!(work_model::task::index::TaskIndex);
                }
                "work-task-item" => {
                    model!(work_model::task::item::TaskItem);
                }
                "work-task-collection-fingerprint" => {
                    model!(work_model::task::TaskCollectionFingerprint);
                }
                "work-task-collection-projection" => {
                    model!(work_model::task::projection::TaskCollectionProjection);
                }
                "work-task-index-validation" => {
                    model!(work_model::task::response::TaskIndexValidation);
                }
                "work-task-item-validation" => {
                    model!(work_model::task::response::TaskItemValidation);
                }
                "work-task-collection-validation" => {
                    model!(work_model::task::response::TaskCollectionValidation);
                }
                "work-file-transaction-request" => {
                    model!(work_model::execution::file_transaction::FileTransactionRequest);
                }
                "work-file-transaction-preview" => {
                    model!(work_model::execution::file_transaction::FileTransactionPreview);
                }
                "work-file-recovery-preview" => {
                    model!(work_model::execution::file_transaction::FileRecoveryPreview);
                }
                "work-file-transaction-result" => {
                    model!(work_model::execution::file_transaction::FileTransactionResult);
                }
                "work-host-execution-report" => {
                    model!(work_model::execution::host_report::HostExecutionReport);
                }
                "work-host-file-report" => {
                    model!(work_model::execution::host_file_report::HostFileReport);
                }
                "work-correction" => {
                    model!(work_model::execution::correction::Correction);
                }
                "work-execution-recovery-evidence" => {
                    model!(work_model::execution::recovery::ExecutionRecoveryEvidence);
                }
                "work-execution-index" => {
                    model!(work_model::execution::index::ExecutionIndex);
                }
                "work-attempt" => {
                    model!(work_model::execution::attempt::Attempt);
                }
                "work-attempt-authorization" => {
                    model!(work_model::execution::attempt::AttemptAuthorization);
                }
                "work-artifact-migration-analysis" => {
                    model!(work_model::specification::ArtifactMigrationAnalysis);
                }
                "work-artifact-migration-request" => {
                    model!(work_model::specification::ArtifactMigrationRequest);
                }
                "work-spec-migration-preview-request" => {
                    model!(work_model::specification::SpecMigrationPreviewRequest);
                }
                "work-spec-reconciliation-preview-request" => {
                    model!(work_model::specification::SpecReconciliationPreviewRequest);
                }
                "work-spec-update-request" => {
                    model!(work_model::specification::SpecUpdateRequest);
                }
                "work-hierarchy" => {
                    model!(work_model::hierarchy::Hierarchy);
                }
                "work-hierarchy-selection" => {
                    model!(work_model::hierarchy::HierarchySelection);
                }
                "work-hierarchy-selection-validation" => {
                    model!(work_model::hierarchy::HierarchySelectionValidation);
                }
                "work-instruction-catalog" => {
                    model!(work_model::instruction::InstructionCatalog);
                }
                "work-instruction-selection" => {
                    model!(work_model::instruction::InstructionSelectionResponse);
                }
                "work-instruction-selection-manifest" => {
                    model!(work_model::instruction::InstructionSelectionManifest);
                }
                "work-instructions" => {
                    model!(work_model::instruction::InstructionsResponse);
                }
                "work-skill-selection" => {
                    model!(work_model::skill::SkillSelection);
                }
                "work-skill-selection-validation" => {
                    model!(work_model::skill::SkillSelectionValidation);
                }
                "work-workflow-state" => {
                    model!(work_model::workflow::WorkflowState);
                }
                "work-attempt-close" => {
                    model!(work_model::execution::response::AttemptCloseResponse);
                }
                "work-attempt-close-request" => {
                    model!(work_model::execution::request::AttemptCloseRequest);
                }
                "work-attempt-start" => {
                    model!(work_model::execution::response::AttemptStartResponse);
                }
                "work-attempt-start-prepare" => {
                    model!(work_model::execution::response::AttemptStartPrepareResponse);
                }
                "work-attempt-start-prepare-request" => {
                    model!(work_model::execution::request::AttemptStartPrepareRequest);
                }
                "work-attempt-start-recovery" => {
                    model!(work_model::execution::response::AttemptStartRecoveryResponse);
                }
                "work-attempt-start-request" => {
                    model!(work_model::execution::request::AttemptStartRequest);
                }
                "work-attempt-validation" => {
                    model!(work_model::execution::response::AttemptValidation);
                }
                "work-command-correction" => {
                    model!(work_model::execution::response::CommandCorrectionResponse);
                }
                "work-command-correction-request" => {
                    model!(work_model::execution::request::CommandCorrectionRequest);
                }
                "work-command-preview" => {
                    model!(work_model::execution::response::PreparedCommandPreview);
                }
                "work-command-result" => {
                    model!(work_model::execution::response::PreparedCommandResult);
                }
                "work-command-run-request" => {
                    model!(work_model::execution::request::CommandRunRequest);
                }
                "work-correction-create" => {
                    model!(work_model::execution::response::CorrectionCreateResponse);
                }
                "work-correction-create-request" => {
                    model!(work_model::execution::request::CorrectionCreateRequest);
                }
                "work-execute-preflight" => {
                    model!(work_model::execution::response::ExecutePreflight);
                }
                "work-execute-worktree" => {
                    model!(work_model::execution::response::ExecuteWorktree);
                }
                "work-execute-worktree-snapshot" => {
                    model!(work_model::execution::response::WorktreeSnapshot);
                }
                "work-execution-deviation" => {
                    model!(work_model::execution::deviation::ExecutionDeviation);
                }
                "work-execution-deviation-preview" => {
                    model!(work_model::execution::response::DeviationPreview);
                }
                "work-execution-deviation-proposal" => {
                    model!(work_model::execution::deviation::DeviationProposal);
                }
                "work-execution-deviation-record" => {
                    model!(work_model::execution::response::DeviationRecordResponse);
                }
                "work-execution-deviation-semantic-request" => {
                    model!(work_model::execution::request::DeviationSemanticRequest);
                }
                "work-execution-recovery" => {
                    model!(work_model::execution::response::ExecutionRecoveryResponse);
                }
                "work-execution-recovery-prepare" => {
                    model!(work_model::execution::response::PreparedExecutionRecoveryPrepare);
                }
                "work-execution-recovery-prepare-request" => {
                    model!(work_model::execution::request::ExecutionRecoveryPrepareRequest);
                }
                "work-execution-recovery-request" => {
                    model!(work_model::execution::request::PreparedExecutionRecoveryRequest);
                }
                "work-record-begin" => {
                    model!(work_model::execution::response::RecordBeginResponse);
                }
                "work-record-finish" => {
                    model!(work_model::execution::response::RecordFinishResponse);
                }
                "work-record-finish-request" => {
                    model!(work_model::execution::request::RecordFinishRequest);
                }
                "work-skill-bundle" => {
                    model!(work_model::skill::SkillBundle);
                }
                "work-skill-catalog" => {
                    model!(work_model::skill::SkillCatalog);
                }
                "work-skill-snapshot" => {
                    model!(work_model::skill::SkillSnapshot);
                }
                "work-spec-prepare-request" => {
                    model!(work_model::specification::SpecPrepareRequest);
                }
                "work-spec-migration-prepare-request" => {
                    model!(work_model::specification::SpecMigrationPrepareRequest);
                }
                "work-spec-reconciliation-prepare-request" => {
                    model!(work_model::specification::SpecReconciliationPrepareRequest);
                }
                "work-spec-verification-request" => {
                    model!(work_model::specification::SpecVerificationRequest);
                }
                "work-spec-migration-preview" => {
                    model!(work_model::specification::SpecMigrationPreview);
                }
                "work-spec-migration-publication" => {
                    model!(work_model::specification::SpecMigrationPublication);
                }
                "work-spec-reconciliation-preview" => {
                    model!(work_model::specification::SpecReconciliationPreview);
                }
                "work-spec-reconciliation-publication" => {
                    model!(work_model::specification::SpecReconciliationPublication);
                }
                "work-spec-verification" => {
                    model!(work_model::specification::SpecVerification);
                }
                "work-command-started" => {
                    let started = &sample["example"];
                    assert_eq!(started["schema"], "work-command-started");
                    assert!(started["authorization_evidence"].is_string());
                    serde_json::from_value::<work_model::execution::response::PreparedCommandPreview>(
                        started["preview"].clone(),
                    )
                    .unwrap_or_else(|error| panic!("{id}: nested preview model: {error}"));
                    model_count += 1;
                }
                _ => panic!("{id}: no applicable Rust Model/Validator classification"),
            }
            if validated.contains(&id) {
                work_infrastructure::fixture_support::validate_contract_example(
                    id,
                    &sample["example"],
                )
                .unwrap_or_else(|error| panic!("{id}: {error}"));
            }
            count += 1;
        }
        assert_eq!(count, 87);
        assert_eq!(model_count, count);
        assert!(
            presence_mismatches.is_empty(),
            "{}",
            presence_mismatches.join("\n")
        );
    }

    #[test]
    fn discussion_nested_required_nullable_metadata_matches_model() {
        let id = "work-discussion-session";
        let description = snapshot_description(id).unwrap();
        let example = &description["example"];
        for field in description["fields"].as_array().unwrap() {
            let Some(nested) = field["constraints"]["required_nullable"].as_array() else {
                continue;
            };
            let parent = field["name"].as_str().unwrap();
            for child in nested {
                let name = child.as_str().unwrap();
                let mut omitted = example.clone();
                omitted[parent].as_object_mut().unwrap().remove(name);
                assert!(
                    serde_json::from_value::<work_model::discussion::DiscussionSession>(omitted)
                        .is_err(),
                    "{id}/{parent}/{name}: missing required nullable accepted"
                );
                let mut explicit_null = example.clone();
                explicit_null[parent][name] = Value::Null;
                assert!(
                    serde_json::from_value::<work_model::discussion::DiscussionSession>(
                        explicit_null
                    )
                    .is_ok(),
                    "{id}/{parent}/{name}: null rejected"
                );
            }
        }
    }

    #[test]
    fn reconciliation_prepare_example_uses_semantic_positions() {
        let id = "work-spec-reconciliation-prepare-request";
        let description = snapshot_description(id).unwrap();
        let example = &description["example"];
        assert_eq!(example["schema"], id);
        assert_eq!(example["task_position"], 1);
        assert_eq!(example["attempt_position"], 1);
        for key in ["attempt_path", "plan_path", "deviation_ids"] {
            assert!(example.get(key).is_none(), "{key}");
        }
        assert_eq!(snapshot_scaffold(id).unwrap()["example"], *example);
    }

    #[test]
    fn execution_deviation_examples_validate_with_declared_field_order() {
        for id in [
            "work-execution-deviation-proposal",
            "work-execution-deviation",
            "work-execution-deviation-preview",
            "work-execution-deviation-record",
            "work-execution-deviation-semantic-request",
        ] {
            let description = snapshot_description(id).unwrap();
            let example = &description["example"];
            work_infrastructure::fixture_support::validate_contract_example(id, example)
                .unwrap_or_else(|error| panic!("{id}: {error}"));
            let order = description["canonical_order"].as_array().unwrap();
            let projected = order
                .iter()
                .filter(|field| example.get(field.as_str().unwrap()).is_some())
                .count();
            assert_eq!(projected, example.as_object().unwrap().len(), "{id}");
        }
        let artifact = snapshot_description("work-execution-deviation").unwrap()["example"].clone();
        let authorization = &artifact["supplemental_authorization"];
        assert_eq!(
            authorization["schema"],
            "work-execution-deviation-authorization"
        );
        assert_eq!(
            authorization["preview_sha256"],
            artifact["approved_preview_sha256"]
        );
        assert_eq!(authorization["action"], artifact["proposal"]["action"]);
        assert_eq!(authorization.as_object().unwrap().len(), 5);
        work_infrastructure::fixture_support::validate_contract_example(
            "work-execution-deviation",
            &artifact,
        )
        .unwrap();
    }

    #[test]
    fn public_registry_catalog_scaffolds_and_errors_match_current_contract() {
        let catalog = snapshot_catalog();
        assert_eq!(catalog["schema"], "work-contract-catalog");
        let entries = catalog["contracts"].as_array().unwrap();
        let ids: Vec<_> = entries
            .iter()
            .map(|entry| entry["id"].as_str().unwrap())
            .collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted);
        sorted.dedup();
        assert_eq!(ids.len(), sorted.len());
        let kinds: std::collections::BTreeSet<_> = entries
            .iter()
            .map(|entry| entry["kind"].as_str().unwrap())
            .collect();
        assert_eq!(
            kinds,
            std::collections::BTreeSet::from([
                "semantic_request",
                "generated_request",
                "response",
                "artifact",
                "envelope"
            ])
        );
        assert_eq!(ids.len(), registry()["items"].as_object().unwrap().len());
        assert!(ids.contains(&"work-contract-description"));
        assert!(ids.contains(&"work-contract-scaffold"));
        for entry in entries {
            let id = entry["id"].as_str().unwrap();
            let description = snapshot_description(id).unwrap();
            assert_eq!(description["id"], id);
            assert_eq!(description["kind"], entry["kind"]);
            if entry["caller_constructible"] == true {
                let scaffold = snapshot_scaffold(id).unwrap();
                assert_eq!(scaffold["id"], id);
                assert_eq!(scaffold["canonical_order"], description["canonical_order"]);
                let fields: std::collections::BTreeSet<_> = scaffold["scaffold"]
                    .as_object()
                    .unwrap()
                    .keys()
                    .map(String::as_str)
                    .collect();
                let expected: std::collections::BTreeSet<_> = description["canonical_order"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|field| field.as_str().unwrap())
                    .collect();
                assert_eq!(fields, expected, "{id}: scaffold fields");
                assert_eq!(scaffold["example"], description["example"]);
            }
        }
        let task = snapshot_scaffold("work-discussion-request").unwrap();
        assert_eq!(task["example"]["command"]["kind"], "read");
        assert_eq!(
            snapshot_error("work-command-run-request").unwrap()["reason_code"],
            "command_launcher_retired"
        );
        let generated = "work-spec-update-request";
        let description = snapshot_description(generated).unwrap();
        assert_eq!(description["kind"], "generated_request");
        assert_eq!(description["caller_constructible"], false);
        assert_eq!(
            snapshot_error(generated).unwrap()["reason_code"],
            "generated_request_not_caller_constructible"
        );
        assert_eq!(
            snapshot_error("work-contract-catalog").unwrap()["reason_code"],
            "contract_scaffold_requires_request"
        );
        assert!(snapshot_description("work-unknown/v1").is_none());
    }

    #[test]
    fn command_correction_description_discriminates_strict_command_modes() {
        let description = snapshot_description("work-command-correction-request").unwrap();
        let command = description["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|field| field["name"] == "actual_command")
            .unwrap();
        let schema = &command["constraints"]["schema"];
        assert_eq!(schema["discriminator"]["propertyName"], "mode");
        assert_eq!(schema["oneOf"].as_array().unwrap().len(), 2);
        for (name, required) in [
            ("SemanticArgvCommandModel", json!(["mode", "argv"])),
            ("SemanticShellCommandModel", json!(["mode", "script"])),
        ] {
            assert_eq!(schema["$defs"][name]["required"], required);
            assert_eq!(schema["$defs"][name]["additionalProperties"], false);
        }
    }

    #[test]
    fn hierarchy_instruction_and_skill_examples_keep_canonical_fields() {
        for (id, fields) in [
            (
                "work-hierarchy",
                vec![
                    "schema",
                    "work_directory",
                    "selected_paths",
                    "resolved_paths",
                    "required_paths",
                    "optional_paths",
                ],
            ),
            (
                "work-instruction-catalog",
                vec![
                    "schema",
                    "mode",
                    "paths",
                    "children",
                    "metadata",
                    "catalog_sha256",
                ],
            ),
            (
                "work-skill-bundle",
                vec!["schema", "files", "bundle_sha256"],
            ),
            (
                "work-skill-catalog",
                vec!["schema", "skills", "unavailable"],
            ),
            (
                "work-skill-selection",
                vec!["schema", "decision", "skills", "selection_sha256"],
            ),
            (
                "work-skill-selection-validation",
                vec!["schema", "status", "skill_selection"],
            ),
            ("work-skill-snapshot", vec!["schema", "skill", "bundle"]),
        ] {
            let description = snapshot_description(id).unwrap();
            assert_eq!(description["canonical_order"], json!(fields), "{id}");
            assert_eq!(description["required"], json!(fields), "{id}");
            assert_eq!(description["example"]["schema"], id, "{id}");
            let example = description["example"].as_object().unwrap();
            assert_eq!(example.len(), fields.len(), "{id}");
            for field in fields {
                assert!(example.contains_key(field), "{id}: {field}");
            }
        }
    }

    #[test]
    fn retired_operation_context_contracts_are_not_public() {
        assert!(snapshot_description("work-operation-envelope").is_none());
        assert!(snapshot_description("work-operation-result").is_none());
    }

    #[test]
    fn specification_request_descriptions_validate_examples_and_nested_references() {
        let prepare = snapshot_description("work-spec-prepare-request").unwrap();
        assert_eq!(
            prepare["required"],
            json!(["schema", "requirement_id", "reason", "edits"])
        );
        work_infrastructure::fixture_support::validate_contract_example(
            "work-spec-prepare-request",
            &prepare["example"],
        )
        .unwrap();
        let verify = snapshot_description("work-spec-verification-request").unwrap();
        assert_eq!(
            verify["required"],
            json!(["schema", "requirement_id", "artifacts", "record_id"])
        );
        work_infrastructure::fixture_support::validate_contract_example(
            "work-spec-verification-request",
            &verify["example"],
        )
        .unwrap();
        let update = snapshot_description("work-spec-update-request").unwrap();
        let references = update["fields"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|field| {
                field["reference"]
                    .as_str()
                    .map(|reference| (field["name"].as_str().unwrap(), reference))
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(
            references,
            std::collections::BTreeMap::from([
                ("task_index", "work-task-index"),
                ("task_items", "work-task-item"),
            ])
        );
        let example = &update["example"];
        assert_eq!(example["schema"], "work-spec-update-request");
        assert!(example.get("plan").is_none());
        assert_eq!(example["task_index"]["schema"], "work-task-index");
        assert_eq!(
            example["task_items"]["TASK-001"]["schema"],
            "work-task-item"
        );
        for value in [
            prepare["example"].clone(),
            verify["example"].clone(),
            example.clone(),
        ] {
            let raw = work_infrastructure::codec::canonical_json(&value).unwrap();
            assert_eq!(
                work_infrastructure::codec::parse_json_contract(&raw).unwrap(),
                value
            );
        }
    }
}
