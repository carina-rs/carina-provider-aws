use carina_provider_aws::schemas::config::aws_validators;

fn assert_validator_accepts(validator_name: &str, value: &str) {
    let validators = aws_validators();
    let validate = validators.get(validator_name).unwrap();
    assert!(validate(value).is_ok());
}

fn assert_schema_accepts(t: carina_core::schema::AttributeType, value: &str) {
    let schema = carina_core::schema::Schema::flat(t);
    let value = carina_core::resource::Value::Concrete(
        carina_core::resource::ConcreteValue::String(value.to_string()),
    );
    assert!(schema.validate(&value).is_ok());
}

fn assert_schema_rejects(t: carina_core::schema::AttributeType, value: &str) {
    let schema = carina_core::schema::Schema::flat(t);
    let value = carina_core::resource::Value::Concrete(
        carina_core::resource::ConcreteValue::String(value.to_string()),
    );
    assert!(schema.validate(&value).is_err());
}

#[test]
fn arn_accepts_only_kms_key_arn() {
    assert_schema_accepts(
        carina_provider_aws::schemas::generated::kms::key::arn(),
        "arn:aws:kms:us-east-1:123456789012:key/1234abcd-12ab-34cd-56ef-1234567890ab",
    );
    assert_schema_rejects(
        carina_provider_aws::schemas::generated::kms::key::arn(),
        "arn:aws:kms:us-east-1:123456789012:alias/my-key",
    );
    assert_schema_rejects(
        carina_provider_aws::schemas::generated::kms::key::arn(),
        "alias/my-key",
    );
    assert_schema_rejects(
        carina_provider_aws::schemas::generated::kms::key::arn(),
        "1234abcd-12ab-34cd-56ef-1234567890ab",
    );
}

#[test]
fn id_accepts_kms_key_identifier_forms() {
    assert_validator_accepts(
        "kms_key_id",
        "arn:aws:kms:us-east-1:123456789012:key/1234abcd-12ab-34cd-56ef-1234567890ab",
    );
    assert_validator_accepts(
        "kms_key_id",
        "arn:aws:kms:us-east-1:123456789012:alias/my-key",
    );
    assert_validator_accepts("kms_key_id", "alias/my-key");
    assert_validator_accepts("kms_key_id", "1234abcd-12ab-34cd-56ef-1234567890ab");
}

#[test]
fn arn_identity_is_provider_scoped() {
    let t = carina_provider_aws::schemas::generated::kms::key::arn();
    let carina_core::schema::RawShape::String { identity, .. } = t.raw_shape() else {
        panic!("kms::key::arn() should be refined String");
    };
    assert_eq!(
        identity.map(|id| id.to_string()).as_deref(),
        Some("aws.kms.Key.Arn")
    );
}
