use carina_provider_aws::schemas::config::validate_aws_custom_type;

#[test]
fn arn_rejects_non_role_iam_arn() {
    let identity = carina_aws_types::provider_type("iam", "Role", "Arn");
    assert!(validate_aws_custom_type(&identity, "arn:aws:iam::123456789012:policy/Foo").is_err());
}

#[test]
fn arn_identity_is_provider_scoped() {
    let t = carina_provider_aws::schemas::generated::iam::role::arn();
    let carina_core::schema::RawShape::String { identity, .. } = t.raw_shape() else {
        panic!("arn() should be refined String");
    };
    assert_eq!(
        identity.map(|id| id.to_string()).as_deref(),
        Some("aws.iam.Role.Arn")
    );
}

#[test]
fn arn_accepts_valid_role_arn() {
    let identity = carina_aws_types::provider_type("iam", "Role", "Arn");
    assert!(validate_aws_custom_type(&identity, "arn:aws:iam::123456789012:role/MyRole").is_ok());
}
