use carina_core::schema::RawShape;
use carina_provider_aws::schemas::generated::organizations;

#[test]
fn account_arn_identity_is_provider_scoped() {
    let t = organizations::account::arn();
    let RawShape::Custom { identity, .. } = t.raw_shape() else {
        panic!("organizations::account::arn() should be Custom");
    };
    assert_eq!(
        identity.map(|id| id.to_string()).as_deref(),
        Some("aws.organizations.Account.Arn")
    );
}

#[test]
fn organization_arn_identity_is_provider_scoped() {
    let t = organizations::organization::arn();
    let RawShape::Custom { identity, .. } = t.raw_shape() else {
        panic!("organizations::organization::arn() should be Custom");
    };
    assert_eq!(
        identity.map(|id| id.to_string()).as_deref(),
        Some("aws.organizations.Organization.Arn")
    );
}
