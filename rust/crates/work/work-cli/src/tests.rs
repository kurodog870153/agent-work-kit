use super::*;

#[test]
fn the_registry_is_structurally_valid_and_has_no_migration_surface() {
    let registry = registry().unwrap();
    assert!(registry.command("work source create").is_some());
    assert!(
        registry
            .commands()
            .all(|entry| !entry.path.to_lowercase().contains("migrat"))
    );
    assert!(
        registry
            .contracts()
            .all(|record| !record.description.id.to_lowercase().contains("migrat"))
    );
}

#[test]
fn the_cli_serializes_model_envelopes_and_maps_errors_at_the_entry_boundary() {
    let ok = success("ready");
    assert_eq!(ok.code, OK);
    assert_eq!(
        ok.stdout,
        "{\n  \"status\": \"ok\",\n  \"data\": \"ready\"\n}\n"
    );

    let work = work_failure(&WorkError::new(Reason::ApprovalMismatch, "not approved"));
    assert_eq!(work.code, FAILED);
    assert_eq!(
        work.stdout,
        "{\n  \"status\": \"error\",\n  \"error\": {\n    \"reason\": \"approval_mismatch\",\n    \"message\": \"not approved\"\n  }\n}\n"
    );

    let contract = contract_failure(&contract_core::ContractError::new(
        "contract_not_found",
        "unknown schema",
    ));
    assert_eq!(contract.code, FAILED);
    assert!(
        contract
            .stdout
            .contains("\"reason\": \"contract_not_found\"")
    );
    assert!(contract.stdout.contains("\"message\": \"unknown schema\""));
}
