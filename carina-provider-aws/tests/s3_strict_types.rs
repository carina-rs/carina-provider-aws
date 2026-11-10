use carina_core::protocol_convert::{protocol_to_resource_schema, resource_schema_to_protocol};
use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{EffectiveType, RawShape, ValidatedResourceSchema};

fn schemas(resource: &str) -> Vec<ValidatedResourceSchema> {
    carina_provider_aws::schemas::all_schemas()
        .into_iter()
        .filter(|schema| schema.parts().resource_type == format!("s3.{resource}"))
        .flat_map(|schema| {
            let wire = protocol_to_resource_schema(&resource_schema_to_protocol(&schema).unwrap())
                .unwrap();
            [schema, wire]
        })
        .collect()
}

fn field<'a>(schema: &'a ValidatedResourceSchema, path: &str) -> EffectiveType<'a> {
    let names = path
        .split('.')
        .map(|name| name.trim_end_matches("[]").to_string())
        .collect::<Vec<_>>();
    let ty = schema.effective_node_at_path(&names).unwrap().value_type();
    if path.ends_with("[]") {
        ty.list_element().unwrap()
    } else {
        ty
    }
}

fn string(value: &str) -> Value {
    Value::Concrete(ConcreteValue::String(value.into()))
}

fn int(value: i64) -> Value {
    Value::Concrete(ConcreteValue::Int(value))
}

fn list(values: Vec<Value>) -> Value {
    Value::Concrete(ConcreteValue::List(values))
}

fn object(values: Vec<(&str, Value)>) -> Value {
    Value::Concrete(ConcreteValue::Map(
        values
            .into_iter()
            .map(|(name, value)| (name.into(), value))
            .collect(),
    ))
}

fn identifier(value: &str) -> Value {
    Value::Concrete(ConcreteValue::enum_identifier(value))
}

fn cors_rule(methods: Vec<Value>, origins: Vec<Value>) -> Value {
    object(vec![
        ("allowed_methods", list(methods)),
        ("allowed_origins", list(origins)),
    ])
}

fn nonempty_list(resource: &str, path: &str, valid_element: Value) {
    for schema in schemas(resource) {
        let ty = field(&schema, path);
        assert!(
            ty.validate(&list(vec![])).is_err(),
            "{resource}.{path} must reject an empty list"
        );
        assert!(
            ty.validate(&list(vec![valid_element.clone()])).is_ok(),
            "{resource}.{path} must allow one element"
        );
        let RawShape::List { constraints, .. } = ty.as_type().raw_shape() else {
            panic!("list {path}");
        };
        assert_eq!(
            constraints.length().unwrap().0,
            Some(1),
            "{resource}.{path}"
        );
    }
}

#[test]
fn s3_cors_allowed_origins_rejects_empty_list() {
    // CORSRule.AllowedOrigins is required and contains one or more origins.
    // https://docs.aws.amazon.com/AmazonS3/latest/API/API_CORSRule.html
    for schema in schemas("BucketCorsConfiguration") {
        let ty = field(&schema, "cors_rules");
        assert!(
            ty.validate(&list(vec![cors_rule(vec![identifier("get")], vec![])]))
                .is_err()
        );
        assert!(
            ty.validate(&list(vec![cors_rule(
                vec![identifier("get")],
                vec![string("*")]
            )]))
            .is_ok()
        );
    }
}

#[test]
fn s3_cors_allowed_methods_rejects_empty_list() {
    // https://docs.aws.amazon.com/AmazonS3/latest/API/API_CORSRule.html
    for schema in schemas("BucketCorsConfiguration") {
        let ty = field(&schema, "cors_rules");
        assert!(
            ty.validate(&list(vec![cors_rule(vec![], vec![string("*")])]))
                .is_err()
        );
        assert!(
            ty.validate(&list(vec![cors_rule(
                vec![identifier("get")],
                vec![string("*")]
            )]))
            .is_ok()
        );
    }
}

#[test]
fn s3_cors_configuration_rules_rejects_empty_list() {
    // https://docs.aws.amazon.com/AmazonS3/latest/API/API_CORSConfiguration.html
    nonempty_list(
        "BucketCorsConfiguration",
        "cors_rules",
        cors_rule(vec![identifier("get")], vec![string("*")]),
    );
}

#[test]
fn s3_topic_notification_events_rejects_empty_list() {
    // https://docs.aws.amazon.com/AmazonS3/latest/API/API_TopicConfiguration.html
    nonempty_list(
        "BucketNotificationConfiguration",
        "topic_configurations[].events",
        identifier("s3_object_created"),
    );
}

#[test]
fn s3_queue_notification_events_rejects_empty_list() {
    // https://docs.aws.amazon.com/AmazonS3/latest/API/API_QueueConfiguration.html
    nonempty_list(
        "BucketNotificationConfiguration",
        "queue_configurations[].events",
        identifier("s3_object_created"),
    );
}

#[test]
fn s3_lambda_notification_events_rejects_empty_list() {
    // https://docs.aws.amazon.com/AmazonS3/latest/API/API_LambdaFunctionConfiguration.html
    nonempty_list(
        "BucketNotificationConfiguration",
        "lambda_function_configurations[].events",
        identifier("s3_object_created"),
    );
}

#[test]
fn s3_encryption_rules_rejects_empty_list() {
    // https://docs.aws.amazon.com/AmazonS3/latest/API/API_ServerSideEncryptionConfiguration.html
    nonempty_list(
        "BucketServerSideEncryptionConfiguration",
        "rules",
        object(vec![(
            "apply_server_side_encryption_by_default",
            object(vec![("sse_algorithm", identifier("aes256"))]),
        )]),
    );
}

#[test]
fn s3_lifecycle_and_replication_required_rules_keep_their_existing_minima() {
    nonempty_list(
        "BucketLifecycleConfiguration",
        "rules",
        object(vec![("status", identifier("enabled"))]),
    );
    nonempty_list(
        "BucketReplicationConfiguration",
        "rules",
        object(vec![
            ("status", identifier("enabled")),
            (
                "destination",
                object(vec![("bucket", string("arn:aws:s3:::destination"))]),
            ),
        ]),
    );
}

#[test]
fn s3_optional_lists_keep_accepting_empty_lists() {
    // In particular, both LifecycleRuleAndOperator.Tags and
    // ReplicationRuleAndOperator.Tags are optional in Smithy and the API docs.
    for (resource, path) in [
        ("BucketCorsConfiguration", "cors_rules[].allowed_headers"),
        ("BucketCorsConfiguration", "cors_rules[].expose_headers"),
        ("BucketLifecycleConfiguration", "rules[].filter.and.tags"),
        ("BucketLifecycleConfiguration", "rules[].transitions"),
        (
            "BucketLifecycleConfiguration",
            "rules[].noncurrent_version_transitions",
        ),
        ("BucketReplicationConfiguration", "rules[].filter.and.tags"),
        ("BucketNotificationConfiguration", "topic_configurations"),
        ("BucketNotificationConfiguration", "queue_configurations"),
        (
            "BucketNotificationConfiguration",
            "lambda_function_configurations",
        ),
        (
            "BucketNotificationConfiguration",
            "topic_configurations[].filter.filter_rules",
        ),
        (
            "BucketNotificationConfiguration",
            "queue_configurations[].filter.filter_rules",
        ),
        (
            "BucketNotificationConfiguration",
            "lambda_function_configurations[].filter.filter_rules",
        ),
    ] {
        for schema in schemas(resource) {
            assert!(
                field(&schema, path).validate(&list(vec![])).is_ok(),
                "{resource}.{path}"
            );
        }
    }
}

fn strings(resource: &str, path: &str, valid: &[&str], invalid: &[&str]) {
    let schemas = schemas(resource);
    assert!(!schemas.is_empty());
    for schema in schemas {
        let ty = field(&schema, path);
        for (values, expected) in [(valid, true), (invalid, false)] {
            for value in values {
                assert_eq!(
                    ty.validate(&string(value)).is_ok(),
                    expected,
                    "{resource}.{path} = {value:?}"
                );
            }
        }
        assert!(ty.validate(&int(1)).is_err());
    }
}

#[test]
fn s3_rule_ids_have_only_the_documented_255_character_cap() {
    // API_CORSRule, API_LifecycleRule, API_ReplicationRule: characters, not bytes.
    for (resource, path) in [
        ("BucketCorsConfiguration", "cors_rules[].id"),
        ("BucketLifecycleConfiguration", "rules[].id"),
        ("BucketReplicationConfiguration", "rules[].id"),
    ] {
        strings(
            resource,
            path,
            &["", "rule", &"界".repeat(255)],
            &[&"x".repeat(256), &"界".repeat(256)],
        );
    }
}

#[test]
fn s3_cors_http_methods_use_snake_case_dsl_and_canonical_wire_values() {
    // Match sqs_closed_enums_have_snake_case_dsl_values_and_exact_aws_wire_values.
    enum_values(
        "BucketCorsConfiguration",
        "cors_rules[].allowed_methods[]",
        "HttpMethod",
        &[
            ("get", "GET"),
            ("put", "PUT"),
            ("head", "HEAD"),
            ("post", "POST"),
            ("delete", "DELETE"),
        ],
    );
}

#[test]
fn s3_cors_origins_and_headers_allow_at_most_one_wildcard() {
    // ManageCorsUsing: no extra URL/header alphabet or length restriction.
    for path in [
        "cors_rules[].allowed_origins[]",
        "cors_rules[].allowed_headers[]",
    ] {
        strings(
            "BucketCorsConfiguration",
            path,
            &[
                "*",
                "http://*.example.com",
                "https://example.com:8443",
                "x-amz-*",
                "Content-Type",
            ],
            &["**", "https://*.*.example.com", "x-*-*"],
        );
    }
}

#[test]
fn s3_lifecycle_expiration_rejects_smithy_integer_overflow() {
    for schema in schemas("BucketLifecycleConfiguration") {
        let days = field(&schema, "rules[].expiration.days");
        for value in [2_147_483_648, 4_294_967_297, i64::MAX] {
            assert!(
                days.validate(&int(value)).is_err(),
                "expiration.days = {value} must not reach an i32 SDK builder"
            );
        }
        assert!(days.validate(&int(i64::from(i32::MAX))).is_ok());
    }
}

#[test]
fn s3_smithy_integer_members_preserve_width_while_long_filters_remain_i64() {
    for (resource, path) in [
        ("BucketCorsConfiguration", "cors_rules[].max_age_seconds"),
        ("BucketReplicationConfiguration", "rules[].priority"),
        ("BucketLifecycleConfiguration", "rules[].expiration.days"),
        ("BucketLifecycleConfiguration", "rules[].transitions[].days"),
        (
            "BucketLifecycleConfiguration",
            "rules[].noncurrent_version_expiration.noncurrent_days",
        ),
        (
            "BucketLifecycleConfiguration",
            "rules[].noncurrent_version_expiration.newer_noncurrent_versions",
        ),
        (
            "BucketLifecycleConfiguration",
            "rules[].noncurrent_version_transitions[].noncurrent_days",
        ),
        (
            "BucketLifecycleConfiguration",
            "rules[].noncurrent_version_transitions[].newer_noncurrent_versions",
        ),
        (
            "BucketLifecycleConfiguration",
            "rules[].abort_incomplete_multipart_upload.days_after_initiation",
        ),
    ] {
        for schema in schemas(resource) {
            let ty = field(&schema, path);
            for value in [
                i64::from(i32::MIN) - 1,
                i64::from(i32::MAX) + 1,
                4_294_967_297,
            ] {
                assert!(
                    ty.validate(&int(value)).is_err(),
                    "{resource}.{path} = {value}"
                );
            }
        }
    }
    for path in [
        "rules[].filter.object_size_greater_than",
        "rules[].filter.object_size_less_than",
        "rules[].filter.and.object_size_greater_than",
        "rules[].filter.and.object_size_less_than",
    ] {
        for schema in schemas("BucketLifecycleConfiguration") {
            let ty = field(&schema, path);
            assert!(ty.validate(&int(i64::MAX)).is_ok(), "Long {path}");
        }
    }
}

#[test]
fn s3_abort_multipart_requires_positive_days_after_initiation() {
    // The lifecycle XML table specifies Positive Integer; AWS rejects zero.
    // https://docs.aws.amazon.com/AmazonS3/latest/developerguide/v1-rel-RESTBucketPUTlifecycle.html
    for schema in schemas("BucketLifecycleConfiguration") {
        let days = field(
            &schema,
            "rules[].abort_incomplete_multipart_upload.days_after_initiation",
        );
        for value in [0, -1, i64::from(i32::MIN)] {
            assert!(
                days.validate(&int(value)).is_err(),
                "days_after_initiation = {value}"
            );
        }
        for value in [1, i64::from(i32::MAX)] {
            assert!(
                days.validate(&int(value)).is_ok(),
                "days_after_initiation = {value}"
            );
        }
        assert!(days.validate(&int(i64::from(i32::MAX) + 1)).is_err());
    }
}

#[test]
fn s3_noncurrent_version_transition_requires_nonnegative_days() {
    // The lifecycle XML table specifies Nonnegative Integer; AWS accepts zero.
    // https://docs.aws.amazon.com/AmazonS3/latest/developerguide/v1-rel-RESTBucketPUTlifecycle.html
    for schema in schemas("BucketLifecycleConfiguration") {
        let days = field(
            &schema,
            "rules[].noncurrent_version_transitions[].noncurrent_days",
        );
        for value in [-1, i64::from(i32::MIN)] {
            assert!(
                days.validate(&int(value)).is_err(),
                "transition noncurrent_days = {value}"
            );
        }
        for value in [0, 1, i64::from(i32::MAX)] {
            assert!(
                days.validate(&int(value)).is_ok(),
                "transition noncurrent_days = {value}"
            );
        }
        assert!(days.validate(&int(i64::from(i32::MAX) + 1)).is_err());
    }
}

#[test]
fn s3_lifecycle_numeric_bounds_distinguish_expiration_and_transition() {
    for (path, minimum, maximum) in [
        ("rules[].expiration.days", 1, i64::from(i32::MAX)),
        ("rules[].transitions[].days", 0, i64::from(i32::MAX)),
        (
            "rules[].noncurrent_version_expiration.noncurrent_days",
            1,
            i64::from(i32::MAX),
        ),
        (
            "rules[].noncurrent_version_expiration.newer_noncurrent_versions",
            1,
            100,
        ),
        (
            "rules[].noncurrent_version_transitions[].newer_noncurrent_versions",
            1,
            100,
        ),
    ] {
        for schema in schemas("BucketLifecycleConfiguration") {
            let ty = field(&schema, path);
            for value in [minimum, maximum] {
                assert!(ty.validate(&int(value)).is_ok(), "{path} = {value}");
            }
            assert!(ty.validate(&int(minimum - 1)).is_err(), "{path}");
            if maximum < i64::MAX {
                assert!(ty.validate(&int(maximum + 1)).is_err(), "{path}");
            }
            assert!(ty.validate(&string("1")).is_err());
        }
    }
}

#[test]
fn s3_lifecycle_dates_require_real_calendar_dates_at_midnight_utc() {
    for path in ["rules[].expiration.date", "rules[].transitions[].date"] {
        strings(
            "BucketLifecycleConfiguration",
            path,
            &["2028-02-29T00:00:00Z", "2026-01-01T00:00:00Z"],
            &[
                "2027-02-29T00:00:00Z",
                "2030-02-30T00:00:00Z",
                "2026-01-01T00:00:01Z",
                "2026-01-01T12:00:00Z",
                "2026-01-01T00:00:00+00:00",
                "2026-01-01T00:00:00.000Z",
            ],
        );
    }
}

fn enum_values(resource: &str, path: &str, kind: &str, pairs: &[(&str, &str)]) {
    for schema in schemas(resource) {
        let ty = field(&schema, path);
        let (identity, values, _, _, spellings) = ty.as_type().enum_parts().expect("closed enum");
        assert_eq!(identity.to_string(), format!("aws.s3.{resource}.{kind}"));
        if resource == "BucketCorsConfiguration" {
            assert_eq!(
                values.unwrap(),
                pairs
                    .iter()
                    .map(|(_, wire)| wire.to_string())
                    .collect::<Vec<_>>()
            );
        }
        for (dsl, wire) in pairs {
            assert_eq!(spellings.api_for(dsl), *wire);
            assert_eq!(spellings.dsl_for(wire), *dsl);
            for alias in [dsl.to_string(), format!("{identity}.{dsl}")] {
                let value: Value = Value::Concrete(ConcreteValue::enum_identifier(&alias));
                assert!(ty.validate(&value).is_ok(), "{path} = {alias}");
            }
            // Match acm_key_algorithm_allows_only_request_certificate_algorithms_and_aliases:
            // SDK values are checked after state canonicalization, not as authored DSL.
            let canonical = ty.canonicalize_state(string(wire)).into_present().unwrap();
            assert!(ty.validate(&canonical).is_ok(), "SDK value {wire}");
        }
        for invalid in ["", "unknown", "options", "s3_object_created_unknown"] {
            let value: Value = Value::Concrete(ConcreteValue::enum_identifier(format!(
                "{identity}.{invalid}"
            )));
            assert!(ty.validate(&value).is_err(), "{path} = {invalid}");
        }
        assert!(ty.validate(&int(1)).is_err());
    }
}

#[test]
fn s3_notification_events_match_sdk_with_punctuation_aliases() {
    for path in [
        "topic_configurations[].events[]",
        "queue_configurations[].events[]",
        "lambda_function_configurations[].events[]",
    ] {
        for schema in schemas("BucketNotificationConfiguration") {
            let ty = field(&schema, path);
            let (_, values, _, _, _) = ty.as_type().enum_parts().expect("closed enum");
            let actual = values
                .unwrap()
                .iter()
                .map(String::as_str)
                .collect::<std::collections::BTreeSet<_>>();
            let expected = carina_aws_types::sdk::s3::types::Event::values()
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(actual, expected, "{path}");
            for wire in carina_aws_types::sdk::s3::types::Event::values() {
                let canonical = ty.canonicalize_state(string(wire)).into_present().unwrap();
                assert!(ty.validate(&canonical).is_ok(), "SDK event {wire}");
            }
        }
        enum_values(
            "BucketNotificationConfiguration",
            path,
            "Event",
            &[
                ("s3_object_created", "s3:ObjectCreated:*"),
                (
                    "s3_object_created_complete_multipart_upload",
                    "s3:ObjectCreated:CompleteMultipartUpload",
                ),
                ("s3_object_tagging_put", "s3:ObjectTagging:Put"),
            ],
        );
    }
}

#[test]
fn s3_website_index_suffix_is_nonempty_without_slashes() {
    strings(
        "BucketWebsiteConfiguration",
        "index_document.suffix",
        &["index.html", "index 文書.html", "index\n.html"],
        &["", "/index.html", "dir/index.html"],
    );
}

#[test]
fn s3_bucket_domain_outputs_use_the_shared_route53_dns_contract() {
    let shared = carina_aws_types::route53_dns_name();
    let RawShape::String {
        identity_layer:
            carina_core::schema::StringIdentityParts {
                pattern: shared_pattern,
                length: shared_length,
                ..
            },
        ..
    } = shared.raw_shape()
    else {
        panic!("shared DNS String");
    };
    for path in ["bucket_domain_name", "bucket_regional_domain_name"] {
        for schema in schemas("Bucket") {
            let ty = field(&schema, path);
            let RawShape::String {
                identity_layer:
                    carina_core::schema::StringIdentityParts {
                        identity,
                        pattern,
                        length,
                        ..
                    },
                ..
            } = ty.as_type().raw_shape()
            else {
                panic!("DNS String");
            };
            assert!(identity.is_none());
            assert_eq!(pattern, shared_pattern);
            assert_eq!(length, shared_length);
        }
    }
    // The shared DNS contract caps labels at 63 bytes. Synthesized domains
    // for pre-2018 bucket labels longer than 63 bytes need separate treatment;
    // this test does not silently give the S3 outputs a different DNS contract.
}

#[test]
fn s3_rule_list_cardinality_survives_protocol_round_trips() {
    for (resource, path, minimum, maximum) in [
        ("BucketCorsConfiguration", "cors_rules", 1, 100),
        ("BucketLifecycleConfiguration", "rules", 1, 1000),
        ("BucketReplicationConfiguration", "rules", 1, 1000),
    ] {
        for schema in schemas(resource) {
            let ty = field(&schema, path);
            let RawShape::List { constraints, .. } = ty.as_type().raw_shape() else {
                panic!("{resource}.{path}")
            };
            assert_eq!(
                constraints.length(),
                Some((Some(minimum), Some(maximum))),
                "{resource}.{path}"
            );
        }
    }
}
