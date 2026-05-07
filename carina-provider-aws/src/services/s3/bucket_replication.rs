use std::collections::HashMap;

use aws_sdk_s3::types::{
    Destination, ReplicationConfiguration, ReplicationRule, ReplicationRuleStatus, StorageClass,
};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{Resource, ResourceId, State, Value};
use carina_core::utils::extract_enum_value;
use indexmap::IndexMap;

use crate::AwsProvider;
use crate::helpers::{require_string_attr, sdk_error_message};
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
                attributes.insert("bucket".to_string(), Value::String(bucket.to_string()));
                if let Some(config) = output.replication_configuration() {
                    attributes.insert("role".to_string(), Value::String(config.role().to_string()));
                    let rules: Vec<Value> = config.rules().iter().map(rule_to_value).collect();
                    if !rules.is_empty() {
                        attributes.insert("rules".to_string(), Value::List(rules));
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
        resource: Resource,
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
        to: Resource,
    ) -> ProviderResult<State> {
        self.put_s3_bucket_replication(&id, identifier, &to).await
    }

    async fn put_s3_bucket_replication(
        &self,
        id: &ResourceId,
        bucket: &str,
        resource: &Resource,
    ) -> ProviderResult<State> {
        let role = require_string_attr(resource, "role")?;
        let rules = match resource.get_attr("rules") {
            Some(Value::List(items)) => items,
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
        let result = self
            .s3_client
            .delete_bucket_replication()
            .bucket(identifier)
            .send()
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
    let Value::Map(map) = rule_value else {
        return Err(
            ProviderError::invalid_input("each rule must be a map").for_resource(id.clone())
        );
    };

    let status_str = match map.get("status") {
        Some(Value::String(s)) => extract_enum_value(s).to_string(),
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

    let mut builder = ReplicationRule::builder()
        .status(ReplicationRuleStatus::from(status_str.as_str()))
        .destination(destination);

    if let Some(Value::String(s)) = map.get("id") {
        builder = builder.id(s);
    }
    if let Some(Value::Int(n)) = map.get("priority") {
        builder = builder.priority(*n as i32);
    }
    if let Some(Value::String(s)) = map.get("prefix") {
        #[allow(deprecated)]
        {
            builder = builder.prefix(s);
        }
    }

    builder.build().map_err(|e| {
        ProviderError::api_error(sdk_error_message("Failed to build ReplicationRule", &e))
            .for_resource(id.clone())
    })
}

fn build_destination(id: &ResourceId, value: &Value) -> ProviderResult<Destination> {
    let Value::Map(map) = value else {
        return Err(
            ProviderError::invalid_input("destination must be a map").for_resource(id.clone())
        );
    };
    let bucket = match map.get("bucket") {
        Some(Value::String(s)) => s.clone(),
        _ => {
            return Err(
                ProviderError::invalid_input("destination.bucket is required")
                    .for_resource(id.clone()),
            );
        }
    };
    let mut builder = Destination::builder().bucket(bucket);
    if let Some(Value::String(s)) = map.get("account") {
        builder = builder.account(s);
    }
    if let Some(Value::String(s)) = map.get("storage_class") {
        let normalized = extract_enum_value(s);
        builder = builder.storage_class(StorageClass::from(normalized));
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
        map.insert("id".to_string(), Value::String(s.to_string()));
    }
    if let Some(p) = rule.priority() {
        map.insert("priority".to_string(), Value::Int(p as i64));
    }
    #[allow(deprecated)]
    if let Some(s) = rule.prefix()
        && !s.is_empty()
    {
        map.insert("prefix".to_string(), Value::String(s.to_string()));
    }
    map.insert(
        "status".to_string(),
        Value::String(rule.status().as_str().to_string()),
    );
    if let Some(dest) = rule.destination() {
        let mut d = IndexMap::new();
        d.insert(
            "bucket".to_string(),
            Value::String(dest.bucket().to_string()),
        );
        if let Some(a) = dest.account() {
            d.insert("account".to_string(), Value::String(a.to_string()));
        }
        if let Some(sc) = dest.storage_class() {
            d.insert(
                "storage_class".to_string(),
                Value::String(sc.as_str().to_string()),
            );
        }
        map.insert("destination".to_string(), Value::Map(d));
    }
    Value::Map(map)
}
