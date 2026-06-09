use carina_core::schema::RawShape;
use carina_provider_aws::schemas::generated::sts;

#[test]
fn caller_identity_arn_identity_is_provider_scoped() {
    let t = sts::caller_identity::arn();
    let RawShape::String { identity, .. } = t.raw_shape() else {
        panic!("sts::caller_identity::arn() should be refined String");
    };
    assert_eq!(
        identity.map(|id| id.to_string()).as_deref(),
        Some("aws.sts.CallerIdentity.Arn")
    );
}
