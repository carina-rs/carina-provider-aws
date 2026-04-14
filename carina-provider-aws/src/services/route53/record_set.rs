//! Route 53 RecordSet service implementation.
//!
//! Uses ChangeResourceRecordSets (UPSERT/DELETE) and ListResourceRecordSets
//! for CRUD operations. Cloud Control does not support Route 53 records.

use std::collections::HashMap;

use aws_sdk_route53::types::{
    AliasTarget, Change, ChangeAction, ChangeBatch, ResourceRecord, ResourceRecordSet, RrType,
};

use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{Resource, ResourceId, State, Value};

use crate::AwsProvider;
use crate::helpers::{require_string_attr, sdk_error_message};

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
    if let Value::String(s) = value {
        Some(s.as_str())
    } else {
        None
    }
}

fn build_resource_records(records: &[Value]) -> Vec<ResourceRecord> {
    records
        .iter()
        .filter_map(|v| {
            if let Value::String(s) = v {
                ResourceRecord::builder().value(s.clone()).build().ok()
            } else {
                None
            }
        })
        .collect()
}

fn build_alias_target_from_map(
    alias: &HashMap<String, Value>,
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
            if let Value::Bool(b) = v {
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
            ProviderError::new(sdk_error_message("Invalid alias_target", &e))
                .for_resource(id.clone())
        })
}

/// Build an AWS SDK ResourceRecordSet from carina resource attributes.
fn build_record_set(resource: &Resource) -> ProviderResult<ResourceRecordSet> {
    let name = require_string_attr(resource, "name")?;
    let record_type = require_string_attr(resource, "type")?;

    let mut builder = ResourceRecordSet::builder()
        .name(normalize_dns_name(&name))
        .r#type(RrType::from(record_type.as_str()));

    if let Some(Value::Int(ttl)) = resource.get_attr("ttl") {
        builder = builder.ttl(*ttl);
    }

    if let Some(Value::List(records)) = resource.get_attr("resource_records") {
        builder = builder.set_resource_records(Some(build_resource_records(records)));
    }

    if let Some(Value::Map(alias)) = resource.get_attr("alias_target") {
        builder = builder.alias_target(build_alias_target_from_map(alias, &resource.id)?);
    }

    builder.build().map_err(|e| {
        ProviderError::new(sdk_error_message("Failed to build ResourceRecordSet", &e))
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
            ProviderError::new(sdk_error_message("Failed to build change", &e))
                .for_resource(id.clone())
        })?;

    let batch = ChangeBatch::builder()
        .changes(change)
        .build()
        .map_err(|e| {
            ProviderError::new(sdk_error_message("Failed to build change batch", &e))
                .for_resource(id.clone())
        })?;

    client
        .change_resource_record_sets()
        .hosted_zone_id(hosted_zone_id)
        .change_batch(batch)
        .send()
        .await
        .map_err(|e| {
            ProviderError::new(sdk_error_message("ChangeResourceRecordSets failed", &e))
                .for_resource(id.clone())
        })?;

    Ok(())
}

/// Extract carina attributes from an AWS SDK ResourceRecordSet.
fn extract_attributes(hosted_zone_id: &str, rrs: &ResourceRecordSet) -> HashMap<String, Value> {
    let mut attrs = HashMap::new();

    attrs.insert(
        "hosted_zone_id".to_string(),
        Value::String(hosted_zone_id.to_string()),
    );

    attrs.insert(
        "name".to_string(),
        Value::String(strip_trailing_dot(rrs.name())),
    );

    attrs.insert(
        "type".to_string(),
        Value::String(rrs.r#type().as_str().to_string()),
    );

    if let Some(ttl) = rrs.ttl() {
        attrs.insert("ttl".to_string(), Value::Int(ttl));
    }

    let records: Vec<Value> = rrs
        .resource_records()
        .iter()
        .map(|r| Value::String(r.value().to_string()))
        .collect();
    if !records.is_empty() {
        attrs.insert("resource_records".to_string(), Value::List(records));
    }

    if let Some(alias) = rrs.alias_target() {
        let mut alias_map = HashMap::new();
        alias_map.insert(
            "dns_name".to_string(),
            Value::String(strip_trailing_dot(alias.dns_name())),
        );
        alias_map.insert(
            "hosted_zone_id".to_string(),
            Value::String(alias.hosted_zone_id().to_string()),
        );
        alias_map.insert(
            "evaluate_target_health".to_string(),
            Value::Bool(alias.evaluate_target_health()),
        );
        attrs.insert("alias_target".to_string(), Value::Map(alias_map));
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
                ProviderError::new(sdk_error_message("Failed to list record sets", &e))
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
        let record_type = require_string_attr(&resource, "type")?;

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
        let record_type = require_string_attr(&to, "type")?;

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
            return Err(ProviderError::new(format!(
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

        if let Some(Value::Int(ttl)) = current.attributes.get("ttl") {
            builder = builder.ttl(*ttl);
        }

        if let Some(Value::List(records)) = current.attributes.get("resource_records") {
            builder = builder.set_resource_records(Some(build_resource_records(records)));
        }

        if let Some(Value::Map(alias)) = current.attributes.get("alias_target") {
            builder = builder.alias_target(build_alias_target_from_map(alias, &id)?);
        }

        let record_set = builder.build().map_err(|e| {
            ProviderError::new(sdk_error_message(
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
