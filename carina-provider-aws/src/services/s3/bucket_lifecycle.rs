use std::collections::HashMap;

use aws_sdk_s3::types::{
    AbortIncompleteMultipartUpload, BucketLifecycleConfiguration, ExpirationStatus,
    LifecycleExpiration, LifecycleRule, LifecycleRuleAndOperator, LifecycleRuleFilter,
    NoncurrentVersionExpiration, NoncurrentVersionTransition, Tag, Transition,
    TransitionStorageClass,
};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, ManagedResource, ResourceId, State, Value};
use indexmap::IndexMap;

use crate::AwsProvider;
use crate::helpers::{RetryPolicy, require_string_attr, retry_aws_operation, sdk_error_message};
use crate::services::s3::bucket::is_s3_not_configured_error;

impl AwsProvider {
    pub(crate) async fn read_s3_bucket_lifecycle_configuration(
        &self,
        id: &ResourceId,
        identifier: Option<&str>,
    ) -> ProviderResult<State> {
        let Some(bucket) = identifier else {
            return Ok(State::not_found(id.clone()));
        };

        let result = self
            .s3_client
            .get_bucket_lifecycle_configuration()
            .bucket(bucket)
            .send()
            .await;

        match result {
            Ok(output) => {
                let mut attributes = HashMap::new();
                attributes.insert(
                    "bucket".to_string(),
                    Value::Concrete(ConcreteValue::String(bucket.to_string())),
                );
                let rules: Vec<Value> = output.rules().iter().map(rule_to_value).collect();
                if !rules.is_empty() {
                    attributes.insert(
                        "rules".to_string(),
                        Value::Concrete(ConcreteValue::List(rules)),
                    );
                }
                Ok(State::existing(id.clone(), attributes).with_identifier(bucket.to_string()))
            }
            Err(e) => {
                if is_s3_not_configured_error(&e, "NoSuchLifecycleConfiguration")
                    || is_s3_not_configured_error(&e, "NoSuchBucket")
                {
                    return Ok(State::not_found(id.clone()));
                }
                Err(ProviderError::api_error(sdk_error_message(
                    "Failed to get bucket lifecycle configuration",
                    &e,
                ))
                .for_resource(id.clone()))
            }
        }
    }

    pub(crate) async fn create_s3_bucket_lifecycle_configuration(
        &self,
        resource: ManagedResource,
    ) -> ProviderResult<State> {
        let bucket = require_string_attr(&resource, "bucket")?;
        self.put_s3_bucket_lifecycle(&resource.id, &bucket, &resource)
            .await
    }

    pub(crate) async fn update_s3_bucket_lifecycle_configuration(
        &self,
        id: ResourceId,
        identifier: &str,
        _from: &State,
        to: ManagedResource,
    ) -> ProviderResult<State> {
        self.put_s3_bucket_lifecycle(&id, identifier, &to).await
    }

    async fn put_s3_bucket_lifecycle(
        &self,
        id: &ResourceId,
        bucket: &str,
        resource: &ManagedResource,
    ) -> ProviderResult<State> {
        let rules = match resource.get_attr("rules") {
            Some(Value::Concrete(ConcreteValue::List(items))) => items,
            _ => {
                return Err(
                    ProviderError::invalid_input("rules is required and must be a list")
                        .for_resource(id.clone()),
                );
            }
        };

        let sdk_rules: Vec<LifecycleRule> = rules
            .iter()
            .map(|v| build_rule(id, v))
            .collect::<ProviderResult<Vec<_>>>()?;

        let config = BucketLifecycleConfiguration::builder()
            .set_rules(Some(sdk_rules))
            .build()
            .map_err(|e| {
                ProviderError::api_error(sdk_error_message(
                    "Failed to build BucketLifecycleConfiguration",
                    &e,
                ))
                .for_resource(id.clone())
            })?;

        self.s3_client
            .put_bucket_lifecycle_configuration()
            .bucket(bucket)
            .lifecycle_configuration(config)
            .send()
            .await
            .map_err(|e| {
                ProviderError::api_error(sdk_error_message(
                    "Failed to put bucket lifecycle configuration",
                    &e,
                ))
                .for_resource(id.clone())
            })?;

        self.read_s3_bucket_lifecycle_configuration(id, Some(bucket))
            .await
    }

    pub(crate) async fn delete_s3_bucket_lifecycle_configuration_idempotent(
        &self,
        id: ResourceId,
        identifier: &str,
    ) -> ProviderResult<()> {
        let result = retry_aws_operation(
            "delete bucket lifecycle configuration",
            RetryPolicy::default(),
            || {
                let client = &self.s3_client;
                async move {
                    client
                        .delete_bucket_lifecycle()
                        .bucket(identifier)
                        .send()
                        .await
                }
            },
        )
        .await;

        match result {
            Ok(_) => Ok(()),
            Err(e)
                if is_s3_not_configured_error(&e, "NoSuchLifecycleConfiguration")
                    || is_s3_not_configured_error(&e, "NoSuchBucket") =>
            {
                Ok(())
            }
            Err(e) => Err(ProviderError::api_error(sdk_error_message(
                "Failed to delete bucket lifecycle configuration",
                &e,
            ))
            .for_resource(id.clone())),
        }
    }
}

fn build_rule(id: &ResourceId, rule_value: &Value) -> ProviderResult<LifecycleRule> {
    let Value::Concrete(ConcreteValue::Map(map)) = rule_value else {
        return Err(
            ProviderError::invalid_input("each rule must be a map").for_resource(id.clone())
        );
    };

    let status_str = match map.get("status") {
        Some(Value::Concrete(ConcreteValue::String(s))) => s.clone(),
        _ => {
            return Err(
                ProviderError::invalid_input("rule.status is required").for_resource(id.clone())
            );
        }
    };

    let mut builder = LifecycleRule::builder().status(ExpirationStatus::from(status_str.as_str()));

    if let Some(Value::Concrete(ConcreteValue::String(s))) = map.get("id") {
        builder = builder.id(s);
    }
    // S3 requires every V2 lifecycle rule to carry a `Filter` element.
    // Build one from the `filter` attribute, or fall back to an empty
    // filter (matches all objects) so the request is not rejected with
    // `MalformedXML`.
    builder = builder.filter(match map.get("filter") {
        Some(v) => build_filter(id, v)?,
        None => LifecycleRuleFilter::builder().build(),
    });
    if let Some(v) = map.get("expiration") {
        builder = builder.expiration(build_expiration(id, v)?);
    }
    if let Some(Value::Concrete(ConcreteValue::List(items))) = map.get("transitions") {
        let transitions: Vec<Transition> = items
            .iter()
            .map(|v| build_transition(id, v))
            .collect::<ProviderResult<Vec<_>>>()?;
        builder = builder.set_transitions(Some(transitions));
    }
    if let Some(v) = map.get("noncurrent_version_expiration") {
        builder = builder.noncurrent_version_expiration(build_ncv_expiration(id, v)?);
    }
    if let Some(Value::Concrete(ConcreteValue::List(items))) =
        map.get("noncurrent_version_transitions")
    {
        let transitions: Vec<NoncurrentVersionTransition> = items
            .iter()
            .map(|v| build_ncv_transition(id, v))
            .collect::<ProviderResult<Vec<_>>>()?;
        builder = builder.set_noncurrent_version_transitions(Some(transitions));
    }
    if let Some(v) = map.get("abort_incomplete_multipart_upload") {
        builder = builder.abort_incomplete_multipart_upload(build_abort_multipart(id, v)?);
    }

    builder.build().map_err(|e| {
        ProviderError::api_error(sdk_error_message("Failed to build LifecycleRule", &e))
            .for_resource(id.clone())
    })
}

/// Build a single S3 object tag from a `{ key, value }` map.
fn build_filter_tag(id: &ResourceId, value: &Value) -> ProviderResult<Tag> {
    let Value::Concrete(ConcreteValue::Map(map)) = value else {
        return Err(
            ProviderError::invalid_input("filter tag must be a map").for_resource(id.clone())
        );
    };
    let key = match map.get("key") {
        Some(Value::Concrete(ConcreteValue::String(s))) => s.clone(),
        _ => {
            return Err(
                ProviderError::invalid_input("filter tag.key is required").for_resource(id.clone())
            );
        }
    };
    let val = match map.get("value") {
        Some(Value::Concrete(ConcreteValue::String(s))) => s.clone(),
        _ => {
            return Err(ProviderError::invalid_input("filter tag.value is required")
                .for_resource(id.clone()));
        }
    };
    Tag::builder().key(key).value(val).build().map_err(|e| {
        ProviderError::api_error(sdk_error_message("Failed to build filter tag", &e))
            .for_resource(id.clone())
    })
}

/// Build a `LifecycleRuleAndOperator` (prefix + tags + size bounds).
fn build_filter_and(id: &ResourceId, value: &Value) -> ProviderResult<LifecycleRuleAndOperator> {
    let Value::Concrete(ConcreteValue::Map(map)) = value else {
        return Err(
            ProviderError::invalid_input("filter.and must be a map").for_resource(id.clone())
        );
    };
    let mut builder = LifecycleRuleAndOperator::builder();
    if let Some(Value::Concrete(ConcreteValue::String(s))) = map.get("prefix") {
        builder = builder.prefix(s);
    }
    if let Some(Value::Concrete(ConcreteValue::List(items))) = map.get("tags") {
        let tags: Vec<Tag> = items
            .iter()
            .map(|v| build_filter_tag(id, v))
            .collect::<ProviderResult<Vec<_>>>()?;
        builder = builder.set_tags(Some(tags));
    }
    if let Some(Value::Concrete(ConcreteValue::Int(n))) = map.get("object_size_greater_than") {
        builder = builder.object_size_greater_than(*n);
    }
    if let Some(Value::Concrete(ConcreteValue::Int(n))) = map.get("object_size_less_than") {
        builder = builder.object_size_less_than(*n);
    }
    Ok(builder.build())
}

/// Build a `LifecycleRuleFilter` from the DSL `filter` map. S3 allows at
/// most one of `prefix` / `tag` / size bound / `and` to be set; the
/// schema validates the shape, so this just forwards whatever is present.
fn build_filter(id: &ResourceId, value: &Value) -> ProviderResult<LifecycleRuleFilter> {
    let Value::Concrete(ConcreteValue::Map(map)) = value else {
        return Err(ProviderError::invalid_input("filter must be a map").for_resource(id.clone()));
    };
    let mut builder = LifecycleRuleFilter::builder();
    if let Some(Value::Concrete(ConcreteValue::String(s))) = map.get("prefix") {
        builder = builder.prefix(s);
    }
    if let Some(v) = map.get("tag") {
        builder = builder.tag(build_filter_tag(id, v)?);
    }
    if let Some(Value::Concrete(ConcreteValue::Int(n))) = map.get("object_size_greater_than") {
        builder = builder.object_size_greater_than(*n);
    }
    if let Some(Value::Concrete(ConcreteValue::Int(n))) = map.get("object_size_less_than") {
        builder = builder.object_size_less_than(*n);
    }
    if let Some(v) = map.get("and") {
        builder = builder.and(build_filter_and(id, v)?);
    }
    Ok(builder.build())
}

fn build_expiration(id: &ResourceId, value: &Value) -> ProviderResult<LifecycleExpiration> {
    let Value::Concrete(ConcreteValue::Map(map)) = value else {
        return Err(
            ProviderError::invalid_input("expiration must be a map").for_resource(id.clone())
        );
    };
    let mut builder = LifecycleExpiration::builder();
    if let Some(Value::Concrete(ConcreteValue::Int(d))) = map.get("days") {
        builder = builder.days(*d as i32);
    }
    if let Some(Value::Concrete(ConcreteValue::Bool(b))) = map.get("expired_object_delete_marker") {
        builder = builder.expired_object_delete_marker(*b);
    }
    Ok(builder.build())
}

fn build_transition(id: &ResourceId, value: &Value) -> ProviderResult<Transition> {
    let Value::Concrete(ConcreteValue::Map(map)) = value else {
        return Err(
            ProviderError::invalid_input("transition must be a map").for_resource(id.clone())
        );
    };
    let storage_class_str = match map.get("storage_class") {
        Some(Value::Concrete(ConcreteValue::String(s))) => s.clone(),
        _ => {
            return Err(
                ProviderError::invalid_input("transition.storage_class is required")
                    .for_resource(id.clone()),
            );
        }
    };
    let mut builder = Transition::builder()
        .storage_class(TransitionStorageClass::from(storage_class_str.as_str()));
    if let Some(Value::Concrete(ConcreteValue::Int(d))) = map.get("days") {
        builder = builder.days(*d as i32);
    }
    Ok(builder.build())
}

fn build_ncv_expiration(
    id: &ResourceId,
    value: &Value,
) -> ProviderResult<NoncurrentVersionExpiration> {
    let Value::Concrete(ConcreteValue::Map(map)) = value else {
        return Err(
            ProviderError::invalid_input("noncurrent_version_expiration must be a map")
                .for_resource(id.clone()),
        );
    };
    let mut builder = NoncurrentVersionExpiration::builder();
    if let Some(Value::Concrete(ConcreteValue::Int(d))) = map.get("noncurrent_days") {
        builder = builder.noncurrent_days(*d as i32);
    }
    if let Some(Value::Concrete(ConcreteValue::Int(n))) = map.get("newer_noncurrent_versions") {
        builder = builder.newer_noncurrent_versions(*n as i32);
    }
    Ok(builder.build())
}

fn build_ncv_transition(
    id: &ResourceId,
    value: &Value,
) -> ProviderResult<NoncurrentVersionTransition> {
    let Value::Concrete(ConcreteValue::Map(map)) = value else {
        return Err(
            ProviderError::invalid_input("noncurrent_version_transition must be a map")
                .for_resource(id.clone()),
        );
    };
    let storage_class_str = match map.get("storage_class") {
        Some(Value::Concrete(ConcreteValue::String(s))) => s.clone(),
        _ => {
            return Err(ProviderError::invalid_input(
                "noncurrent_version_transition.storage_class is required",
            )
            .for_resource(id.clone()));
        }
    };
    let mut builder = NoncurrentVersionTransition::builder()
        .storage_class(TransitionStorageClass::from(storage_class_str.as_str()));
    if let Some(Value::Concrete(ConcreteValue::Int(d))) = map.get("noncurrent_days") {
        builder = builder.noncurrent_days(*d as i32);
    }
    if let Some(Value::Concrete(ConcreteValue::Int(n))) = map.get("newer_noncurrent_versions") {
        builder = builder.newer_noncurrent_versions(*n as i32);
    }
    Ok(builder.build())
}

fn build_abort_multipart(
    id: &ResourceId,
    value: &Value,
) -> ProviderResult<AbortIncompleteMultipartUpload> {
    let Value::Concrete(ConcreteValue::Map(map)) = value else {
        return Err(ProviderError::invalid_input(
            "abort_incomplete_multipart_upload must be a map",
        )
        .for_resource(id.clone()));
    };
    let mut builder = AbortIncompleteMultipartUpload::builder();
    if let Some(Value::Concrete(ConcreteValue::Int(d))) = map.get("days_after_initiation") {
        builder = builder.days_after_initiation(*d as i32);
    }
    Ok(builder.build())
}

fn rule_to_value(rule: &LifecycleRule) -> Value {
    let mut map = IndexMap::new();
    if let Some(s) = rule.id()
        && !s.is_empty()
    {
        map.insert(
            "id".to_string(),
            Value::Concrete(ConcreteValue::String(s.to_string())),
        );
    }
    map.insert(
        "status".to_string(),
        Value::Concrete(ConcreteValue::String(rule.status().as_str().to_string())),
    );
    // Only surface a non-empty filter. S3 returns an empty `<Filter/>`
    // for match-all rules; reflecting that as an empty map would diff
    // against a DSL config that simply omits `filter`.
    if let Some(filter) = rule.filter()
        && let Some(filter_value) = filter_to_value(filter)
    {
        map.insert("filter".to_string(), filter_value);
    }
    if let Some(exp) = rule.expiration() {
        let mut e = IndexMap::new();
        if let Some(d) = exp.days() {
            e.insert(
                "days".to_string(),
                Value::Concrete(ConcreteValue::Int(d as i64)),
            );
        }
        if let Some(b) = exp.expired_object_delete_marker() {
            e.insert(
                "expired_object_delete_marker".to_string(),
                Value::Concrete(ConcreteValue::Bool(b)),
            );
        }
        if !e.is_empty() {
            map.insert(
                "expiration".to_string(),
                Value::Concrete(ConcreteValue::Map(e)),
            );
        }
    }
    let transitions: Vec<Value> = rule.transitions().iter().map(transition_to_value).collect();
    if !transitions.is_empty() {
        map.insert(
            "transitions".to_string(),
            Value::Concrete(ConcreteValue::List(transitions)),
        );
    }
    if let Some(ncv) = rule.noncurrent_version_expiration() {
        let mut e = IndexMap::new();
        if let Some(d) = ncv.noncurrent_days() {
            e.insert(
                "noncurrent_days".to_string(),
                Value::Concrete(ConcreteValue::Int(d as i64)),
            );
        }
        if let Some(n) = ncv.newer_noncurrent_versions() {
            e.insert(
                "newer_noncurrent_versions".to_string(),
                Value::Concrete(ConcreteValue::Int(n as i64)),
            );
        }
        if !e.is_empty() {
            map.insert(
                "noncurrent_version_expiration".to_string(),
                Value::Concrete(ConcreteValue::Map(e)),
            );
        }
    }
    let ncv_transitions: Vec<Value> = rule
        .noncurrent_version_transitions()
        .iter()
        .map(ncv_transition_to_value)
        .collect();
    if !ncv_transitions.is_empty() {
        map.insert(
            "noncurrent_version_transitions".to_string(),
            Value::Concrete(ConcreteValue::List(ncv_transitions)),
        );
    }
    if let Some(abort) = rule.abort_incomplete_multipart_upload() {
        let mut a = IndexMap::new();
        if let Some(d) = abort.days_after_initiation() {
            a.insert(
                "days_after_initiation".to_string(),
                Value::Concrete(ConcreteValue::Int(d as i64)),
            );
        }
        if !a.is_empty() {
            map.insert(
                "abort_incomplete_multipart_upload".to_string(),
                Value::Concrete(ConcreteValue::Map(a)),
            );
        }
    }
    Value::Concrete(ConcreteValue::Map(map))
}

/// Convert an S3 `Tag` into a `{ key, value }` map value.
fn tag_to_value(tag: &Tag) -> Value {
    let mut m = IndexMap::new();
    m.insert(
        "key".to_string(),
        Value::Concrete(ConcreteValue::String(tag.key().to_string())),
    );
    m.insert(
        "value".to_string(),
        Value::Concrete(ConcreteValue::String(tag.value().to_string())),
    );
    Value::Concrete(ConcreteValue::Map(m))
}

/// Convert a `LifecycleRuleAndOperator` into a map value, or `None` when
/// every field is empty.
fn filter_and_to_value(and: &LifecycleRuleAndOperator) -> Option<Value> {
    let mut m = IndexMap::new();
    if let Some(p) = and.prefix()
        && !p.is_empty()
    {
        m.insert(
            "prefix".to_string(),
            Value::Concrete(ConcreteValue::String(p.to_string())),
        );
    }
    let tags = and.tags();
    if !tags.is_empty() {
        m.insert(
            "tags".to_string(),
            Value::Concrete(ConcreteValue::List(tags.iter().map(tag_to_value).collect())),
        );
    }
    if let Some(n) = and.object_size_greater_than() {
        m.insert(
            "object_size_greater_than".to_string(),
            Value::Concrete(ConcreteValue::Int(n)),
        );
    }
    if let Some(n) = and.object_size_less_than() {
        m.insert(
            "object_size_less_than".to_string(),
            Value::Concrete(ConcreteValue::Int(n)),
        );
    }
    if m.is_empty() {
        None
    } else {
        Some(Value::Concrete(ConcreteValue::Map(m)))
    }
}

/// Convert a `LifecycleRuleFilter` into a map value, or `None` when the
/// filter is empty (a match-all `<Filter/>`).
fn filter_to_value(filter: &LifecycleRuleFilter) -> Option<Value> {
    let mut m = IndexMap::new();
    if let Some(p) = filter.prefix()
        && !p.is_empty()
    {
        m.insert(
            "prefix".to_string(),
            Value::Concrete(ConcreteValue::String(p.to_string())),
        );
    }
    if let Some(tag) = filter.tag() {
        m.insert("tag".to_string(), tag_to_value(tag));
    }
    if let Some(n) = filter.object_size_greater_than() {
        m.insert(
            "object_size_greater_than".to_string(),
            Value::Concrete(ConcreteValue::Int(n)),
        );
    }
    if let Some(n) = filter.object_size_less_than() {
        m.insert(
            "object_size_less_than".to_string(),
            Value::Concrete(ConcreteValue::Int(n)),
        );
    }
    if let Some(and) = filter.and()
        && let Some(and_value) = filter_and_to_value(and)
    {
        m.insert("and".to_string(), and_value);
    }
    if m.is_empty() {
        None
    } else {
        Some(Value::Concrete(ConcreteValue::Map(m)))
    }
}

fn transition_to_value(t: &Transition) -> Value {
    let mut m = IndexMap::new();
    if let Some(d) = t.days() {
        m.insert(
            "days".to_string(),
            Value::Concrete(ConcreteValue::Int(d as i64)),
        );
    }
    if let Some(sc) = t.storage_class() {
        m.insert(
            "storage_class".to_string(),
            Value::Concrete(ConcreteValue::String(sc.as_str().to_string())),
        );
    }
    Value::Concrete(ConcreteValue::Map(m))
}

fn ncv_transition_to_value(t: &NoncurrentVersionTransition) -> Value {
    let mut m = IndexMap::new();
    if let Some(d) = t.noncurrent_days() {
        m.insert(
            "noncurrent_days".to_string(),
            Value::Concrete(ConcreteValue::Int(d as i64)),
        );
    }
    if let Some(n) = t.newer_noncurrent_versions() {
        m.insert(
            "newer_noncurrent_versions".to_string(),
            Value::Concrete(ConcreteValue::Int(n as i64)),
        );
    }
    if let Some(sc) = t.storage_class() {
        m.insert(
            "storage_class".to_string(),
            Value::Concrete(ConcreteValue::String(sc.as_str().to_string())),
        );
    }
    Value::Concrete(ConcreteValue::Map(m))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rid() -> ResourceId {
        ResourceId::new("s3.bucket_lifecycle_configuration", "test")
    }

    fn str_val(s: &str) -> Value {
        Value::Concrete(ConcreteValue::String(s.to_string()))
    }

    fn map_val(pairs: Vec<(&str, Value)>) -> Value {
        let mut m = IndexMap::new();
        for (k, v) in pairs {
            m.insert(k.to_string(), v);
        }
        Value::Concrete(ConcreteValue::Map(m))
    }

    #[test]
    fn rule_without_filter_gets_empty_filter() {
        // Reproduces carina-rs/carina-provider-aws#273: a rule with no
        // `filter` must still emit a `Filter` element, otherwise S3
        // rejects the request with `MalformedXML`.
        let rule_value = map_val(vec![
            ("id", str_val("expire-all")),
            ("status", str_val("Enabled")),
            (
                "expiration",
                map_val(vec![("days", Value::Concrete(ConcreteValue::Int(365)))]),
            ),
        ]);
        let rule = build_rule(&rid(), &rule_value).expect("build_rule should succeed");
        assert!(
            rule.filter().is_some(),
            "a lifecycle rule must always carry a Filter element"
        );
    }

    #[test]
    fn rule_with_prefix_filter() {
        let rule_value = map_val(vec![
            ("status", str_val("Enabled")),
            ("filter", map_val(vec![("prefix", str_val("logs/"))])),
        ]);
        let rule = build_rule(&rid(), &rule_value).expect("build_rule should succeed");
        let filter = rule.filter().expect("filter set");
        assert_eq!(filter.prefix(), Some("logs/"));
    }

    #[test]
    fn rule_with_tag_filter() {
        let rule_value = map_val(vec![
            ("status", str_val("Enabled")),
            (
                "filter",
                map_val(vec![(
                    "tag",
                    map_val(vec![("key", str_val("env")), ("value", str_val("dev"))]),
                )]),
            ),
        ]);
        let rule = build_rule(&rid(), &rule_value).expect("build_rule should succeed");
        let tag = rule.filter().and_then(|f| f.tag()).expect("tag set");
        assert_eq!(tag.key(), "env");
        assert_eq!(tag.value(), "dev");
    }

    #[test]
    fn rule_with_and_filter() {
        let rule_value = map_val(vec![
            ("status", str_val("Enabled")),
            (
                "filter",
                map_val(vec![(
                    "and",
                    map_val(vec![
                        ("prefix", str_val("data/")),
                        (
                            "object_size_greater_than",
                            Value::Concrete(ConcreteValue::Int(1024)),
                        ),
                        (
                            "tags",
                            Value::Concrete(ConcreteValue::List(vec![map_val(vec![
                                ("key", str_val("tier")),
                                ("value", str_val("cold")),
                            ])])),
                        ),
                    ]),
                )]),
            ),
        ]);
        let rule = build_rule(&rid(), &rule_value).expect("build_rule should succeed");
        let and = rule.filter().and_then(|f| f.and()).expect("and set");
        assert_eq!(and.prefix(), Some("data/"));
        assert_eq!(and.object_size_greater_than(), Some(1024));
        assert_eq!(and.tags().len(), 1);
    }

    #[test]
    fn empty_filter_is_not_surfaced_on_read() {
        // S3 returns an empty <Filter/> for match-all rules; reading it
        // back as an empty map would diff against a DSL that omits it.
        let empty = LifecycleRuleFilter::builder().build();
        assert!(filter_to_value(&empty).is_none());
    }

    #[test]
    fn prefix_filter_round_trips() {
        let built = build_rule(
            &rid(),
            &map_val(vec![
                ("status", str_val("Enabled")),
                ("filter", map_val(vec![("prefix", str_val("logs/"))])),
            ]),
        )
        .unwrap();
        let value = rule_to_value(&built);
        let Value::Concrete(ConcreteValue::Map(m)) = value else {
            panic!("expected map");
        };
        let Some(Value::Concrete(ConcreteValue::Map(filter))) = m.get("filter") else {
            panic!("filter should round-trip");
        };
        assert_eq!(filter.get("prefix"), Some(&str_val("logs/")));
    }
}
