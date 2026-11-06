use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use carina_core::protocol_convert::{protocol_to_resource_schema, resource_schema_to_protocol};
use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{ResourceSchema, SchemaRegistry, ValidatedResourceSchema};
use carina_provider_aws::schemas::generated::{acm::certificate, iam::role, logs::log_group};

fn tags(count: usize) -> Value {
    Value::Concrete(ConcreteValue::Map(
        (0..count)
            .map(|index| {
                (
                    format!("key{index}"),
                    Value::Concrete(ConcreteValue::String("value".into())),
                )
            })
            .collect(),
    ))
}

fn tag_schemas(schema: ResourceSchema) -> [ValidatedResourceSchema; 2] {
    let native = schema.validate().expect("native schema admission");
    let wire = resource_schema_to_protocol(&native).expect("encode schema");
    let decoded = protocol_to_resource_schema(&wire).expect("decode protocol 22 schema");
    [native, decoded]
}

fn check_tag_count(schema: ResourceSchema, minimum: usize, maximum: usize) {
    for schema in tag_schemas(schema) {
        let attr = schema.effective_attribute("tags").expect("tags attribute");
        for count in [minimum, maximum] {
            attr.validate(&tags(count))
                .expect("model boundary accepted");
        }
        assert!(
            attr.validate(&tags(maximum + 1)).is_err(),
            "{} accepted {} tags",
            schema.parts().resource_type,
            maximum + 1
        );
        if minimum > 0 {
            assert!(
                attr.validate(&tags(0)).is_err(),
                "minimum entry count must be enforced"
            );
        }
    }
}

#[test]
fn map_length_logs_log_group_tags_validate_through_core_and_protocol() {
    check_tag_count(log_group::logs_log_group_config().schema, 1, 50);
}

#[test]
fn map_length_logs_tag_strings_validate_through_core_and_protocol() {
    for schema in tag_schemas(log_group::logs_log_group_config().schema) {
        let attr = schema.effective_attribute("tags").unwrap();
        for (key, value, accepted) in [
            ("Name".to_string(), "valid value".to_string(), true),
            ("日本語".to_string(), "値".to_string(), true),
            ("k".repeat(128), "v".repeat(256), true),
            ("Name".to_string(), String::new(), true),
            (String::new(), "value".to_string(), false),
            ("k".repeat(129), "value".to_string(), false),
            ("bad!key".to_string(), "value".to_string(), false),
            ("Name".to_string(), "bad!value".to_string(), false),
            ("Name".to_string(), "v".repeat(257), false),
        ] {
            let tag_value = Value::Concrete(ConcreteValue::String(value.clone().into()));
            let tags: Value = Value::Concrete(ConcreteValue::Map(
                [(key.clone(), tag_value)].into_iter().collect(),
            ));
            assert_eq!(attr.validate(&tags).is_ok(), accepted, "{key:?}: {value:?}");
        }
    }
}

#[test]
fn map_length_iam_role_tags_validate_through_core_and_protocol() {
    check_tag_count(role::iam_role_config().schema, 0, 50);
}

#[test]
fn map_length_acm_certificate_tags_validate_through_core_and_protocol() {
    check_tag_count(certificate::acm_certificate_config().schema, 1, 50);
}

#[test]
fn map_length_iam_and_acm_tags_use_the_core_resource_validation_path() {
    let snapshot = carina_core::provider::loaded_provider_snapshot_from_factories(
        Arc::new(vec![Box::new(carina_provider_aws::AwsProviderFactory)]),
        SchemaRegistry::new(),
        carina_core::EnvironmentSnapshot::empty(),
    );
    let context = snapshot.provider_context();
    for (resource, attributes, maximum) in [
        (
            "iam.Role",
            r"role_name = 'map-length'
            assume_role_policy_document = {
                version = aws.iam.PolicyDocument.Version.2012_10_17
                statement = [{
                    effect = aws.iam.PolicyDocument.Statement.Effect.allow
                    principal = { service = 'lambda.amazonaws.com' }
                    action = 'sts:AssumeRole'
                }]
            }",
            50,
        ),
        ("acm.Certificate", "domain_name = 'example.com'", 50),
    ] {
        for count in [maximum, maximum + 1] {
            let entries = (0..count)
                .map(|index| format!("key{index} = 'value'"))
                .collect::<Vec<_>>()
                .join("\n");
            let source = format!("aws.{resource} {{ {attributes}\ntags = {{ {entries} }} }}");
            let parsed =
                carina_core::parser::parse_file(&source, Path::new("map-length.crn"), context)
                    .expect("parse generated schema acceptance fixture");
            let result = carina_core::validation::validate_resources(
                &parsed,
                snapshot.schema_registry(),
                &HashSet::from(["aws".to_string()]),
                context,
            );
            assert_eq!(
                result.is_ok(),
                count <= maximum,
                "{resource}, {count} tags: {result:?}"
            );
        }
    }
}

#[test]
fn map_length_logs_tags_use_the_validate_and_lsp_resource_validation_path() {
    let snapshot = carina_core::provider::loaded_provider_snapshot_from_factories(
        Arc::new(vec![Box::new(carina_provider_aws::AwsProviderFactory)]),
        SchemaRegistry::new(),
        carina_core::EnvironmentSnapshot::empty(),
    );
    let context = snapshot.provider_context();
    for (entries, accepted) in [
        ("Name = 'valid value'".to_string(), true),
        ("'日本語' = '値'".to_string(), true),
        (
            (0..51)
                .map(|i| format!("key{i} = 'value'"))
                .collect::<Vec<_>>()
                .join("\n"),
            false,
        ),
        ("'' = 'value'".to_string(), false),
        (format!("'{}' = 'value'", "k".repeat(129)), false),
        ("'bad!key' = 'value'".to_string(), false),
        (format!("Name = '{}'", "v".repeat(257)), false),
    ] {
        let source = format!(
            "aws.logs.LogGroup {{ log_group_name = 'map-length'\ntags = {{ {entries} }} }}"
        );
        let parsed = carina_core::parser::parse_file(&source, Path::new("map-length.crn"), context)
            .expect("parse tags fixture");
        let result = carina_core::validation::validate_resources(
            &parsed,
            snapshot.schema_registry(),
            &HashSet::from(["aws".to_string()]),
            context,
        );
        assert_eq!(result.is_ok(), accepted, "{source}\n{result:?}");
    }
}
