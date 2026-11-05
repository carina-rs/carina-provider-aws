use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use carina_core::protocol_convert::{protocol_to_resource_schema, resource_schema_to_protocol};
use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{
    AttributeSchema, AttributeType, RawShape, ResourceSchema, SchemaRegistry,
    ValidatedResourceSchema,
};
use carina_provider_aws::schemas::generated::sqs::queue::sqs_queue_config;

fn sqs_schemas() -> [ValidatedResourceSchema; 2] {
    let native = sqs_queue_config()
        .schema
        .validate()
        .expect("Queue schema admission");
    let wire = resource_schema_to_protocol(&native).expect("encode Queue schema");
    let decoded = protocol_to_resource_schema(&wire).expect("decode Queue schema");
    [native, decoded]
}

fn string(value: &str) -> Value {
    Value::Concrete(ConcreteValue::String(value.to_string().into()))
}

fn enum_identifier(value: &str) -> Value {
    Value::Concrete(ConcreteValue::enum_identifier(value))
}

macro_rules! bound_test {
    ($test:ident, $path:literal, $min:expr, $max:expr) => {
        #[test]
        fn $test() {
            for schema in sqs_schemas() {
                let path: Vec<String> = $path.split('.').map(str::to_string).collect();
                let field = schema.effective_type_at_attribute_path(&path).unwrap();
                let RawShape::Int {
                    identity, range, ..
                } = field.as_type().raw_shape()
                else {
                    panic!("{} must keep its Int carrier", $path);
                };
                assert!(identity.is_none());
                assert_eq!(range, Some((Some($min), Some($max))), "{}", $path);
                for (value, valid) in [
                    ($min - 1, false),
                    ($min, true),
                    ($max, true),
                    ($max + 1, false),
                ] {
                    let input: Value = Value::Concrete(ConcreteValue::Int(value));
                    assert_eq!(field.validate(&input).is_ok(), valid, "{} = {value}", $path,);
                }
                for invalid in [
                    ConcreteValue::String($min.to_string().into()),
                    ConcreteValue::Float($min as f64),
                    ConcreteValue::Bool(true),
                ] {
                    let input: Value = Value::Concrete(invalid);
                    assert!(field.validate(&input).is_err());
                }
            }
        }
    };
}

bound_test!(
    sqs_visibility_timeout_bounds,
    "visibility_timeout",
    0_i64,
    43200_i64
);
bound_test!(
    sqs_message_retention_period_bounds,
    "message_retention_period",
    60_i64,
    1209600_i64
);
bound_test!(sqs_delay_seconds_bounds, "delay_seconds", 0_i64, 900_i64);
bound_test!(
    sqs_maximum_message_size_bounds,
    "maximum_message_size",
    1024_i64,
    1048576_i64
);
bound_test!(
    sqs_receive_message_wait_time_seconds_bounds,
    "receive_message_wait_time_seconds",
    0_i64,
    20_i64
);
bound_test!(
    sqs_kms_data_key_reuse_period_seconds_bounds,
    "kms_data_key_reuse_period_seconds",
    60_i64,
    86400_i64
);
bound_test!(
    sqs_max_receive_count_bounds,
    "redrive_policy.max_receive_count",
    1_i64,
    1000_i64
);

#[test]
fn sqs_queue_name_is_anonymous_and_counts_the_fifo_suffix_within_eighty_characters() {
    for schema in sqs_schemas() {
        let name = schema.effective_attribute("queue_name").unwrap();
        let RawShape::String {
            identity,
            pattern,
            length,
            ..
        } = name.field_type().raw_shape()
        else {
            panic!("queue_name must remain String");
        };
        assert!(identity.is_none());
        assert_eq!(
            pattern,
            Some(r"^(?:[A-Za-z0-9_-]{1,80}|[A-Za-z0-9_-]{1,75}\.fifo)\z")
        );
        assert_eq!(length, Some((Some(1), Some(80))));
        for valid in [
            "a".to_string(),
            "Az09_-".to_string(),
            "a".repeat(80),
            "a.fifo".to_string(),
            format!("{}.fifo", "a".repeat(75)),
        ] {
            assert!(name.validate(&string(&valid)).is_ok(), "{valid:?}");
        }
        for invalid in [
            "".to_string(),
            "a".repeat(81),
            format!("{}.fifo", "a".repeat(76)),
            ".fifo".to_string(),
            "a.FIFO".to_string(),
            "a.b".to_string(),
            "a.fifo.fifo".to_string(),
            "a/b".to_string(),
            "a:b".to_string(),
            "a b".to_string(),
            "a\n".to_string(),
            "日本語".to_string(),
        ] {
            assert!(name.validate(&string(&invalid)).is_err(), "{invalid:?}");
        }
    }
}

#[test]
fn sqs_closed_enums_have_snake_case_dsl_values_and_exact_aws_wire_values() {
    for schema in sqs_schemas() {
        for (field, kind, pairs) in [
            (
                "deduplication_scope",
                "DeduplicationScope",
                vec![("message_group", "messageGroup"), ("queue", "queue")],
            ),
            (
                "fifo_throughput_limit",
                "FifoThroughputLimit",
                vec![
                    ("per_queue", "perQueue"),
                    ("per_message_group_id", "perMessageGroupId"),
                ],
            ),
        ] {
            let attr = schema.effective_attribute(field).unwrap();
            let (identity, values, _, _, spellings) =
                attr.field_type().enum_parts().expect("closed enum");
            assert_eq!(identity.to_string(), format!("aws.sqs.Queue.{kind}"));
            assert_eq!(
                values.unwrap(),
                pairs
                    .iter()
                    .map(|(_, api)| api.to_string())
                    .collect::<Vec<_>>()
            );
            for (dsl, api) in pairs {
                assert_eq!(spellings.api_for(dsl), api);
                assert_eq!(spellings.dsl_for(api), dsl);
                assert!(
                    attr.validate(&enum_identifier(&format!("{identity}.{dsl}")))
                        .is_ok(),
                    "{field} = {dsl}"
                );
                if dsl != api {
                    assert!(
                        attr.validate(&enum_identifier(&format!("{identity}.{api}")))
                            .is_err(),
                        "old wire spelling must require migration: {api}"
                    );
                }
            }
            for invalid in ["", "unknown", "MESSAGE_GROUP", "per_message_group"] {
                assert!(
                    attr.validate(&enum_identifier(&format!("{identity}.{invalid}")))
                        .is_err(),
                    "{field} = {invalid}"
                );
            }
        }
    }
}

#[test]
fn sqs_queue_arn_identity_and_grammar_apply_to_all_three_paths() {
    let catalog = carina_aws_types::sqs_queue_arn();
    let RawShape::String {
        identity: Some(expected_identity),
        pattern: expected_pattern,
        length: expected_length,
        ..
    } = catalog.raw_shape()
    else {
        panic!("catalog queue ARN must have a named String identity");
    };
    assert_eq!(expected_identity.to_string(), "aws.sqs.Queue.Arn");
    for schema in sqs_schemas() {
        for field in ["redrive_policy", "redrive_allow_policy"] {
            assert!(
                matches!(
                    schema
                        .effective_attribute(field)
                        .unwrap()
                        .field_type()
                        .raw_shape(),
                    RawShape::Struct { .. }
                ),
                "{field} must be Struct"
            );
        }
        // Check every String refinement component against the catalog, including
        // after protocol decoding. This covers the same grammar at every leaf;
        // run the accept/reject matrix once on queue_arn below.
        for (path, list) in [
            ("queue_arn", false),
            ("redrive_policy.dead_letter_target_arn", false),
            ("redrive_allow_policy.source_queue_arns", true),
        ] {
            let segments: Vec<String> = path.split('.').map(str::to_string).collect();
            let leaf = schema.effective_type_at_attribute_path(&segments).unwrap();
            let leaf = if list {
                leaf.list_element()
                    .expect("source_queue_arns must be a list")
            } else {
                leaf
            };
            let RawShape::String {
                identity,
                pattern,
                length,
                validate,
                to_dsl,
            } = leaf.as_type().raw_shape()
            else {
                panic!("{path} must be a String ARN leaf");
            };
            assert_eq!(identity, Some(expected_identity), "{path}");
            assert_eq!(pattern, expected_pattern, "{path}");
            assert_eq!(length, expected_length, "{path}");
            assert!(
                validate.is_none(),
                "{path} must use portable metadata validation"
            );
            assert!(to_dsl.is_none(), "{path} must preserve ARN spelling");
        }
        let queue_arn = schema.effective_attribute("queue_arn").unwrap();
        for (arn, valid) in [
            ("arn:aws:sqs:us-east-1:123456789012:a".to_string(), true),
            (
                "arn:aws-cn:sqs:cn-north-1:123456789012:Az09_-".to_string(),
                true,
            ),
            (
                "arn:aws-us-gov:sqs:us-gov-west-1:123456789012:a.fifo".to_string(),
                true,
            ),
            (
                format!("arn:aws:sqs:ap-northeast-1:123456789012:{}", "a".repeat(80)),
                true,
            ),
            (
                format!(
                    "arn:aws:sqs:ap-northeast-1:123456789012:{}.fifo",
                    "a".repeat(75)
                ),
                true,
            ),
            (
                format!("arn:aws:sqs:ap-northeast-1:123456789012:{}", "a".repeat(81)),
                false,
            ),
            (
                format!(
                    "arn:aws:sqs:ap-northeast-1:123456789012:{}.fifo",
                    "a".repeat(76)
                ),
                false,
            ),
            ("arn:aws:sns:us-east-1:123456789012:a".to_string(), false),
            (
                "arn:invalid:sqs:us-east-1:123456789012:a".to_string(),
                false,
            ),
            ("arn:aws:sqs::123456789012:a".to_string(), false),
            ("arn:aws:sqs:*:123456789012:a".to_string(), false),
            ("arn:aws:sqs:us-east-1:12345678901:a".to_string(), false),
            ("arn:aws:sqs:us-east-1:1234567890123:a".to_string(), false),
            (
                "arn:aws:sqs:us-east-1:１２３４５６７８９０１２:a".to_string(),
                false,
            ),
            ("arn:aws:sqs:us-east-1:123456789012:".to_string(), false),
            (
                "arn:aws:sqs:us-east-1:123456789012:queue/a".to_string(),
                false,
            ),
            ("arn:aws:sqs:us-east-1:123456789012:a*".to_string(), false),
            (
                "arn:aws:sqs:us-east-1:123456789012:a.fifo\n".to_string(),
                false,
            ),
        ] {
            assert_eq!(
                queue_arn.validate(&string(&arn)).is_ok(),
                valid,
                "queue_arn = {arn:?}"
            );
        }
    }
}

#[test]
fn sqs_arn_sinks_reject_plain_string_references_and_accept_explicit_annotations() {
    let mut registry = SchemaRegistry::new();
    registry.insert(
        "probe",
        ResourceSchema::new("sqs.Source")
            .attribute(
                AttributeSchema::provider_populated("value", AttributeType::string()).build(),
            )
            .validate()
            .unwrap(),
    );
    let snapshot = carina_core::provider::loaded_provider_snapshot_from_factories(
        Arc::new(vec![Box::new(carina_provider_aws::AwsProviderFactory)]),
        registry,
        carina_core::EnvironmentSnapshot::empty(),
    );
    let context = snapshot.provider_context();
    for (declaration, expression, valid) in [
        ("", "source.value", false),
        ("arguments { target: String }", "target", false),
        ("arguments { target: aws.sqs.Queue.Arn }", "target", true),
        ("", "queue.queue_arn", true),
    ] {
        for sink in [
            format!(
                "redrive_policy = {{ dead_letter_target_arn = {expression} max_receive_count = 1 }}"
            ),
            format!(
                "redrive_allow_policy = {{ redrive_permission = aws.sqs.Queue.RedriveAllowPolicy.RedrivePermission.by_queue source_queue_arns = [{expression}] }}"
            ),
        ] {
            let source = format!(
                "let source = probe.sqs.Source {{}}\nlet queue = aws.sqs.Queue {{ queue_name = 'source' }}\n{declaration}\naws.sqs.Queue {{ queue_name = 'sink' {sink} }}"
            );
            let parsed = carina_core::parser::parse_file(
                &source,
                Path::new("sqs-arn-reference.crn"),
                context,
            )
            .expect("parse ARN reference fixture");
            carina_core::validation::validate_resources(
                &parsed,
                snapshot.schema_registry(),
                &HashSet::from(["aws".to_string(), "probe".to_string()]),
                context,
            )
            .expect("value validation defers references");
            let bindings = carina_core::binding_index::BindingIndex::from_parsed(
                &parsed,
                snapshot.schema_registry(),
                context,
            );
            let configuration = carina_core::config_loader::load_buffer_configuration(
                &parsed,
                snapshot.schema_registry(),
                context,
            );
            let result = carina_core::validation::validate_resource_ref_types(
                configuration.judged(),
                &HashSet::new(),
                &bindings,
            );
            if valid {
                assert!(result.is_ok(), "{source}: {result:?}");
            } else {
                let error = result
                    .expect_err("carina#3798: named ARN sinks require typed references")
                    .to_string();
                assert!(
                    error.contains("expected aws.sqs.Queue.Arn, got String"),
                    "{error}"
                );
            }
        }
    }
}

#[test]
fn sqs_examples_and_acceptance_fixtures_validate() {
    let snapshot = carina_core::provider::loaded_provider_snapshot_from_factories(
        Arc::new(vec![Box::new(carina_provider_aws::AwsProviderFactory)]),
        SchemaRegistry::new(),
        carina_core::EnvironmentSnapshot::empty(),
    );
    let context = snapshot.provider_context();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    for fixture in [
        "examples/sqs_queue/main.crn",
        "acceptance-tests/sqs_queue/basic.crn",
        "acceptance-tests/sqs_queue/fifo.crn",
    ] {
        let path = root.join(fixture);
        let source = std::fs::read_to_string(&path).unwrap();
        let parsed = carina_core::parser::parse_file(&source, &path, context)
            .unwrap_or_else(|error| panic!("{fixture}: {error}"));
        carina_core::validation::validate_resources(
            &parsed,
            snapshot.schema_registry(),
            &HashSet::from(["aws".to_string()]),
            context,
        )
        .unwrap_or_else(|error| panic!("{fixture}: {error}"));
    }
}
