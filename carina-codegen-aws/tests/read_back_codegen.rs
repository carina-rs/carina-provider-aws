use carina_codegen_aws::resource_defs;

#[test]
fn read_back_declaration_rejects_nonexistent_attribute() {
    let resource = resource_defs::acm_resources().remove(0);
    let mut overrides = resource.read_back_overrides();
    assert!(overrides.validate(["domain_validation_options"]).is_ok());
    overrides.await_attributes[0].attribute = "nonexistent_attribute";
    let error = overrides
        .validate(["domain_validation_options"])
        .unwrap_err();
    assert!(error.contains("nonexistent_attribute"), "{error}");
}

#[test]
fn read_back_codegen_emits_dns_requirement_and_acm_budget() {
    let binary = env!("CARGO_BIN_EXE_smithy-codegen");
    let output = tempfile::tempdir().unwrap();
    let models = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../carina-provider-aws/tests/fixtures/smithy");
    let result = std::process::Command::new(binary)
        .args([
            "--model-dir",
            models.to_str().unwrap(),
            "--output-dir",
            output.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let certificate = std::fs::read_to_string(output.path().join("acm/certificate.rs")).unwrap();
    assert!(certificate.contains("await_on_read_back"));
    assert!(certificate.contains("AcmDnsDvo"));
    assert!(
        certificate.contains(")).deferred_populate()"),
        "DVO references must retain validation warnings for EMAIL and partial reads"
    );
    assert!(
        certificate
            .contains("read_back: carina_provider_protocol::types::ReadBackBudget::from_secs(300)"),
        "ACM overrides the read-back budget to 300 seconds"
    );
    assert!(!certificate.contains("..Default::default()"));
    for field in [
        "delete_timeout_secs",
        "delete_max_retries",
        "create_timeout_secs",
        "create_max_retries",
        "update_timeout_secs",
    ] {
        assert!(
            certificate.contains(&format!("{field}: None")),
            "OperationConfig must name {field}"
        );
    }
    let vpc = std::fs::read_to_string(output.path().join("ec2/vpc.rs")).unwrap();
    assert!(!vpc.contains(".with_operation_config"));
}

#[test]
fn read_back_non_default_budget_is_emitted_in_schema_and_docs() {
    let resource = resource_defs::acm_resources().remove(0);
    let overrides = resource.read_back_overrides();
    assert!(
        overrides
            .operation_config_rust()
            .unwrap()
            .contains("read_back: carina_provider_protocol::types::ReadBackBudget::from_secs(300)")
    );
    assert!(overrides.budget_markdown().unwrap().contains("300 seconds"));
    let default_overrides = resource_defs::ec2_resources()
        .into_iter()
        .find(|resource| resource.name == "ec2.Vpc")
        .unwrap()
        .read_back_overrides();
    assert!(default_overrides.budget.is_none());
    assert!(default_overrides.operation_config_rust().is_none());
    assert!(default_overrides.budget_markdown().is_none());
}
