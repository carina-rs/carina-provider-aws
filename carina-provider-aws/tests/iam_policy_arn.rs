use std::collections::BTreeMap;

use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{RawShape, ValidatedConfigSchema};
use carina_provider_aws::schemas::generated::iam;

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
    let schema = ValidatedConfigSchema::validate(BTreeMap::from([(
        "value".to_string(),
        carina_aws_types::iam_policy_arn(),
    )]))
    .expect("IAM policy ARN type must pass admission");
    let value: Value = Value::Concrete(ConcreteValue::String(
        "arn:aws:iam::123456789012:role/MyRole".into(),
    ));
    let result = schema
        .rootless_type("value")
        .expect("IAM policy ARN type")
        .validate(&value);
    assert!(result.is_err());
}
