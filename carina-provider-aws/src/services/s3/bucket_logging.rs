use std::collections::HashMap;

use aws_sdk_s3::types::{
    BucketLoggingStatus, LoggingEnabled, PartitionDateSource, PartitionedPrefix, SimplePrefix,
    TargetObjectKeyFormat,
};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{Resource, ResourceId, State, Value};
use carina_core::utils::extract_enum_value;
use indexmap::IndexMap;

use crate::AwsProvider;
use crate::helpers::{require_string_attr, sdk_error_message};
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
                attributes.insert("bucket".to_string(), Value::String(bucket.to_string()));
                if let Some(le) = output.logging_enabled() {
                    attributes.insert(
                        "target_bucket".to_string(),
                        Value::String(le.target_bucket().to_string()),
                    );
                    attributes.insert(
                        "target_prefix".to_string(),
                        Value::String(le.target_prefix().to_string()),
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
                    ProviderError::new(sdk_error_message("Failed to get bucket logging", &e))
                        .for_resource(id.clone()),
                )
            }
        }
    }

    pub(crate) async fn create_s3_bucket_logging(
        &self,
        resource: Resource,
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
        to: Resource,
    ) -> ProviderResult<State> {
        self.put_s3_bucket_logging(&id, identifier, &to).await
    }

    async fn put_s3_bucket_logging(
        &self,
        id: &ResourceId,
        bucket: &str,
        resource: &Resource,
    ) -> ProviderResult<State> {
        let target_bucket = require_string_attr(resource, "target_bucket")?;
        let target_prefix = match resource.get_attr("target_prefix") {
            Some(Value::String(s)) => s.clone(),
            None => String::new(),
            _ => {
                return Err(
                    ProviderError::new("target_prefix must be a string").for_resource(id.clone())
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
            ProviderError::new(sdk_error_message("Failed to build LoggingEnabled", &e))
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
                ProviderError::new(sdk_error_message("Failed to put bucket logging", &e))
                    .for_resource(id.clone())
            })?;

        self.read_s3_bucket_logging(id, Some(bucket)).await
    }

    pub(crate) async fn delete_s3_bucket_logging_idempotent(
        &self,
        id: ResourceId,
        identifier: &str,
    ) -> ProviderResult<()> {
        let empty = BucketLoggingStatus::builder().build();
        let result = self
            .s3_client
            .put_bucket_logging()
            .bucket(identifier)
            .bucket_logging_status(empty)
            .send()
            .await;

        match result {
            Ok(_) => Ok(()),
            Err(e) if is_s3_not_configured_error(&e, "NoSuchBucket") => Ok(()),
            Err(e) => Err(ProviderError::new(sdk_error_message(
                "Failed to clear bucket logging",
                &e,
            ))
            .for_resource(id.clone())),
        }
    }
}

fn build_key_format(id: &ResourceId, value: &Value) -> ProviderResult<TargetObjectKeyFormat> {
    let Value::Map(map) = value else {
        return Err(
            ProviderError::new("target_object_key_format must be a map").for_resource(id.clone())
        );
    };

    let mut builder = TargetObjectKeyFormat::builder();
    if map.contains_key("simple_prefix") {
        builder = builder.simple_prefix(SimplePrefix::builder().build());
    }
    if let Some(Value::Map(pp)) = map.get("partitioned_prefix") {
        let mut pb = PartitionedPrefix::builder();
        if let Some(Value::String(s)) = pp.get("partition_date_source") {
            let normalized = extract_enum_value(s);
            pb = pb.partition_date_source(PartitionDateSource::from(normalized));
        }
        builder = builder.partitioned_prefix(pb.build());
    }
    Ok(builder.build())
}

fn target_object_key_format_to_value(fmt: &TargetObjectKeyFormat) -> Value {
    let mut m = IndexMap::new();
    if fmt.simple_prefix().is_some() {
        m.insert("simple_prefix".to_string(), Value::Map(IndexMap::new()));
    }
    if let Some(pp) = fmt.partitioned_prefix() {
        let mut inner = IndexMap::new();
        if let Some(src) = pp.partition_date_source() {
            inner.insert(
                "partition_date_source".to_string(),
                Value::String(src.as_str().to_string()),
            );
        }
        m.insert("partitioned_prefix".to_string(), Value::Map(inner));
    }
    Value::Map(m)
}
