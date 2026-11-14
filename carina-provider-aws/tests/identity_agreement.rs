//! Admit the actual native provider configuration, types and every schema together.
#[test]
fn provider_schema_witness_includes_all_views_and_configuration() {
    let witness = carina_provider_aws::schemas::provider_schema_witness();
    assert_eq!(
        witness.resources().len(),
        carina_provider_aws::schemas::generated::configs().len()
    );
    assert!(witness.config().parts().contains_key("region"));
    assert!(witness.config().parts().contains_key("assume_role"));
}
