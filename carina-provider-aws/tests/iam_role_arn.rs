use std::collections::BTreeMap;

use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{AttributeType, ValidatedConfigSchema};

fn validates(attribute_type: AttributeType, value: &str) -> bool {
    let schema =
        ValidatedConfigSchema::validate(BTreeMap::from([("value".to_string(), attribute_type)]))
            .expect("test type must pass schema admission");
    schema
        .rootless_type("value")
        .expect("test type")
        .validate(&Value::Concrete(ConcreteValue::String(value.into())))
        .is_ok()
}

#[test]
fn arn_rejects_non_role_iam_arn() {
    assert!(!validates(
        carina_aws_types::iam_role_arn(),
        "arn:aws:iam::123456789012:policy/Foo"
    ));
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
    assert!(validates(
        carina_aws_types::iam_role_arn(),
        "arn:aws:iam::123456789012:role/MyRole"
    ));
}
