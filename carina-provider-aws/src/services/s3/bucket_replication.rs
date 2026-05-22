use std::collections::HashMap;

use aws_sdk_s3::types::{
    DeleteMarkerReplication, DeleteMarkerReplicationStatus, Destination, ReplicationConfiguration,
    ReplicationRule, ReplicationRuleAndOperator, ReplicationRuleFilter, ReplicationRuleStatus,
    StorageClass, Tag,
};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, ManagedResource, ResourceId, State, Value};
use indexmap::IndexMap;

use crate::AwsProvider;
use crate::helpers::{require_string_attr, retry_aws_operation, sdk_error_message};
use crate::services::s3::bucket::is_s3_not_configured_error;

impl AwsProvider {
    pub(crate) async fn read_s3_bucket_replication_configuration(
        &self,
        id: &ResourceId,
        identifier: Option<&str>,
    ) -> ProviderResult<State> {
        let Some(bucket) = identifier else {
            return Ok(State::not_found(id.clone()));
        };

        let result = self
            .s3_client
            .get_bucket_replication()
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
                if let Some(config) = output.replication_configuration() {
                    attributes.insert(
                        "role".to_string(),
                        Value::Concrete(ConcreteValue::String(config.role().to_string())),
                    );
                    let rules: Vec<Value> = config.rules().iter().map(rule_to_value).collect();
                    if !rules.is_empty() {
                        attributes.insert(
                            "rules".to_string(),
                            Value::Concrete(ConcreteValue::List(rules)),
                        );
                    }
                }
                Ok(State::existing(id.clone(), attributes).with_identifier(bucket.to_string()))
            }
            Err(e) => {
                if is_s3_not_configured_error(&e, "ReplicationConfigurationNotFoundError")
                    || is_s3_not_configured_error(&e, "NoSuchBucket")
                {
                    return Ok(State::not_found(id.clone()));
                }
                Err(ProviderError::api_error(sdk_error_message(
                    "Failed to get bucket replication",
                    &e,
                ))
                .for_resource(id.clone()))
            }
        }
    }

    pub(crate) async fn create_s3_bucket_replication_configuration(
        &self,
        resource: ManagedResource,
    ) -> ProviderResult<State> {
        let bucket = require_string_attr(&resource, "bucket")?;
        self.put_s3_bucket_replication(&resource.id, &bucket, &resource)
            .await
    }

    pub(crate) async fn update_s3_bucket_replication_configuration(
        &self,
        id: ResourceId,
        identifier: &str,
        _from: &State,
        to: ManagedResource,
    ) -> ProviderResult<State> {
        self.put_s3_bucket_replication(&id, identifier, &to).await
    }

    async fn put_s3_bucket_replication(
        &self,
        id: &ResourceId,
        bucket: &str,
        resource: &ManagedResource,
    ) -> ProviderResult<State> {
        let role = require_string_attr(resource, "role")?;
        let rules = match resource.get_attr("rules") {
            Some(Value::Concrete(ConcreteValue::List(items))) => items,
            _ => {
                return Err(
                    ProviderError::invalid_input("rules is required and must be a list")
                        .for_resource(id.clone()),
                );
            }
        };

        let sdk_rules: Vec<ReplicationRule> = rules
            .iter()
            .map(|v| build_rule(id, v))
            .collect::<ProviderResult<Vec<_>>>()?;

        let config = ReplicationConfiguration::builder()
            .role(&role)
            .set_rules(Some(sdk_rules))
            .build()
            .map_err(|e| {
                ProviderError::api_error(sdk_error_message(
                    "Failed to build replication configuration",
                    &e,
                ))
                .for_resource(id.clone())
            })?;

        self.s3_client
            .put_bucket_replication()
            .bucket(bucket)
            .replication_configuration(config)
            .send()
            .await
            .map_err(|e| {
                ProviderError::api_error(sdk_error_message("Failed to put bucket replication", &e))
                    .for_resource(id.clone())
            })?;

        self.read_s3_bucket_replication_configuration(id, Some(bucket))
            .await
    }

    pub(crate) async fn delete_s3_bucket_replication_configuration_idempotent(
        &self,
        id: ResourceId,
        identifier: &str,
    ) -> ProviderResult<()> {
        let result = retry_aws_operation("delete bucket replication", 3, 5, || {
            let client = &self.s3_client;
            async move {
                client
                    .delete_bucket_replication()
                    .bucket(identifier)
                    .send()
                    .await
            }
        })
        .await;

        match result {
            Ok(_) => Ok(()),
            Err(e)
                if is_s3_not_configured_error(&e, "ReplicationConfigurationNotFoundError")
                    || is_s3_not_configured_error(&e, "NoSuchBucket") =>
            {
                Ok(())
            }
            Err(e) => Err(ProviderError::api_error(sdk_error_message(
                "Failed to delete bucket replication",
                &e,
            ))
            .for_resource(id.clone())),
        }
    }
}

fn build_rule(id: &ResourceId, rule_value: &Value) -> ProviderResult<ReplicationRule> {
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
    let destination = match map.get("destination") {
        Some(v) => build_destination(id, v)?,
        None => {
            return Err(ProviderError::invalid_input("rule.destination is required")
                .for_resource(id.clone()));
        }
    };

    // S3 requires every V2 replication rule to carry a `Filter`. Build
    // one from the `filter` attribute, or fall back to an empty filter
    // (matches all objects) so the request is not rejected with
    // `MalformedXML`.
    let filter = match map.get("filter") {
        Some(v) => build_filter(id, v)?,
        None => ReplicationRuleFilter::builder().build(),
    };
    // A `Filter`-based rule must also declare `DeleteMarkerReplication`.
    // Default to `Disabled` when the DSL omits it.
    let delete_marker_status = match map.get("delete_marker_replication") {
        Some(Value::Concrete(ConcreteValue::Map(dm))) => match dm.get("status") {
            Some(Value::Concrete(ConcreteValue::String(s))) => {
                DeleteMarkerReplicationStatus::from(s.as_str())
            }
            _ => {
                return Err(ProviderError::invalid_input(
                    "delete_marker_replication.status is required",
                )
                .for_resource(id.clone()));
            }
        },
        _ => DeleteMarkerReplicationStatus::Disabled,
    };

    let mut builder = ReplicationRule::builder()
        .status(ReplicationRuleStatus::from(status_str.as_str()))
        .filter(filter)
        .delete_marker_replication(
            DeleteMarkerReplication::builder()
                .status(delete_marker_status)
                .build(),
        )
        .destination(destination);

    if let Some(Value::Concrete(ConcreteValue::String(s))) = map.get("id") {
        builder = builder.id(s);
    }
    // A V2 replication rule (one carrying a `Filter`) must declare a
    // `Priority` — S3 uses it to resolve precedence when rules overlap.
    // Default to 0 (lowest precedence) when the DSL omits it.
    let priority = match map.get("priority") {
        Some(Value::Concrete(ConcreteValue::Int(n))) => *n as i32,
        _ => 0,
    };
    builder = builder.priority(priority);

    builder.build().map_err(|e| {
        ProviderError::api_error(sdk_error_message("Failed to build ReplicationRule", &e))
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

/// Build a `ReplicationRuleAndOperator` (prefix + tags).
fn build_filter_and(id: &ResourceId, value: &Value) -> ProviderResult<ReplicationRuleAndOperator> {
    let Value::Concrete(ConcreteValue::Map(map)) = value else {
        return Err(
            ProviderError::invalid_input("filter.and must be a map").for_resource(id.clone())
        );
    };
    let mut builder = ReplicationRuleAndOperator::builder();
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
    Ok(builder.build())
}

/// Build a `ReplicationRuleFilter` from the DSL `filter` map.
fn build_filter(id: &ResourceId, value: &Value) -> ProviderResult<ReplicationRuleFilter> {
    let Value::Concrete(ConcreteValue::Map(map)) = value else {
        return Err(ProviderError::invalid_input("filter must be a map").for_resource(id.clone()));
    };
    let mut builder = ReplicationRuleFilter::builder();
    if let Some(Value::Concrete(ConcreteValue::String(s))) = map.get("prefix") {
        builder = builder.prefix(s);
    }
    if let Some(v) = map.get("tag") {
        builder = builder.tag(build_filter_tag(id, v)?);
    }
    if let Some(v) = map.get("and") {
        builder = builder.and(build_filter_and(id, v)?);
    }
    Ok(builder.build())
}

fn build_destination(id: &ResourceId, value: &Value) -> ProviderResult<Destination> {
    let Value::Concrete(ConcreteValue::Map(map)) = value else {
        return Err(
            ProviderError::invalid_input("destination must be a map").for_resource(id.clone())
        );
    };
    let bucket = match map.get("bucket") {
        Some(Value::Concrete(ConcreteValue::String(s))) => s.clone(),
        _ => {
            return Err(
                ProviderError::invalid_input("destination.bucket is required")
                    .for_resource(id.clone()),
            );
        }
    };
    let mut builder = Destination::builder().bucket(bucket);
    if let Some(Value::Concrete(ConcreteValue::String(s))) = map.get("account") {
        builder = builder.account(s);
    }
    if let Some(Value::Concrete(ConcreteValue::String(s))) = map.get("storage_class") {
        builder = builder.storage_class(StorageClass::from(s.as_str()));
    }
    builder.build().map_err(|e| {
        ProviderError::api_error(sdk_error_message("Failed to build Destination", &e))
            .for_resource(id.clone())
    })
}

fn rule_to_value(rule: &ReplicationRule) -> Value {
    let mut map = IndexMap::new();
    if let Some(s) = rule.id()
        && !s.is_empty()
    {
        map.insert(
            "id".to_string(),
            Value::Concrete(ConcreteValue::String(s.to_string())),
        );
    }
    // Surface priority only when non-zero. The provider defaults an
    // omitted priority to 0, so reflecting 0 would diff against a DSL
    // config that omits `priority`.
    if let Some(p) = rule.priority()
        && p != 0
    {
        map.insert(
            "priority".to_string(),
            Value::Concrete(ConcreteValue::Int(p as i64)),
        );
    }
    // Only surface a non-empty filter. S3 returns an empty `<Filter/>`
    // for match-all rules; reflecting that as an empty map would diff
    // against a DSL config that simply omits `filter`.
    if let Some(filter) = rule.filter()
        && let Some(filter_value) = filter_to_value(filter)
    {
        map.insert("filter".to_string(), filter_value);
    }
    map.insert(
        "status".to_string(),
        Value::Concrete(ConcreteValue::String(rule.status().as_str().to_string())),
    );
    // Only surface delete_marker_replication when it is Enabled. The
    // provider defaults it to Disabled, so reflecting Disabled would
    // diff against a DSL config that omits it.
    if let Some(dm) = rule.delete_marker_replication()
        && let Some(status) = dm.status()
        && *status == DeleteMarkerReplicationStatus::Enabled
    {
        let mut m = IndexMap::new();
        m.insert(
            "status".to_string(),
            Value::Concrete(ConcreteValue::String(status.as_str().to_string())),
        );
        map.insert(
            "delete_marker_replication".to_string(),
            Value::Concrete(ConcreteValue::Map(m)),
        );
    }
    if let Some(dest) = rule.destination() {
        let mut d = IndexMap::new();
        d.insert(
            "bucket".to_string(),
            Value::Concrete(ConcreteValue::String(dest.bucket().to_string())),
        );
        if let Some(a) = dest.account() {
            d.insert(
                "account".to_string(),
                Value::Concrete(ConcreteValue::String(a.to_string())),
            );
        }
        if let Some(sc) = dest.storage_class() {
            d.insert(
                "storage_class".to_string(),
                Value::Concrete(ConcreteValue::String(sc.as_str().to_string())),
            );
        }
        map.insert(
            "destination".to_string(),
            Value::Concrete(ConcreteValue::Map(d)),
        );
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

/// Convert a `ReplicationRuleAndOperator` into a map value, or `None`
/// when every field is empty.
fn filter_and_to_value(and: &ReplicationRuleAndOperator) -> Option<Value> {
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
    if m.is_empty() {
        None
    } else {
        Some(Value::Concrete(ConcreteValue::Map(m)))
    }
}

/// Convert a `ReplicationRuleFilter` into a map value, or `None` when the
/// filter is empty (a match-all `<Filter/>`).
fn filter_to_value(filter: &ReplicationRuleFilter) -> Option<Value> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn rid() -> ResourceId {
        ResourceId::new("s3.bucket_replication_configuration", "test")
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

    fn dest() -> Value {
        map_val(vec![("bucket", str_val("arn:aws:s3:::dest-bucket"))])
    }

    #[test]
    fn rule_without_filter_gets_empty_filter() {
        // Reproduces carina-rs/carina-provider-aws#273: a V2 replication
        // rule with no `filter` must still emit a `Filter` element,
        // otherwise S3 rejects the request with `MalformedXML`.
        let rule_value = map_val(vec![
            ("id", str_val("replicate-all")),
            ("status", str_val("Enabled")),
            ("destination", dest()),
        ]);
        let rule = build_rule(&rid(), &rule_value).expect("build_rule should succeed");
        assert!(
            rule.filter().is_some(),
            "a replication rule must always carry a Filter element"
        );
    }

    #[test]
    fn rule_without_delete_marker_replication_defaults_to_disabled() {
        // A Filter-based replication rule must also declare
        // DeleteMarkerReplication; default it to Disabled.
        let rule_value = map_val(vec![
            ("status", str_val("Enabled")),
            ("destination", dest()),
        ]);
        let rule = build_rule(&rid(), &rule_value).expect("build_rule should succeed");
        let status = rule
            .delete_marker_replication()
            .and_then(|d| d.status())
            .expect("delete_marker_replication set");
        assert_eq!(*status, DeleteMarkerReplicationStatus::Disabled);
    }

    #[test]
    fn rule_with_explicit_delete_marker_replication() {
        let rule_value = map_val(vec![
            ("status", str_val("Enabled")),
            ("destination", dest()),
            (
                "delete_marker_replication",
                map_val(vec![("status", str_val("Enabled"))]),
            ),
        ]);
        let rule = build_rule(&rid(), &rule_value).expect("build_rule should succeed");
        let status = rule
            .delete_marker_replication()
            .and_then(|d| d.status())
            .expect("delete_marker_replication set");
        assert_eq!(*status, DeleteMarkerReplicationStatus::Enabled);
    }

    #[test]
    fn rule_with_prefix_filter() {
        let rule_value = map_val(vec![
            ("status", str_val("Enabled")),
            ("destination", dest()),
            ("filter", map_val(vec![("prefix", str_val("docs/"))])),
        ]);
        let rule = build_rule(&rid(), &rule_value).expect("build_rule should succeed");
        assert_eq!(rule.filter().and_then(|f| f.prefix()), Some("docs/"));
    }

    #[test]
    fn rule_with_and_filter() {
        let rule_value = map_val(vec![
            ("status", str_val("Enabled")),
            ("destination", dest()),
            (
                "filter",
                map_val(vec![(
                    "and",
                    map_val(vec![
                        ("prefix", str_val("data/")),
                        (
                            "tags",
                            Value::Concrete(ConcreteValue::List(vec![map_val(vec![
                                ("key", str_val("replicate")),
                                ("value", str_val("yes")),
                            ])])),
                        ),
                    ]),
                )]),
            ),
        ]);
        let rule = build_rule(&rid(), &rule_value).expect("build_rule should succeed");
        let and = rule.filter().and_then(|f| f.and()).expect("and set");
        assert_eq!(and.prefix(), Some("data/"));
        assert_eq!(and.tags().len(), 1);
    }

    #[test]
    fn empty_filter_is_not_surfaced_on_read() {
        let empty = ReplicationRuleFilter::builder().build();
        assert!(filter_to_value(&empty).is_none());
    }

    #[test]
    fn disabled_delete_marker_not_surfaced_on_read() {
        // The provider defaults delete_marker_replication to Disabled;
        // reflecting Disabled on read would diff against a DSL that
        // omits it. Only Enabled should round-trip.
        let built = build_rule(
            &rid(),
            &map_val(vec![
                ("status", str_val("Enabled")),
                ("destination", dest()),
            ]),
        )
        .unwrap();
        let value = rule_to_value(&built);
        let Value::Concrete(ConcreteValue::Map(m)) = value else {
            panic!("expected map");
        };
        assert!(
            !m.contains_key("delete_marker_replication"),
            "Disabled delete_marker_replication must not be surfaced"
        );
    }

    #[test]
    fn rule_without_priority_defaults_to_zero() {
        // Reproduces carina-rs/carina-provider-aws#349: a V2 replication
        // rule (one carrying a Filter) must declare a Priority, or S3
        // rejects the request with `InvalidRequest: Priority must be
        // specified`. build_rule must always set one.
        let rule_value = map_val(vec![
            ("status", str_val("Enabled")),
            ("destination", dest()),
        ]);
        let rule = build_rule(&rid(), &rule_value).expect("build_rule should succeed");
        assert_eq!(
            rule.priority(),
            Some(0),
            "an omitted priority must default to 0, not stay unset"
        );
    }

    #[test]
    fn explicit_priority_is_kept() {
        let rule_value = map_val(vec![
            ("status", str_val("Enabled")),
            ("destination", dest()),
            ("priority", Value::Concrete(ConcreteValue::Int(5))),
        ]);
        let rule = build_rule(&rid(), &rule_value).expect("build_rule should succeed");
        assert_eq!(rule.priority(), Some(5));
    }

    #[test]
    fn zero_priority_not_surfaced_on_read() {
        // The provider defaults an omitted priority to 0; reflecting 0
        // on read would diff against a DSL config that omits `priority`.
        let built = build_rule(
            &rid(),
            &map_val(vec![
                ("status", str_val("Enabled")),
                ("destination", dest()),
            ]),
        )
        .unwrap();
        let value = rule_to_value(&built);
        let Value::Concrete(ConcreteValue::Map(m)) = value else {
            panic!("expected map");
        };
        assert!(
            !m.contains_key("priority"),
            "a defaulted priority of 0 must not be surfaced"
        );
    }

    #[test]
    fn nonzero_priority_round_trips() {
        let built = build_rule(
            &rid(),
            &map_val(vec![
                ("status", str_val("Enabled")),
                ("destination", dest()),
                ("priority", Value::Concrete(ConcreteValue::Int(3))),
            ]),
        )
        .unwrap();
        let value = rule_to_value(&built);
        let Value::Concrete(ConcreteValue::Map(m)) = value else {
            panic!("expected map");
        };
        assert_eq!(
            m.get("priority"),
            Some(&Value::Concrete(ConcreteValue::Int(3)))
        );
    }
}
