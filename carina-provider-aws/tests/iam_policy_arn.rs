use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::RawShape;
use carina_provider_aws::schemas::generated::iam;

#[test]
fn policy_arn_identity_is_provider_scoped() {
    let t = iam::policy::arn();
    let RawShape::Custom { identity, .. } = t.raw_shape() else {
        panic!("iam::policy::arn() should be Custom");
    };
    assert_eq!(
        identity.map(|id| id.to_string()).as_deref(),
        Some("aws.iam.Policy.Arn")
    );
}

#[test]
fn policy_arn_rejects_role_arn() {
    let t = iam::policy::arn();
    let RawShape::Custom { validate, .. } = t.raw_shape() else {
        panic!("iam::policy::arn() should be Custom");
    };
    let v = Value::Concrete(ConcreteValue::String(
        "arn:aws:iam::123456789012:role/MyRole".to_string(),
    ));
    assert!(validate(&v).is_err());
}
