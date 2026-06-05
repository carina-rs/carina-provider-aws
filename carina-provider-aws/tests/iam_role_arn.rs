use carina_core::resource::{ConcreteValue, Value};

#[test]
fn arn_rejects_non_role_iam_arn() {
    let t = carina_provider_aws::schemas::generated::iam::role::arn();
    let carina_core::schema::RawShape::Custom { validate, .. } = t.raw_shape() else {
        panic!("arn() should be custom");
    };
    let v = Value::Concrete(ConcreteValue::String(
        "arn:aws:iam::123456789012:policy/Foo".to_string(),
    ));
    assert!(validate(&v).is_err());
}

#[test]
fn arn_identity_is_provider_scoped() {
    let t = carina_provider_aws::schemas::generated::iam::role::arn();
    let carina_core::schema::RawShape::Custom { identity, .. } = t.raw_shape() else {
        panic!("arn() should be custom");
    };
    assert_eq!(
        identity.map(|id| id.to_string()).as_deref(),
        Some("aws.iam.Role.Arn")
    );
}

#[test]
fn arn_accepts_valid_role_arn() {
    let t = carina_provider_aws::schemas::generated::iam::role::arn();
    let carina_core::schema::RawShape::Custom { validate, .. } = t.raw_shape() else {
        panic!("arn() should be Custom");
    };
    let v = Value::Concrete(ConcreteValue::String(
        "arn:aws:iam::123456789012:role/MyRole".to_string(),
    ));
    assert!(
        validate(&v).is_ok(),
        "should accept valid role ARN: {:?}",
        validate(&v)
    );
}
