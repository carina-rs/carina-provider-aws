use carina_provider_aws::schemas::config::aws_validators;

#[test]
fn arn_rejects_non_role_iam_arn() {
    let validators = aws_validators();
    let validate = validators.get("iam_role_arn").unwrap();
    assert!(validate("arn:aws:iam::123456789012:policy/Foo").is_err());
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
    let validators = aws_validators();
    let validate = validators.get("iam_role_arn").unwrap();
    assert!(validate("arn:aws:iam::123456789012:role/MyRole").is_ok());
}
