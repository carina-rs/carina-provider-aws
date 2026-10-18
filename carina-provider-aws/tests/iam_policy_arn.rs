use carina_core::schema::RawShape;
use carina_provider_aws::schemas::{config::validate_aws_custom_type, generated::iam};

#[test]
fn policy_arn_identity_is_provider_scoped() {
    let t = iam::policy::arn();
    let RawShape::String { identity, .. } = t.raw_shape() else {
        panic!("iam::policy::arn() should be refined String");
    };
    assert_eq!(
        identity.map(|id| id.to_string()).as_deref(),
        Some("aws.iam.Policy.Arn")
    );
}

#[test]
fn policy_arn_rejects_role_arn() {
    let identity = carina_aws_types::provider_type("iam", "Policy", "Arn");
    assert!(validate_aws_custom_type(&identity, "arn:aws:iam::123456789012:role/MyRole").is_err());
}
