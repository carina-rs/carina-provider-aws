//! Route 53 RecordSet service implementation.
//!
//! Uses ChangeResourceRecordSets (UPSERT/DELETE) and ListResourceRecordSets
//! for CRUD operations. Cloud Control does not support Route 53 records.

use indexmap::IndexMap;
use std::collections::HashMap;

use aws_sdk_route53::types::{
    AliasTarget, Change, ChangeAction, ChangeBatch, ResourceRecord, ResourceRecordSet, RrType,
};

use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};

use crate::AwsProvider;
use crate::helpers::{require_enum_attr, require_string_attr, sdk_error_message};

/// Composite identifier format: `hosted_zone_id|name|type`
fn make_identifier(hosted_zone_id: &str, name: &str, record_type: &str) -> String {
    format!("{}|{}|{}", hosted_zone_id, name, record_type)
}

fn parse_identifier(identifier: &str) -> Option<(&str, &str, &str)> {
    let mut parts = identifier.splitn(3, '|');
    let zone_id = parts.next()?;
    let name = parts.next()?;
    let record_type = parts.next()?;
    Some((zone_id, name, record_type))
}

/// Normalize a DNS name by ensuring it has a trailing dot (Route 53 convention).
fn normalize_dns_name(name: &str) -> String {
    if name.ends_with('.') {
        name.to_string()
    } else {
        format!("{}.", name)
    }
}

/// Strip the trailing dot for display/comparison with user input.
fn strip_trailing_dot(name: &str) -> String {
    name.strip_suffix('.').unwrap_or(name).to_string()
}

fn extract_string(value: &Value) -> Option<&str> {
    if let Value::Concrete(ConcreteValue::String(s)) = value {
        Some(s.as_str())
    } else {
        None
    }
}

fn build_resource_records(records: &[Value]) -> Vec<ResourceRecord> {
    records
        .iter()
        .filter_map(|v| {
            if let Value::Concrete(ConcreteValue::String(s)) = v {
                ResourceRecord::builder().value(s.clone()).build().ok()
            } else {
                None
            }
        })
        .collect()
}

fn build_alias_target_from_map(
    alias: &IndexMap<String, Value>,
    id: &ResourceId,
) -> ProviderResult<AliasTarget> {
    let dns_name = alias
        .get("dns_name")
        .and_then(extract_string)
        .unwrap_or_default();
    let zone_id = alias
        .get("hosted_zone_id")
        .and_then(extract_string)
        .unwrap_or_default();
    let evaluate = alias
        .get("evaluate_target_health")
        .and_then(|v| {
            if let Value::Concrete(ConcreteValue::Bool(b)) = v {
                Some(*b)
            } else {
                None
            }
        })
        .unwrap_or(false);

    AliasTarget::builder()
        .dns_name(dns_name)
        .hosted_zone_id(zone_id)
        .evaluate_target_health(evaluate)
        .build()
        .map_err(|e| {
            ProviderError::api_error(sdk_error_message("Invalid alias_target", &e))
                .for_resource(id.clone())
        })
}

/// Build an AWS SDK ResourceRecordSet from carina resource attributes.
fn build_record_set(resource: &Resource) -> ProviderResult<ResourceRecordSet> {
    let name = require_string_attr(resource, "name")?;
    let record_type = require_enum_attr(resource, "type")?;

    let mut builder = ResourceRecordSet::builder()
        .name(normalize_dns_name(&name))
        .r#type(RrType::from(record_type.as_str()));

    if let Some(Value::Concrete(ConcreteValue::Int(ttl))) = resource.get_attr("ttl") {
        builder = builder.ttl(*ttl);
    }

    if let Some(Value::Concrete(ConcreteValue::List(records))) =
        resource.get_attr("resource_records")
    {
        builder = builder.set_resource_records(Some(build_resource_records(records)));
    }

    if let Some(Value::Concrete(ConcreteValue::Map(alias))) = resource.get_attr("alias_target") {
        builder = builder.alias_target(build_alias_target_from_map(alias, &resource.id)?);
    }

    builder.build().map_err(|e| {
        ProviderError::api_error(sdk_error_message("Failed to build ResourceRecordSet", &e))
            .for_resource(resource.id.clone())
    })
}

/// Execute a ChangeResourceRecordSets UPSERT or DELETE.
async fn change_record_set(
    client: &aws_sdk_route53::Client,
    hosted_zone_id: &str,
    action: ChangeAction,
    record_set: ResourceRecordSet,
    id: &ResourceId,
) -> ProviderResult<()> {
    let change = Change::builder()
        .action(action)
        .resource_record_set(record_set)
        .build()
        .map_err(|e| {
            ProviderError::api_error(sdk_error_message("Failed to build change", &e))
                .for_resource(id.clone())
        })?;

    let batch = ChangeBatch::builder()
        .changes(change)
        .build()
        .map_err(|e| {
            ProviderError::api_error(sdk_error_message("Failed to build change batch", &e))
                .for_resource(id.clone())
        })?;

    client
        .change_resource_record_sets()
        .hosted_zone_id(hosted_zone_id)
        .change_batch(batch)
        .send()
        .await
        .map_err(|e| {
            ProviderError::api_error(sdk_error_message("ChangeResourceRecordSets failed", &e))
                .for_resource(id.clone())
        })?;

    Ok(())
}

/// Extract carina attributes from an AWS SDK ResourceRecordSet.
fn extract_attributes(hosted_zone_id: &str, rrs: &ResourceRecordSet) -> HashMap<String, Value> {
    let mut attrs = HashMap::new();

    attrs.insert(
        "hosted_zone_id".to_string(),
        Value::Concrete(ConcreteValue::String(hosted_zone_id.to_string())),
    );

    attrs.insert(
        "name".to_string(),
        Value::Concrete(ConcreteValue::String(strip_trailing_dot(rrs.name()))),
    );

    attrs.insert(
        "type".to_string(),
        Value::Concrete(ConcreteValue::String(rrs.r#type().as_str().to_string())),
    );

    if let Some(ttl) = rrs.ttl() {
        attrs.insert("ttl".to_string(), Value::Concrete(ConcreteValue::Int(ttl)));
    }

    let records: Vec<Value> = rrs
        .resource_records()
        .iter()
        .map(|r| Value::Concrete(ConcreteValue::String(r.value().to_string())))
        .collect();
    if !records.is_empty() {
        attrs.insert(
            "resource_records".to_string(),
            Value::Concrete(ConcreteValue::List(records)),
        );
    }

    if let Some(alias) = rrs.alias_target() {
        let mut alias_map: IndexMap<String, Value> = IndexMap::new();
        alias_map.insert(
            "dns_name".to_string(),
            Value::Concrete(ConcreteValue::String(strip_trailing_dot(alias.dns_name()))),
        );
        alias_map.insert(
            "hosted_zone_id".to_string(),
            Value::Concrete(ConcreteValue::String(alias.hosted_zone_id().to_string())),
        );
        alias_map.insert(
            "evaluate_target_health".to_string(),
            Value::Concrete(ConcreteValue::Bool(alias.evaluate_target_health())),
        );
        attrs.insert(
            "alias_target".to_string(),
            Value::Concrete(ConcreteValue::Map(alias_map)),
        );
    }

    attrs
}

impl AwsProvider {
    pub(crate) async fn read_route53_record_set(
        &self,
        id: &ResourceId,
        identifier: Option<&str>,
    ) -> ProviderResult<State> {
        let Some(identifier) = identifier else {
            return Ok(State::not_found(id.clone()));
        };
        let Some((zone_id, name, record_type)) = parse_identifier(identifier) else {
            return Ok(State::not_found(id.clone()));
        };

        let normalized_name = normalize_dns_name(name);

        let result = self
            .route53_client
            .list_resource_record_sets()
            .hosted_zone_id(zone_id)
            .start_record_name(&normalized_name)
            .start_record_type(RrType::from(record_type))
            .max_items(1)
            .send()
            .await
            .map_err(|e| {
                ProviderError::api_error(sdk_error_message("Failed to list record sets", &e))
                    .for_resource(id.clone())
            })?;

        for rrs in result.resource_record_sets() {
            let rrs_name = rrs.name();
            let rrs_type = rrs.r#type().as_str();

            if rrs_name == normalized_name && rrs_type == record_type {
                let attrs = extract_attributes(zone_id, rrs);
                return Ok(
                    State::existing(id.clone(), attrs).with_identifier(identifier.to_string())
                );
            }
        }

        Ok(State::not_found(id.clone()))
    }

    pub(crate) async fn create_route53_record_set(
        &self,
        resource: Resource,
    ) -> ProviderResult<State> {
        let hosted_zone_id = require_string_attr(&resource, "hosted_zone_id")?;
        let name = require_string_attr(&resource, "name")?;
        let record_type = require_enum_attr(&resource, "type")?;

        let record_set = build_record_set(&resource)?;
        change_record_set(
            &self.route53_client,
            &hosted_zone_id,
            ChangeAction::Upsert,
            record_set,
            &resource.id,
        )
        .await?;

        let identifier = make_identifier(&hosted_zone_id, &name, &record_type);
        self.read_route53_record_set(&resource.id, Some(&identifier))
            .await
    }

    pub(crate) async fn update_route53_record_set(
        &self,
        id: ResourceId,
        _identifier: &str,
        to: Resource,
    ) -> ProviderResult<State> {
        let hosted_zone_id = require_string_attr(&to, "hosted_zone_id")?;
        let name = require_string_attr(&to, "name")?;
        let record_type = require_enum_attr(&to, "type")?;

        let record_set = build_record_set(&to)?;
        change_record_set(
            &self.route53_client,
            &hosted_zone_id,
            ChangeAction::Upsert,
            record_set,
            &id,
        )
        .await?;

        let identifier = make_identifier(&hosted_zone_id, &name, &record_type);
        self.read_route53_record_set(&id, Some(&identifier)).await
    }

    pub(crate) async fn delete_route53_record_set(
        &self,
        id: ResourceId,
        identifier: &str,
    ) -> ProviderResult<()> {
        let Some((zone_id, name, record_type)) = parse_identifier(identifier) else {
            return Err(ProviderError::invalid_input(format!(
                "Invalid record set identifier: {}",
                identifier
            ))
            .for_resource(id));
        };

        // Read current state to get the exact record for deletion.
        // Route 53 DELETE requires the record to match exactly.
        let current = self.read_route53_record_set(&id, Some(identifier)).await?;
        if current.attributes.is_empty() {
            return Ok(());
        }

        let normalized_name = normalize_dns_name(name);
        let mut builder = ResourceRecordSet::builder()
            .name(&normalized_name)
            .r#type(RrType::from(record_type));

        if let Some(Value::Concrete(ConcreteValue::Int(ttl))) = current.attributes.get("ttl") {
            builder = builder.ttl(*ttl);
        }

        if let Some(Value::Concrete(ConcreteValue::List(records))) =
            current.attributes.get("resource_records")
        {
            builder = builder.set_resource_records(Some(build_resource_records(records)));
        }

        if let Some(Value::Concrete(ConcreteValue::Map(alias))) =
            current.attributes.get("alias_target")
        {
            builder = builder.alias_target(build_alias_target_from_map(alias, &id)?);
        }

        let record_set = builder.build().map_err(|e| {
            ProviderError::api_error(sdk_error_message(
                "Failed to build record set for deletion",
                &e,
            ))
            .for_resource(id.clone())
        })?;

        change_record_set(
            &self.route53_client,
            zone_id,
            ChangeAction::Delete,
            record_set,
            &id,
        )
        .await
    }
}

/// Normalize desired-side `route53.RecordSet` attributes so a trailing
/// dot fed in from another resource's `read` does not produce a spurious
/// `forces replacement` diff against state.
///
/// The read side (`extract_attributes`) already strips trailing dots from
/// `name` and `alias_target.dns_name`. Without symmetric normalization on
/// the desired side, values like `cert.domain_validation_options[0]
/// .resource_record.name` (which the ACM read returns verbatim as
/// `"_abc.example.com."`) compare unequal against the dot-stripped state
/// row, surfacing a replace diff that AWS would not actually require —
/// the apply would just re-UPSERT the same DNS name.
///
/// See aws#300, follow-up to aws#117.
pub(crate) fn normalize_record_set_dns_names(resources: &mut [Resource]) {
    for resource in resources.iter_mut() {
        if resource.id.provider != "aws" || resource.id.resource_type != "route53.RecordSet" {
            continue;
        }

        if let Some(Value::Concrete(ConcreteValue::String(name))) = resource.get_attr("name")
            && let Some(stripped) = name.strip_suffix('.')
        {
            resource.set_attr(
                "name".to_string(),
                Value::Concrete(ConcreteValue::String(stripped.to_string())),
            );
        }

        if let Some(Value::Concrete(ConcreteValue::Map(alias))) = resource.get_attr("alias_target")
            && let Some(Value::Concrete(ConcreteValue::String(dns_name))) = alias.get("dns_name")
            && let Some(stripped) = dns_name.strip_suffix('.')
        {
            let mut new_alias = alias.clone();
            new_alias.insert(
                "dns_name".to_string(),
                Value::Concrete(ConcreteValue::String(stripped.to_string())),
            );
            resource.set_attr(
                "alias_target".to_string(),
                Value::Concrete(ConcreteValue::Map(new_alias)),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use carina_core::resource::Resource;

    fn record_set(name: &str) -> Resource {
        let mut r = Resource::with_provider("aws", "route53.RecordSet", "test-rec");
        r.set_attr(
            "name".to_string(),
            Value::Concrete(ConcreteValue::String(name.to_string())),
        );
        r
    }

    #[test]
    fn strips_trailing_dot_on_name() {
        let mut resources = vec![record_set("_abc.example.com.")];
        normalize_record_set_dns_names(&mut resources);
        assert_eq!(
            resources[0].get_attr("name"),
            Some(&Value::Concrete(ConcreteValue::String(
                "_abc.example.com".to_string()
            )))
        );
    }

    #[test]
    fn leaves_name_without_trailing_dot_untouched() {
        let mut resources = vec![record_set("_abc.example.com")];
        normalize_record_set_dns_names(&mut resources);
        assert_eq!(
            resources[0].get_attr("name"),
            Some(&Value::Concrete(ConcreteValue::String(
                "_abc.example.com".to_string()
            )))
        );
    }

    #[test]
    fn strips_trailing_dot_on_alias_target_dns_name() {
        let mut r = record_set("alias.example.com");
        let mut alias: IndexMap<String, Value> = IndexMap::new();
        alias.insert(
            "dns_name".to_string(),
            Value::Concrete(ConcreteValue::String(
                "dualstack.elb.amazonaws.com.".to_string(),
            )),
        );
        alias.insert(
            "hosted_zone_id".to_string(),
            Value::Concrete(ConcreteValue::String("Z35SXDOTRQ7X7K".to_string())),
        );
        alias.insert(
            "evaluate_target_health".to_string(),
            Value::Concrete(ConcreteValue::Bool(false)),
        );
        r.set_attr(
            "alias_target".to_string(),
            Value::Concrete(ConcreteValue::Map(alias)),
        );
        let mut resources = vec![r];
        normalize_record_set_dns_names(&mut resources);
        let Some(Value::Concrete(ConcreteValue::Map(out))) = resources[0].get_attr("alias_target")
        else {
            panic!("expected alias_target map");
        };
        assert_eq!(
            out.get("dns_name"),
            Some(&Value::Concrete(ConcreteValue::String(
                "dualstack.elb.amazonaws.com".to_string()
            )))
        );
    }

    #[test]
    fn ignores_non_route53_record_set_resources() {
        let mut r = Resource::with_provider("aws", "s3.Bucket", "test");
        r.set_attr(
            "name".to_string(),
            Value::Concrete(ConcreteValue::String(
                "bucket.with.trailing.dot.".to_string(),
            )),
        );
        let mut resources = vec![r];
        normalize_record_set_dns_names(&mut resources);
        assert_eq!(
            resources[0].get_attr("name"),
            Some(&Value::Concrete(ConcreteValue::String(
                "bucket.with.trailing.dot.".to_string()
            )))
        );
    }

    #[test]
    fn ignores_non_aws_provider() {
        let mut r = Resource::with_provider("awscc", "route53.RecordSet", "test");
        r.set_attr(
            "name".to_string(),
            Value::Concrete(ConcreteValue::String("foo.example.com.".to_string())),
        );
        let mut resources = vec![r];
        normalize_record_set_dns_names(&mut resources);
        // unchanged: awscc has its own normalizer
        assert_eq!(
            resources[0].get_attr("name"),
            Some(&Value::Concrete(ConcreteValue::String(
                "foo.example.com.".to_string()
            )))
        );
    }
}
