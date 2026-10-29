use aws_sdk_s3::types::{CorsConfiguration, CorsRule};
use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};
use indexmap::IndexMap;

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{RetryPolicy, require_string_attr, retry_aws_operation, sdk_error_message};
use crate::provider::CreateCompletion;
use crate::read_model::found_managed_read;
use crate::read_models_generated::S3BucketCorsConfigurationReadModel;
use crate::services::s3::bucket::is_s3_not_configured_error;

impl AwsProvider {
    pub(crate) async fn read_s3_bucket_cors_configuration(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        let bucket = identifier.as_str();

        let result = self.s3_client.get_bucket_cors().bucket(bucket).send().await;

        match result {
            Ok(output) => {
                let rules: Vec<Value> = output.cors_rules().iter().map(rule_to_value).collect();
                found_managed_read(
                    id,
                    identifier,
                    S3BucketCorsConfigurationReadModel {
                        cors_rules: Value::Concrete(ConcreteValue::List(rules)),
                    },
                )
            }
            Err(e) => {
                if is_s3_not_configured_error(&e, "NoSuchCORSConfiguration")
                    || is_s3_not_configured_error(&e, "NoSuchBucket")
                {
                    return Ok(ReadOutcome::not_found(id.clone()));
                }
                Err(
                    api_error_with_meta("Failed to get bucket cors configuration", e)
                        .for_resource(id.clone()),
                )
            }
        }
    }

    pub(crate) async fn create_s3_bucket_cors_configuration(
        &self,
        resource: &Resource<impl Sized>,
    ) -> ProviderResult<CreateCompletion> {
        let bucket = require_string_attr(resource, "bucket")?;
        let identifier =
            crate::helpers::applied_identifier(&resource.id, "s3.PutBucketCors", bucket)?;
        let outcome = self
            .put_s3_bucket_cors(&resource.id, &identifier, resource)
            .await?;
        Ok(self.create_outcome_from_read(
            &resource.id,
            identifier,
            resource.resolved_attributes(),
            outcome,
        ))
    }

    pub(crate) async fn update_s3_bucket_cors_configuration(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        _from: &State,
        to: Resource,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        self.put_s3_bucket_cors(&id, identifier, &to).await
    }

    async fn put_s3_bucket_cors(
        &self,
        id: &ResourceId,
        bucket: &AppliedIdentifier,
        resource: &Resource<impl Sized>,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        let applied_identifier = bucket;
        let bucket = bucket.as_str();
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
                api_error_with_meta("Failed to put bucket cors", e).for_resource(id.clone())
            })?;

        Ok(self
            .read_s3_bucket_cors_configuration(id, applied_identifier)
            .await)
    }

    pub(crate) async fn delete_s3_bucket_cors_configuration_idempotent(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        let identifier = identifier.as_str();
        let result = retry_aws_operation(RetryPolicy::default(), || {
            let client = &self.s3_client;
            client.delete_bucket_cors().bucket(identifier)
        })
        .await;

        match result {
            Ok(_) => Ok(()),
            Err(e)
                if is_s3_not_configured_error(&e, "NoSuchCORSConfiguration")
                    || is_s3_not_configured_error(&e, "NoSuchBucket") =>
            {
                Ok(())
            }
            Err(e) => Err(
                api_error_with_meta("Failed to delete bucket cors configuration", e)
                    .for_resource(id.clone()),
            ),
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
        builder = builder.id(s.expose_plaintext());
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
            Value::Concrete(ConcreteValue::String(s)) => Ok(s.expose_plaintext().to_string()),
            _ => Err(ProviderError::invalid_input(format!(
                "cors_rule.{field} must be a list of strings"
            ))
            .for_resource(id.clone())),
        })
        .collect()
}

fn rule_to_value(rule: &CorsRule) -> Value {
    let mut m = IndexMap::new();
    if let Some(s) = rule.id() {
        m.insert(
            "id".to_string(),
            Value::Concrete(ConcreteValue::String(s.to_string().into())),
        );
    }
    let methods: Vec<Value> = rule
        .allowed_methods()
        .iter()
        .map(|s| Value::Concrete(ConcreteValue::String(s.clone().into())))
        .collect();
    m.insert(
        "allowed_methods".to_string(),
        Value::Concrete(ConcreteValue::List(methods)),
    );
    let origins: Vec<Value> = rule
        .allowed_origins()
        .iter()
        .map(|s| Value::Concrete(ConcreteValue::String(s.clone().into())))
        .collect();
    m.insert(
        "allowed_origins".to_string(),
        Value::Concrete(ConcreteValue::List(origins)),
    );
    let headers: Vec<Value> = rule
        .allowed_headers()
        .iter()
        .map(|s| Value::Concrete(ConcreteValue::String(s.clone().into())))
        .collect();
    m.insert(
        "allowed_headers".to_string(),
        Value::Concrete(ConcreteValue::List(headers)),
    );
    let expose: Vec<Value> = rule
        .expose_headers()
        .iter()
        .map(|s| Value::Concrete(ConcreteValue::String(s.clone().into())))
        .collect();
    m.insert(
        "expose_headers".to_string(),
        Value::Concrete(ConcreteValue::List(expose)),
    );
    if let Some(n) = rule.max_age_seconds() {
        m.insert(
            "max_age_seconds".to_string(),
            Value::Concrete(ConcreteValue::Int(n as i64)),
        );
    }
    Value::Concrete(ConcreteValue::Map(m))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_optional_cors_fields_are_preserved() {
        let rule = CorsRule::builder()
            .id("")
            .allowed_methods("GET")
            .allowed_origins("https://example.com")
            .set_allowed_headers(Some(Vec::new()))
            .set_expose_headers(Some(Vec::new()))
            .build()
            .unwrap();
        let Value::Concrete(ConcreteValue::Map(read)) = rule_to_value(&rule) else {
            panic!("expected rule map");
        };
        assert_eq!(
            read.get("id"),
            Some(&Value::Concrete(ConcreteValue::String("".into())))
        );
        assert_eq!(
            read.get("allowed_headers"),
            Some(&Value::Concrete(ConcreteValue::List(Vec::new())))
        );
        assert_eq!(
            read.get("expose_headers"),
            Some(&Value::Concrete(ConcreteValue::List(Vec::new())))
        );
    }
}
