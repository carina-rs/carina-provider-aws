use carina_provider_aws::schemas::config::validate_aws_custom_type;

fn rootless_type(attr: carina_core::schema::AttributeType) -> carina_core::schema::RootlessType {
    const NAME: &str = "test_value";
    carina_core::schema::ValidatedConfigSchema::validate(std::collections::BTreeMap::from([(
        NAME.to_string(),
        attr,
    )]))
    .expect("test attribute type must pass config-schema admission")
    .rootless_type(NAME)
    .expect("test attribute type must be present")
}

fn assert_kms_key_id_validator_accepts(value: &str) {
    let identity = carina_aws_types::provider_type("kms", "Key", "Id");
    assert!(validate_aws_custom_type(&identity, value).is_ok());
}

fn assert_schema_accepts(t: carina_core::schema::AttributeType, value: &str) {
    let schema = rootless_type(t);
    let value = carina_core::resource::Value::Concrete(
        carina_core::resource::ConcreteValue::String(value.to_string().into()),
    );
    assert!(schema.validate(&value).is_ok());
}

fn assert_schema_rejects(t: carina_core::schema::AttributeType, value: &str) {
    let schema = rootless_type(t);
    let value = carina_core::resource::Value::Concrete(
        carina_core::resource::ConcreteValue::String(value.to_string().into()),
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
    assert_kms_key_id_validator_accepts(
        "arn:aws:kms:us-east-1:123456789012:key/1234abcd-12ab-34cd-56ef-1234567890ab",
    );
    assert_kms_key_id_validator_accepts("arn:aws:kms:us-east-1:123456789012:alias/my-key");
    assert_kms_key_id_validator_accepts("alias/my-key");
    assert_kms_key_id_validator_accepts("1234abcd-12ab-34cd-56ef-1234567890ab");
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
