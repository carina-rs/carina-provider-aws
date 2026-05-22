use std::collections::HashMap;

use aws_sdk_s3::types::{
    BucketLoggingStatus, LoggingEnabled, PartitionDateSource, PartitionedPrefix, SimplePrefix,
    TargetObjectKeyFormat,
};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, ManagedResource, ResourceId, State, Value};
use indexmap::IndexMap;

use crate::AwsProvider;
use crate::helpers::{require_string_attr, retry_aws_operation, sdk_error_message};
use crate::services::s3::bucket::is_s3_not_configured_error;

impl AwsProvider {
    pub(crate) async fn read_s3_bucket_logging(
        &self,
        id: &ResourceId,
        identifier: Option<&str>,
    ) -> ProviderResult<State> {
        let Some(bucket) = identifier else {
            return Ok(State::not_found(id.clone()));
        };

        let result = self
            .s3_client
            .get_bucket_logging()
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
                if let Some(le) = output.logging_enabled() {
                    attributes.insert(
                        "target_bucket".to_string(),
                        Value::Concrete(ConcreteValue::String(le.target_bucket().to_string())),
                    );
                    attributes.insert(
                        "target_prefix".to_string(),
                        Value::Concrete(ConcreteValue::String(le.target_prefix().to_string())),
                    );
                    if let Some(fmt) = le.target_object_key_format() {
                        attributes.insert(
                            "target_object_key_format".to_string(),
                            target_object_key_format_to_value(fmt),
                        );
                    }
                }
                Ok(State::existing(id.clone(), attributes).with_identifier(bucket.to_string()))
            }
            Err(e) => {
                if is_s3_not_configured_error(&e, "NoSuchBucket") {
                    return Ok(State::not_found(id.clone()));
                }
                Err(
                    ProviderError::api_error(sdk_error_message("Failed to get bucket logging", &e))
                        .for_resource(id.clone()),
                )
            }
        }
    }

    pub(crate) async fn create_s3_bucket_logging(
        &self,
        resource: ManagedResource,
    ) -> ProviderResult<State> {
        let bucket = require_string_attr(&resource, "bucket")?;
        self.put_s3_bucket_logging(&resource.id, &bucket, &resource)
            .await
    }

    pub(crate) async fn update_s3_bucket_logging(
        &self,
        id: ResourceId,
        identifier: &str,
        _from: &State,
        to: ManagedResource,
    ) -> ProviderResult<State> {
        self.put_s3_bucket_logging(&id, identifier, &to).await
    }

    async fn put_s3_bucket_logging(
        &self,
        id: &ResourceId,
        bucket: &str,
        resource: &ManagedResource,
    ) -> ProviderResult<State> {
        let target_bucket = require_string_attr(resource, "target_bucket")?;
        let target_prefix = match resource.get_attr("target_prefix") {
            Some(Value::Concrete(ConcreteValue::String(s))) => s.clone(),
            None => String::new(),
            _ => {
                return Err(
                    ProviderError::invalid_input("target_prefix must be a string")
                        .for_resource(id.clone()),
                );
            }
        };

        let mut le_builder = LoggingEnabled::builder()
            .target_bucket(target_bucket)
            .target_prefix(target_prefix);

        if let Some(v) = resource.get_attr("target_object_key_format") {
            le_builder = le_builder.target_object_key_format(build_key_format(id, v)?);
        }

        let le = le_builder.build().map_err(|e| {
            ProviderError::api_error(sdk_error_message("Failed to build LoggingEnabled", &e))
                .for_resource(id.clone())
        })?;

        let status = BucketLoggingStatus::builder().logging_enabled(le).build();

        self.s3_client
            .put_bucket_logging()
            .bucket(bucket)
            .bucket_logging_status(status)
            .send()
            .await
            .map_err(|e| {
                ProviderError::api_error(sdk_error_message("Failed to put bucket logging", &e))
                    .for_resource(id.clone())
            })?;

        self.read_s3_bucket_logging(id, Some(bucket)).await
    }

    pub(crate) async fn delete_s3_bucket_logging_idempotent(
        &self,
        id: ResourceId,
        identifier: &str,
    ) -> ProviderResult<()> {
        let result = retry_aws_operation("clear bucket logging", 3, 5, || {
            let client = &self.s3_client;
            async move {
                client
                    .put_bucket_logging()
                    .bucket(identifier)
                    .bucket_logging_status(BucketLoggingStatus::builder().build())
                    .send()
                    .await
            }
        })
        .await;

        match result {
            Ok(_) => Ok(()),
            Err(e) if is_s3_not_configured_error(&e, "NoSuchBucket") => Ok(()),
            Err(e) => Err(ProviderError::api_error(sdk_error_message(
                "Failed to clear bucket logging",
                &e,
            ))
            .for_resource(id.clone())),
        }
    }
}

fn build_key_format(id: &ResourceId, value: &Value) -> ProviderResult<TargetObjectKeyFormat> {
    let Value::Concrete(ConcreteValue::Map(map)) = value else {
        return Err(
            ProviderError::invalid_input("target_object_key_format must be a map")
                .for_resource(id.clone()),
        );
    };

    let mut builder = TargetObjectKeyFormat::builder();
    if map.contains_key("simple_prefix") {
        builder = builder.simple_prefix(SimplePrefix::builder().build());
    }
    if let Some(Value::Concrete(ConcreteValue::Map(pp))) = map.get("partitioned_prefix") {
        let mut pb = PartitionedPrefix::builder();
        if let Some(Value::Concrete(ConcreteValue::String(s))) = pp.get("partition_date_source") {
            pb = pb.partition_date_source(PartitionDateSource::from(s.as_str()));
        }
        builder = builder.partitioned_prefix(pb.build());
    }
    Ok(builder.build())
}

fn target_object_key_format_to_value(fmt: &TargetObjectKeyFormat) -> Value {
    let mut m = IndexMap::new();
    if fmt.simple_prefix().is_some() {
        m.insert(
            "simple_prefix".to_string(),
            Value::Concrete(ConcreteValue::Map(IndexMap::new())),
        );
    }
    if let Some(pp) = fmt.partitioned_prefix() {
        let mut inner = IndexMap::new();
        if let Some(src) = pp.partition_date_source() {
            inner.insert(
                "partition_date_source".to_string(),
                Value::Concrete(ConcreteValue::String(src.as_str().to_string())),
            );
        }
        m.insert(
            "partitioned_prefix".to_string(),
            Value::Concrete(ConcreteValue::Map(inner)),
        );
    }
    Value::Concrete(ConcreteValue::Map(m))
}
