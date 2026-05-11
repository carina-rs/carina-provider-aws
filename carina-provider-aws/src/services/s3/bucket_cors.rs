use std::collections::HashMap;

use aws_sdk_s3::types::{CorsConfiguration, CorsRule};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};
use indexmap::IndexMap;

use crate::AwsProvider;
use crate::helpers::{require_string_attr, sdk_error_message};
use crate::services::s3::bucket::is_s3_not_configured_error;

impl AwsProvider {
    pub(crate) async fn read_s3_bucket_cors_configuration(
        &self,
        id: &ResourceId,
        identifier: Option<&str>,
    ) -> ProviderResult<State> {
        let Some(bucket) = identifier else {
            return Ok(State::not_found(id.clone()));
        };

        let result = self.s3_client.get_bucket_cors().bucket(bucket).send().await;

        match result {
            Ok(output) => {
                let mut attributes = HashMap::new();
                attributes.insert(
                    "bucket".to_string(),
                    Value::Concrete(ConcreteValue::String(bucket.to_string())),
                );
                let rules: Vec<Value> = output.cors_rules().iter().map(rule_to_value).collect();
                attributes.insert(
                    "cors_rules".to_string(),
                    Value::Concrete(ConcreteValue::List(rules)),
                );
                Ok(State::existing(id.clone(), attributes).with_identifier(bucket.to_string()))
            }
            Err(e) => {
                if is_s3_not_configured_error(&e, "NoSuchCORSConfiguration")
                    || is_s3_not_configured_error(&e, "NoSuchBucket")
                {
                    return Ok(State::not_found(id.clone()));
                }
                Err(ProviderError::api_error(sdk_error_message(
                    "Failed to get bucket cors configuration",
                    &e,
                ))
                .for_resource(id.clone()))
            }
        }
    }

    pub(crate) async fn create_s3_bucket_cors_configuration(
        &self,
        resource: Resource,
    ) -> ProviderResult<State> {
        let bucket = require_string_attr(&resource, "bucket")?;
        self.put_s3_bucket_cors(&resource.id, &bucket, &resource)
            .await
    }

    pub(crate) async fn update_s3_bucket_cors_configuration(
        &self,
        id: ResourceId,
        identifier: &str,
        _from: &State,
        to: Resource,
    ) -> ProviderResult<State> {
        self.put_s3_bucket_cors(&id, identifier, &to).await
    }

    async fn put_s3_bucket_cors(
        &self,
        id: &ResourceId,
        bucket: &str,
        resource: &Resource,
    ) -> ProviderResult<State> {
        let rules = match resource.get_attr("cors_rules") {
            Some(Value::Concrete(ConcreteValue::List(items))) => items,
            _ => {
                return Err(ProviderError::invalid_input(
                    "cors_rules is required and must be a list",
                )
                .for_resource(id.clone()));
            }
        };

        let sdk_rules: Vec<CorsRule> = rules
            .iter()
            .map(|v| build_rule(id, v))
            .collect::<ProviderResult<Vec<_>>>()?;

        let config = CorsConfiguration::builder()
            .set_cors_rules(Some(sdk_rules))
            .build()
            .map_err(|e| {
                ProviderError::api_error(sdk_error_message("Failed to build CorsConfiguration", &e))
                    .for_resource(id.clone())
            })?;

        self.s3_client
            .put_bucket_cors()
            .bucket(bucket)
            .cors_configuration(config)
            .send()
            .await
            .map_err(|e| {
                ProviderError::api_error(sdk_error_message("Failed to put bucket cors", &e))
                    .for_resource(id.clone())
            })?;

        self.read_s3_bucket_cors_configuration(id, Some(bucket))
            .await
    }

    pub(crate) async fn delete_s3_bucket_cors_configuration_idempotent(
        &self,
        id: ResourceId,
        identifier: &str,
    ) -> ProviderResult<()> {
        let result = self
            .s3_client
            .delete_bucket_cors()
            .bucket(identifier)
            .send()
            .await;

        match result {
            Ok(_) => Ok(()),
            Err(e)
                if is_s3_not_configured_error(&e, "NoSuchCORSConfiguration")
                    || is_s3_not_configured_error(&e, "NoSuchBucket") =>
            {
                Ok(())
            }
            Err(e) => Err(ProviderError::api_error(sdk_error_message(
                "Failed to delete bucket cors configuration",
                &e,
            ))
            .for_resource(id.clone())),
        }
    }
}

fn build_rule(id: &ResourceId, rule_value: &Value) -> ProviderResult<CorsRule> {
    let Value::Concrete(ConcreteValue::Map(map)) = rule_value else {
        return Err(
            ProviderError::invalid_input("each cors rule must be a map").for_resource(id.clone())
        );
    };

    let allowed_methods = require_string_list(id, map, "allowed_methods")?;
    let allowed_origins = require_string_list(id, map, "allowed_origins")?;

    let mut builder = CorsRule::builder()
        .set_allowed_methods(Some(allowed_methods))
        .set_allowed_origins(Some(allowed_origins));

    if let Some(Value::Concrete(ConcreteValue::String(s))) = map.get("id") {
        builder = builder.id(s);
    }
    if let Some(Value::Concrete(ConcreteValue::List(items))) = map.get("allowed_headers") {
        builder =
            builder.set_allowed_headers(Some(strings_from_list(id, items, "allowed_headers")?));
    }
    if let Some(Value::Concrete(ConcreteValue::List(items))) = map.get("expose_headers") {
        builder = builder.set_expose_headers(Some(strings_from_list(id, items, "expose_headers")?));
    }
    if let Some(Value::Concrete(ConcreteValue::Int(n))) = map.get("max_age_seconds") {
        builder = builder.max_age_seconds(*n as i32);
    }

    builder.build().map_err(|e| {
        ProviderError::api_error(sdk_error_message("Failed to build CorsRule", &e))
            .for_resource(id.clone())
    })
}

fn require_string_list(
    id: &ResourceId,
    map: &IndexMap<String, Value>,
    field: &str,
) -> ProviderResult<Vec<String>> {
    match map.get(field) {
        Some(Value::Concrete(ConcreteValue::List(items))) => strings_from_list(id, items, field),
        _ => Err(
            ProviderError::invalid_input(format!("cors_rule.{field} is required"))
                .for_resource(id.clone()),
        ),
    }
}

fn strings_from_list(id: &ResourceId, items: &[Value], field: &str) -> ProviderResult<Vec<String>> {
    items
        .iter()
        .map(|v| match v {
            Value::Concrete(ConcreteValue::String(s)) => Ok(s.clone()),
            _ => Err(ProviderError::invalid_input(format!(
                "cors_rule.{field} must be a list of strings"
            ))
            .for_resource(id.clone())),
        })
        .collect()
}

fn rule_to_value(rule: &CorsRule) -> Value {
    let mut m = IndexMap::new();
    if let Some(s) = rule.id()
        && !s.is_empty()
    {
        m.insert(
            "id".to_string(),
            Value::Concrete(ConcreteValue::String(s.to_string())),
        );
    }
    let methods: Vec<Value> = rule
        .allowed_methods()
        .iter()
        .map(|s| Value::Concrete(ConcreteValue::String(s.clone())))
        .collect();
    m.insert(
        "allowed_methods".to_string(),
        Value::Concrete(ConcreteValue::List(methods)),
    );
    let origins: Vec<Value> = rule
        .allowed_origins()
        .iter()
        .map(|s| Value::Concrete(ConcreteValue::String(s.clone())))
        .collect();
    m.insert(
        "allowed_origins".to_string(),
        Value::Concrete(ConcreteValue::List(origins)),
    );
    let headers: Vec<Value> = rule
        .allowed_headers()
        .iter()
        .map(|s| Value::Concrete(ConcreteValue::String(s.clone())))
        .collect();
    if !headers.is_empty() {
        m.insert(
            "allowed_headers".to_string(),
            Value::Concrete(ConcreteValue::List(headers)),
        );
    }
    let expose: Vec<Value> = rule
        .expose_headers()
        .iter()
        .map(|s| Value::Concrete(ConcreteValue::String(s.clone())))
        .collect();
    if !expose.is_empty() {
        m.insert(
            "expose_headers".to_string(),
            Value::Concrete(ConcreteValue::List(expose)),
        );
    }
    if let Some(n) = rule.max_age_seconds() {
        m.insert(
            "max_age_seconds".to_string(),
            Value::Concrete(ConcreteValue::Int(n as i64)),
        );
    }
    Value::Concrete(ConcreteValue::Map(m))
}
