use carina_core::provider::ProviderNormalizer;
use carina_core::resource::{ConcreteValue, DeferredValue, ProviderResource, Resource, Value};
use carina_provider_aws::AwsNormalizer;
use indexmap::IndexMap;

fn string(value: &str) -> Value {
    Value::Concrete(ConcreteValue::String(value.into()))
}
fn int(value: i64) -> Value {
    Value::Concrete(ConcreteValue::Int(value))
}
fn bool_(value: bool) -> Value {
    Value::Concrete(ConcreteValue::Bool(value))
}
fn map(values: &[(&str, Value)]) -> Value {
    Value::Concrete(ConcreteValue::Map(
        values
            .iter()
            .map(|(key, value)| (key.to_string(), value.clone()))
            .collect::<IndexMap<_, _>>(),
    ))
}
fn list(values: Vec<Value>) -> Value {
    Value::Concrete(ConcreteValue::List(values))
}

async fn normalize(resource: &str, attrs: &[(&str, Value)]) -> Result<ProviderResource, String> {
    let mut resource = Resource::with_provider("aws", format!("s3.{resource}"), "s3-rules", None);
    for (key, value) in attrs {
        resource.set_attr(*key, value.clone());
    }
    let mut resources = vec![resource.into()];
    AwsNormalizer
        .normalize_desired(&mut resources)
        .await
        .map_err(|error| error.to_string())?;
    Ok(resources.remove(0))
}

#[tokio::test]
async fn s3_lifecycle_expiration_and_transition_choose_one_clock() {
    // API_LifecycleExpiration; API_Transition; intro-lifecycle-rules.
    for action in ["expiration", "transitions"] {
        let value = map(&[("days", int(1)), ("date", string("2030-01-01T00:00:00Z"))]);
        let value = if action == "transitions" {
            list(vec![value])
        } else {
            value
        };
        let rules = list(vec![map(&[(action, value)])]);
        let error = normalize("BucketLifecycleConfiguration", &[("rules", rules)])
            .await
            .unwrap_err();
        assert!(
            error.contains(action) && error.contains("days") && error.contains("date"),
            "{error}"
        );
    }
    for marker in [true, false] {
        let rules = list(vec![map(&[(
            "expiration",
            map(&[
                ("days", int(1)),
                ("expired_object_delete_marker", bool_(marker)),
            ]),
        )])]);
        assert!(
            normalize("BucketLifecycleConfiguration", &[("rules", rules)])
                .await
                .unwrap_err()
                .contains("expired_object_delete_marker")
        );
    }
    for action in [
        map(&[("days", int(1))]),
        map(&[("date", string("2030-01-01T00:00:00Z"))]),
        map(&[("expired_object_delete_marker", bool_(true))]),
    ] {
        let rules = list(vec![map(&[("expiration", action)])]);
        normalize("BucketLifecycleConfiguration", &[("rules", rules)])
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn s3_lifecycle_size_range_must_increase_without_guessing_the_50_tb_boundary() {
    for (lower, upper) in [(1, 1), (2, 1)] {
        let rules = list(vec![map(&[(
            "filter",
            map(&[(
                "and",
                map(&[
                    ("object_size_greater_than", int(lower)),
                    ("object_size_less_than", int(upper)),
                ]),
            )]),
        )])]);
        let error = normalize("BucketLifecycleConfiguration", &[("rules", rules)])
            .await
            .unwrap_err();
        assert!(
            error.contains("object_size_greater_than") && error.contains("object_size_less_than"),
            "{error}"
        );
    }
    // AWS's documents give conflicting approximations of '50 TB'; no primitive cap.
    for upper in [50_000_000_000_000, 53_687_091_200_000, 54_975_581_388_800] {
        let rules = list(vec![map(&[(
            "filter",
            map(&[(
                "and",
                map(&[
                    ("object_size_greater_than", int(0)),
                    ("object_size_less_than", int(upper)),
                ]),
            )]),
        )])]);
        normalize("BucketLifecycleConfiguration", &[("rules", rules)])
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn s3_logging_key_format_selects_exactly_one_format() {
    for fields in [
        vec![],
        vec![
            ("simple_prefix", map(&[])),
            ("partitioned_prefix", map(&[])),
        ],
    ] {
        let error = normalize(
            "BucketLogging",
            &[("target_object_key_format", map(&fields))],
        )
        .await
        .unwrap_err();
        assert!(
            error.contains("simple_prefix") && error.contains("partitioned_prefix"),
            "{error}"
        );
    }
    for field in ["simple_prefix", "partitioned_prefix"] {
        normalize(
            "BucketLogging",
            &[("target_object_key_format", map(&[(field, map(&[]))]))],
        )
        .await
        .unwrap();
    }
    normalize("BucketLogging", &[]).await.unwrap();
}

#[tokio::test]
async fn s3_website_redirect_excludes_documents_and_otherwise_needs_index() {
    for document in ["index_document", "error_document"] {
        let error = normalize(
            "BucketWebsiteConfiguration",
            &[
                (
                    "redirect_all_requests_to",
                    map(&[("host_name", string("example.com"))]),
                ),
                (document, map(&[])),
            ],
        )
        .await
        .unwrap_err();
        assert!(error.contains("redirect_all_requests_to"), "{error}");
    }
    assert!(
        normalize(
            "BucketWebsiteConfiguration",
            &[("error_document", map(&[("key", string("error.html"))]))]
        )
        .await
        .unwrap_err()
        .contains("index_document")
    );
    normalize(
        "BucketWebsiteConfiguration",
        &[("index_document", map(&[("suffix", string("index.html"))]))],
    )
    .await
    .unwrap();
    normalize(
        "BucketWebsiteConfiguration",
        &[(
            "redirect_all_requests_to",
            map(&[("host_name", string("example.com"))]),
        )],
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn s3_conditional_rules_defer_unknown_parents_and_leaves() {
    for opaque in [
        Value::resource_ref("peer", "value", vec![]).resolve_root_for_test(),
        Value::Deferred(DeferredValue::Secret(Box::new(string("opaque")))),
    ] {
        for (resource, field) in [
            ("BucketLifecycleConfiguration", "rules"),
            ("BucketLogging", "target_object_key_format"),
            ("BucketWebsiteConfiguration", "redirect_all_requests_to"),
        ] {
            normalize(resource, &[(field, opaque.clone())])
                .await
                .unwrap();
        }
        let rules = list(vec![map(&[(
            "expiration",
            map(&[
                ("days", opaque.clone()),
                ("date", string("2030-01-01T00:00:00Z")),
            ]),
        )])]);
        normalize("BucketLifecycleConfiguration", &[("rules", rules)])
            .await
            .unwrap();
        normalize(
            "BucketLogging",
            &[(
                "target_object_key_format",
                map(&[
                    ("simple_prefix", opaque.clone()),
                    ("partitioned_prefix", map(&[])),
                ]),
            )],
        )
        .await
        .unwrap();
        normalize(
            "BucketWebsiteConfiguration",
            &[
                ("redirect_all_requests_to", opaque.clone()),
                ("index_document", map(&[])),
            ],
        )
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn s3_lifecycle_version_count_uses_the_providers_automatic_empty_filter() {
    // The provider emits Filter={} when omitted, satisfying AWS's requirement
    // that NewerNoncurrentVersions have a Filter without requiring an infra rewrite.
    let rules = list(vec![map(&[(
        "noncurrent_version_expiration",
        map(&[
            ("noncurrent_days", int(30)),
            ("newer_noncurrent_versions", int(1)),
        ]),
    )])]);
    normalize("BucketLifecycleConfiguration", &[("rules", rules)])
        .await
        .unwrap();
}

#[tokio::test]
async fn s3_lifecycle_rule_cannot_mix_date_and_day_actions() {
    let rules = list(vec![map(&[
        ("expiration", map(&[("days", int(30))])),
        (
            "transitions",
            list(vec![map(&[("date", string("2030-01-01T00:00:00Z"))])]),
        ),
    ])]);
    let error = normalize("BucketLifecycleConfiguration", &[("rules", rules)])
        .await
        .unwrap_err();
    assert!(error.contains("days") && error.contains("date"), "{error}");
}

#[tokio::test]
async fn s3_lifecycle_tag_filters_exclude_multipart_abort_and_delete_marker_actions() {
    for filter in [
        map(&[(
            "tag",
            map(&[("key", string("env")), ("value", string("prod"))]),
        )]),
        map(&[(
            "and",
            map(&[(
                "tags",
                list(vec![map(&[
                    ("key", string("env")),
                    ("value", string("prod")),
                ])]),
            )]),
        )]),
    ] {
        for (action, value) in [
            (
                "abort_incomplete_multipart_upload",
                map(&[("days_after_initiation", int(7))]),
            ),
            (
                "expiration",
                map(&[("expired_object_delete_marker", bool_(true))]),
            ),
        ] {
            let rules = list(vec![map(&[("filter", filter.clone()), (action, value)])]);
            assert!(
                normalize("BucketLifecycleConfiguration", &[("rules", rules)])
                    .await
                    .unwrap_err()
                    .contains("tag")
            );
        }
    }
}

#[tokio::test]
async fn s3_replication_filter_exclusivity_and_tag_delete_marker_conflicts_are_local() {
    let filter = map(&[
        ("prefix", string("logs/")),
        (
            "tag",
            map(&[("key", string("env")), ("value", string("prod"))]),
        ),
    ]);
    let rules = list(vec![map(&[("filter", filter)])]);
    assert!(
        normalize("BucketReplicationConfiguration", &[("rules", rules)])
            .await
            .unwrap_err()
            .contains("filter")
    );
    let filter = map(&[(
        "tag",
        map(&[("key", string("env")), ("value", string("prod"))]),
    )]);
    let rules = list(vec![map(&[
        ("filter", filter),
        (
            "delete_marker_replication",
            map(&[("status", string("Enabled"))]),
        ),
    ])]);
    assert!(
        normalize("BucketReplicationConfiguration", &[("rules", rules)])
            .await
            .unwrap_err()
            .contains("delete_marker_replication")
    );
}

fn notification(event: Value, prefix: Value, suffix: &str) -> Value {
    map(&[
        ("events", list(vec![event])),
        (
            "filter",
            map(&[(
                "filter_rules",
                list(vec![
                    map(&[("name", string("prefix")), ("value", prefix)]),
                    map(&[("name", string("suffix")), ("value", string(suffix))]),
                ]),
            )]),
        ),
    ])
}

#[tokio::test]
async fn s3_notification_overlap_compares_events_and_both_prefix_and_suffix_across_destinations() {
    for (left, right, expected) in [
        (
            notification(string("s3:ObjectCreated:*"), string("images/"), ".jpg"),
            notification(string("s3:ObjectCreated:Put"), string("images/raw/"), "jpg"),
            false,
        ),
        (
            notification(string("s3:ObjectCreated:Put"), string("images/"), ".jpg"),
            notification(string("s3:ObjectRemoved:Delete"), string("images/"), ".jpg"),
            true,
        ),
        (
            notification(string("s3:ObjectCreated:Put"), string("images/"), ".jpg"),
            notification(string("s3:ObjectCreated:Put"), string("images/"), ".png"),
            true,
        ),
        (
            notification(string("s3:ObjectCreated:Put"), string("images/"), ".jpg"),
            notification(string("s3:ObjectCreated:Put"), string("logs/"), ".jpg"),
            true,
        ),
        (
            notification(string("s3:ObjectCreated:Put"), string("a+b/"), ""),
            notification(string("s3:ObjectCreated:Put"), string("a%20b/"), ""),
            false,
        ),
    ] {
        let result = normalize(
            "BucketNotificationConfiguration",
            &[
                ("topic_configurations", list(vec![left])),
                ("queue_configurations", list(vec![right])),
            ],
        )
        .await;
        assert_eq!(result.is_ok(), expected, "{result:?}");
    }
    for opaque in [
        Value::resource_ref("peer", "value", vec![]).resolve_root_for_test(),
        Value::Deferred(DeferredValue::Secret(Box::new(string("opaque")))),
    ] {
        for second in [
            notification(opaque.clone(), string("images/"), ".jpg"),
            notification(string("s3:ObjectCreated:Put"), opaque.clone(), ".jpg"),
        ] {
            normalize(
                "BucketNotificationConfiguration",
                &[(
                    "lambda_function_configurations",
                    list(vec![
                        notification(string("s3:ObjectCreated:Put"), string("images/"), ".jpg"),
                        second,
                    ]),
                )],
            )
            .await
            .unwrap();
        }
    }
}
