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
    assert_schema_accepts(carina_aws_types::kms_key_id(), value);
}

fn assert_schema_accepts(t: carina_core::schema::AttributeType, value: &str) {
    let schema = rootless_type(t);
    let value: carina_core::resource::Value = carina_core::resource::Value::Concrete(
        carina_core::resource::ConcreteValue::String(value.to_string().into()),
    );
    assert!(schema.validate(&value).is_ok());
}

fn assert_schema_rejects(t: carina_core::schema::AttributeType, value: &str) {
    let schema = rootless_type(t);
    let value: carina_core::resource::Value = carina_core::resource::Value::Concrete(
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
    let carina_core::schema::RawShape::String {
        identity_layer: carina_core::schema::StringIdentityParts { identity, .. },
        ..
    } = t.raw_shape()
    else {
        panic!("kms::key::arn() should be refined String");
    };
    assert_eq!(
        identity.map(|id| id.to_string()).as_deref(),
        Some("aws.kms.Key.Arn")
    );
}

#[test]
fn reference_leaf_is_offline_and_accepts_all_key_identifier_forms() {
    let t = carina_aws_types::kms_key_reference();
    let carina_core::schema::RawShape::Reference { identity, .. } = t.raw_shape() else {
        panic!("expected a reference leaf");
    };
    assert_eq!(identity.to_string(), "aws.kms.Key.Reference");
    for value in [
        "alias/aws/sqs",
        "1234abcd-12ab-34cd-56ef-1234567890ab",
        "mrk-0123456789abcdef0123456789abcdef",
        "arn:aws:kms:us-east-1:123456789012:alias/example",
        "arn:aws:kms:us-east-1:123456789012:key/mrk-0123456789abcdef0123456789abcdef",
    ] {
        assert_schema_accepts(t.clone(), value);
    }
    for value in ["alias/", "random", "arn:aws:kms::123:key/bad"] {
        assert_schema_rejects(t.clone(), value);
    }
}

#[test]
fn key_arn_outputs_are_assignable_to_reference_inputs() {
    use carina_core::schema::{SchemaKind, SchemaRegistry, TypeInSchema, ValidatedConfigSchema};
    let snapshot = carina_core::provider::loaded_provider_snapshot_from_factories(
        std::sync::Arc::new(vec![Box::new(carina_provider_aws::AwsProviderFactory)]),
        SchemaRegistry::new(),
        carina_core::EnvironmentSnapshot::empty(),
    );
    let registry = snapshot.schema_registry();
    let source_schema = ValidatedConfigSchema::validate(std::collections::BTreeMap::from([(
        "arn".into(),
        carina_aws_types::kms_key_arn(),
    )]))
    .expect("KMS ARN source type admission");
    let source = &source_schema.parts()["arn"];
    for (resource, attribute) in [
        ("sqs.Queue", "kms_master_key_id"),
        ("logs.LogGroup", "kms_key_id"),
    ] {
        let schema = registry.get("aws", resource, SchemaKind::Resource).unwrap();
        let sink = schema.as_schema().attributes()[attribute].attr_type();
        assert!(
            TypeInSchema::schemaless(source).is_assignable_to(
                schema.assignment_sink(sink, &snapshot.provider_context().custom_types)
            ),
            "aws.kms.Key.Arn must be assignable to {resource}.{attribute}"
        );
    }
}

#[test]
fn logs_reference_preserves_arn_only_and_smithy_length_constraints() {
    let t = carina_aws_types::kms_key_arn_reference();
    let carina_core::schema::RawShape::Reference { length, .. } = t.raw_shape() else {
        panic!("expected a reference leaf");
    };
    assert_eq!(length, Some((Some(0), Some(256))));
    assert_schema_accepts(
        t.clone(),
        "arn:aws:kms:us-east-1:123456789012:key/1234abcd-12ab-34cd-56ef-1234567890ab",
    );
    assert_schema_rejects(t.clone(), "alias/example");
    assert_schema_rejects(t, "arn:aws:kms:us-east-1:123456789012:alias/example");
}
