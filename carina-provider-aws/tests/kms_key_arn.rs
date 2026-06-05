use carina_core::resource::{ConcreteValue, Value};

fn assert_accepts(t: carina_core::schema::AttributeType, value: &str) {
    let carina_core::schema::RawShape::Custom { validate, .. } = t.raw_shape() else {
        panic!("type should be custom");
    };
    let value = Value::Concrete(ConcreteValue::String(value.to_string()));
    assert!(validate(&value).is_ok());
}

fn assert_rejects(t: carina_core::schema::AttributeType, value: &str) {
    let carina_core::schema::RawShape::Custom { validate, .. } = t.raw_shape() else {
        panic!("type should be custom");
    };
    let value = Value::Concrete(ConcreteValue::String(value.to_string()));
    assert!(validate(&value).is_err());
}

#[test]
fn arn_accepts_only_kms_key_arn() {
    assert_accepts(
        carina_provider_aws::schemas::generated::kms::key::arn(),
        "arn:aws:kms:us-east-1:123456789012:key/1234abcd-12ab-34cd-56ef-1234567890ab",
    );
    assert_rejects(
        carina_provider_aws::schemas::generated::kms::key::arn(),
        "arn:aws:kms:us-east-1:123456789012:alias/my-key",
    );
    assert_rejects(
        carina_provider_aws::schemas::generated::kms::key::arn(),
        "alias/my-key",
    );
    assert_rejects(
        carina_provider_aws::schemas::generated::kms::key::arn(),
        "1234abcd-12ab-34cd-56ef-1234567890ab",
    );
}

#[test]
fn id_accepts_kms_key_identifier_forms() {
    assert_accepts(
        carina_provider_aws::schemas::generated::kms::key::id(),
        "arn:aws:kms:us-east-1:123456789012:key/1234abcd-12ab-34cd-56ef-1234567890ab",
    );
    assert_accepts(
        carina_provider_aws::schemas::generated::kms::key::id(),
        "arn:aws:kms:us-east-1:123456789012:alias/my-key",
    );
    assert_accepts(
        carina_provider_aws::schemas::generated::kms::key::id(),
        "alias/my-key",
    );
    assert_accepts(
        carina_provider_aws::schemas::generated::kms::key::id(),
        "1234abcd-12ab-34cd-56ef-1234567890ab",
    );
}

#[test]
fn arn_identity_is_provider_scoped() {
    let t = carina_provider_aws::schemas::generated::kms::key::arn();
    let carina_core::schema::RawShape::Custom { identity, .. } = t.raw_shape() else {
        panic!("kms::key::arn() should be Custom");
    };
    assert_eq!(
        identity.map(|id| id.to_string()).as_deref(),
        Some("aws.kms.Key.Arn")
    );
}
