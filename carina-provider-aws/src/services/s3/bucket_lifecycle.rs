use std::collections::HashMap;

use aws_sdk_s3::types::{
    AbortIncompleteMultipartUpload, BucketLifecycleConfiguration, ExpirationStatus,
    LifecycleExpiration, LifecycleRule, NoncurrentVersionExpiration, NoncurrentVersionTransition,
    Transition, TransitionStorageClass,
};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, ManagedResource, ResourceId, State, Value};
use indexmap::IndexMap;

use crate::AwsProvider;
use crate::helpers::{require_string_attr, retry_aws_operation, sdk_error_message};
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
        let result = retry_aws_operation("delete bucket lifecycle configuration", 3, 5, || {
            let client = &self.s3_client;
            async move {
                client
                    .delete_bucket_lifecycle()
                    .bucket(identifier)
                    .send()
                    .await
            }
        })
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
    #[allow(deprecated)]
    if let Some(Value::Concrete(ConcreteValue::String(s))) = map.get("prefix") {
        builder = builder.prefix(s);
    }
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
    #[allow(deprecated)]
    if let Some(s) = rule.prefix()
        && !s.is_empty()
    {
        map.insert(
            "prefix".to_string(),
            Value::Concrete(ConcreteValue::String(s.to_string())),
        );
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
