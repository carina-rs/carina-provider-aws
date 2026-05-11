use std::collections::HashMap;

use aws_sdk_s3::types::{
    ServerSideEncryption, ServerSideEncryptionByDefault, ServerSideEncryptionConfiguration,
    ServerSideEncryptionRule,
};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};
use indexmap::IndexMap;

use crate::AwsProvider;
use crate::helpers::{require_string_attr, sdk_error_message};
use crate::services::s3::bucket::is_s3_not_configured_error;

impl AwsProvider {
    pub(crate) async fn read_s3_bucket_server_side_encryption_configuration(
        &self,
        id: &ResourceId,
        identifier: Option<&str>,
    ) -> ProviderResult<State> {
        let Some(bucket) = identifier else {
            return Ok(State::not_found(id.clone()));
        };

        let result = self
            .s3_client
            .get_bucket_encryption()
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
                if let Some(config) = output.server_side_encryption_configuration() {
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
                if is_s3_not_configured_error(&e, "ServerSideEncryptionConfigurationNotFoundError")
                    || is_s3_not_configured_error(&e, "NoSuchBucket")
                {
                    return Ok(State::not_found(id.clone()));
                }
                Err(ProviderError::api_error(sdk_error_message(
                    "Failed to get bucket encryption",
                    &e,
                ))
                .for_resource(id.clone()))
            }
        }
    }

    pub(crate) async fn create_s3_bucket_server_side_encryption_configuration(
        &self,
        resource: Resource,
    ) -> ProviderResult<State> {
        let bucket = require_string_attr(&resource, "bucket")?;
        self.put_s3_bucket_encryption(&resource.id, &bucket, &resource)
            .await
    }

    pub(crate) async fn update_s3_bucket_server_side_encryption_configuration(
        &self,
        id: ResourceId,
        identifier: &str,
        _from: &State,
        to: Resource,
    ) -> ProviderResult<State> {
        self.put_s3_bucket_encryption(&id, identifier, &to).await
    }

    async fn put_s3_bucket_encryption(
        &self,
        id: &ResourceId,
        bucket: &str,
        resource: &Resource,
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

        let sdk_rules: Vec<ServerSideEncryptionRule> = rules
            .iter()
            .map(|v| build_rule(id, v))
            .collect::<ProviderResult<Vec<_>>>()?;

        let config = ServerSideEncryptionConfiguration::builder()
            .set_rules(Some(sdk_rules))
            .build()
            .map_err(|e| {
                ProviderError::api_error(sdk_error_message(
                    "Failed to build server-side encryption configuration",
                    &e,
                ))
                .for_resource(id.clone())
            })?;

        self.s3_client
            .put_bucket_encryption()
            .bucket(bucket)
            .server_side_encryption_configuration(config)
            .send()
            .await
            .map_err(|e| {
                ProviderError::api_error(sdk_error_message("Failed to put bucket encryption", &e))
                    .for_resource(id.clone())
            })?;

        self.read_s3_bucket_server_side_encryption_configuration(id, Some(bucket))
            .await
    }

    pub(crate) async fn delete_s3_bucket_server_side_encryption_configuration_idempotent(
        &self,
        id: ResourceId,
        identifier: &str,
    ) -> ProviderResult<()> {
        let result = self
            .s3_client
            .delete_bucket_encryption()
            .bucket(identifier)
            .send()
            .await;

        match result {
            Ok(_) => Ok(()),
            Err(e)
                if is_s3_not_configured_error(
                    &e,
                    "ServerSideEncryptionConfigurationNotFoundError",
                ) || is_s3_not_configured_error(&e, "NoSuchBucket") =>
            {
                Ok(())
            }
            Err(e) => Err(ProviderError::api_error(sdk_error_message(
                "Failed to delete bucket encryption",
                &e,
            ))
            .for_resource(id.clone())),
        }
    }
}

fn build_rule(id: &ResourceId, rule_value: &Value) -> ProviderResult<ServerSideEncryptionRule> {
    let Value::Concrete(ConcreteValue::Map(rule_map)) = rule_value else {
        return Err(
            ProviderError::invalid_input("each rule must be a map").for_resource(id.clone())
        );
    };

    let mut builder = ServerSideEncryptionRule::builder();

    if let Some(by_default) = rule_map.get("apply_server_side_encryption_by_default") {
        builder =
            builder.apply_server_side_encryption_by_default(build_by_default(id, by_default)?);
    }
    if let Some(Value::Concrete(ConcreteValue::Bool(b))) = rule_map.get("bucket_key_enabled") {
        builder = builder.bucket_key_enabled(*b);
    }

    Ok(builder.build())
}

fn build_by_default(
    id: &ResourceId,
    value: &Value,
) -> ProviderResult<ServerSideEncryptionByDefault> {
    let Value::Concrete(ConcreteValue::Map(map)) = value else {
        return Err(ProviderError::invalid_input(
            "apply_server_side_encryption_by_default must be a map",
        )
        .for_resource(id.clone()));
    };
    let algorithm_str = match map.get("sse_algorithm") {
        Some(Value::Concrete(ConcreteValue::String(s))) => s.clone(),
        _ => {
            return Err(
                ProviderError::invalid_input("sse_algorithm is required").for_resource(id.clone())
            );
        }
    };
    let mut builder = ServerSideEncryptionByDefault::builder()
        .sse_algorithm(ServerSideEncryption::from(algorithm_str.as_str()));
    if let Some(Value::Concrete(ConcreteValue::String(k))) = map.get("kms_master_key_id") {
        builder = builder.kms_master_key_id(k);
    }
    builder.build().map_err(|e| {
        ProviderError::api_error(sdk_error_message(
            "Failed to build apply_server_side_encryption_by_default",
            &e,
        ))
        .for_resource(id.clone())
    })
}

fn rule_to_value(rule: &ServerSideEncryptionRule) -> Value {
    let mut map = IndexMap::new();
    if let Some(by_default) = rule.apply_server_side_encryption_by_default() {
        let mut inner = IndexMap::new();
        inner.insert(
            "sse_algorithm".to_string(),
            Value::Concrete(ConcreteValue::String(
                by_default.sse_algorithm().as_str().to_string(),
            )),
        );
        if let Some(k) = by_default.kms_master_key_id() {
            inner.insert(
                "kms_master_key_id".to_string(),
                Value::Concrete(ConcreteValue::String(k.to_string())),
            );
        }
        map.insert(
            "apply_server_side_encryption_by_default".to_string(),
            Value::Concrete(ConcreteValue::Map(inner)),
        );
    }
    if let Some(b) = rule.bucket_key_enabled() {
        map.insert(
            "bucket_key_enabled".to_string(),
            Value::Concrete(ConcreteValue::Bool(b)),
        );
    }
    Value::Concrete(ConcreteValue::Map(map))
}
