use std::collections::HashMap;

use aws_sdk_s3::types::{
    BucketLoggingStatus, LoggingEnabled, PartitionDateSource, PartitionedPrefix, SimplePrefix,
    TargetObjectKeyFormat,
};
use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{
    ConcreteValue, Resource, ResourceId, State, Value, enum_attr_at_schema_path,
};
use carina_core::schema::ResourceSchema;
use indexmap::IndexMap;

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{
    RetryPolicy, enum_attr_error, require_string_attr, retry_aws_operation, sdk_error_message,
};
use crate::provider::CreateCompletion;
use crate::services::s3::bucket::is_s3_not_configured_error;

impl AwsProvider {
    pub(crate) async fn read_s3_bucket_logging(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        let applied_identifier = identifier;
        let bucket = identifier.as_str();

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
                    Value::Concrete(ConcreteValue::String(bucket.to_string().into())),
                );
                if let Some(le) = output.logging_enabled() {
                    attributes.insert(
                        "target_bucket".to_string(),
                        Value::Concrete(ConcreteValue::String(
                            le.target_bucket().to_string().into(),
                        )),
                    );
                    attributes.insert(
                        "target_prefix".to_string(),
                        Value::Concrete(ConcreteValue::String(
                            le.target_prefix().to_string().into(),
                        )),
                    );
                    if let Some(fmt) = le.target_object_key_format() {
                        attributes.insert(
                            "target_object_key_format".to_string(),
                            target_object_key_format_to_value(fmt),
                        );
                    }
                }
                Ok(crate::helpers::found_read(
                    id,
                    applied_identifier,
                    attributes,
                ))
            }
            Err(e) => {
                if is_s3_not_configured_error(&e, "NoSuchBucket") {
                    return Ok(ReadOutcome::not_found(id.clone()));
                }
                Err(
                    api_error_with_meta("Failed to get bucket logging", "s3.GetBucketLogging", e)
                        .for_resource(id.clone()),
                )
            }
        }
    }

    pub(crate) async fn create_s3_bucket_logging(
        &self,
        resource: &Resource,
        schema: &ResourceSchema,
    ) -> ProviderResult<CreateCompletion> {
        let bucket = require_string_attr(resource, "bucket")?;
        let identifier =
            crate::helpers::applied_identifier(&resource.id, "s3.PutBucketLogging", bucket)?;
        let outcome = self
            .put_s3_bucket_logging(&resource.id, &identifier, resource, schema)
            .await?;
        Ok(self.create_outcome_from_read(
            &resource.id,
            identifier,
            resource.resolved_attributes(),
            outcome,
        ))
    }

    pub(crate) async fn update_s3_bucket_logging(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        _from: &State,
        to: Resource,
        schema: &ResourceSchema,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        self.put_s3_bucket_logging(&id, identifier, &to, schema)
            .await
    }

    async fn put_s3_bucket_logging(
        &self,
        id: &ResourceId,
        bucket: &AppliedIdentifier,
        resource: &Resource,
        schema: &ResourceSchema,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        let applied_identifier = bucket;
        let bucket = bucket.as_str();
        let target_bucket = require_string_attr(resource, "target_bucket")?;
        let target_prefix = match resource.get_attr("target_prefix") {
            Some(Value::Concrete(ConcreteValue::String(s))) => s.expose_plaintext().to_string(),
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
            le_builder = le_builder.target_object_key_format(build_key_format(id, schema, v)?);
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
                api_error_with_meta("Failed to put bucket logging", "s3.PutBucketLogging", e)
                    .for_resource(id.clone())
            })?;

        Ok(self.read_s3_bucket_logging(id, applied_identifier).await)
    }

    pub(crate) async fn delete_s3_bucket_logging_idempotent(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        let identifier = identifier.as_str();
        let result = retry_aws_operation("clear bucket logging", RetryPolicy::default(), || {
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
            Err(e) => {
                Err(
                    api_error_with_meta("Failed to clear bucket logging", "s3.PutBucketLogging", e)
                        .for_resource(id.clone()),
                )
            }
        }
    }
}

fn build_key_format(
    id: &ResourceId,
    schema: &ResourceSchema,
    value: &Value,
) -> ProviderResult<TargetObjectKeyFormat> {
    let Value::Concrete(ConcreteValue::Map(map)) = value else {
        return Err(
            ProviderError::invalid_input("target_object_key_format must be a map")
                .for_resource(id.clone()),
        );
    };

    let mut builder = TargetObjectKeyFormat::builder();
    match map.get("simple_prefix") {
        Some(Value::Concrete(ConcreteValue::Map(_))) => {
            builder = builder.simple_prefix(SimplePrefix::builder().build());
        }
        Some(_) => {
            return Err(ProviderError::invalid_input("simple_prefix must be a map")
                .for_resource(id.clone()));
        }
        None => {}
    }
    match map.get("partitioned_prefix") {
        Some(Value::Concrete(ConcreteValue::Map(pp))) => {
            let mut pb = PartitionedPrefix::builder();
            if let Some(source) = enum_attr_at_schema_path(
                schema,
                "target_object_key_format",
                &["partitioned_prefix", "partition_date_source"],
                pp.get("partition_date_source"),
            )
            .map_err(|error| enum_attr_error(id, error))?
            {
                pb = pb.partition_date_source(PartitionDateSource::from(source.api_value()));
            }
            builder = builder.partitioned_prefix(pb.build());
        }
        Some(_) => {
            return Err(
                ProviderError::invalid_input("partitioned_prefix must be a map")
                    .for_resource(id.clone()),
            );
        }
        None => {}
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
                Value::Concrete(ConcreteValue::String(src.as_str().to_string().into())),
            );
        }
        m.insert(
            "partitioned_prefix".to_string(),
            Value::Concrete(ConcreteValue::Map(inner)),
        );
    }
    Value::Concrete(ConcreteValue::Map(m))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id() -> ResourceId {
        ResourceId::with_identity("s3.bucket_logging", "test")
    }

    fn schema() -> ResourceSchema {
        crate::schemas::generated::s3::bucket_logging::s3_bucket_logging_config().schema
    }

    #[test]
    fn invalid_partition_date_source_is_rejected() {
        let value = Value::Concrete(ConcreteValue::Map(
            [(
                "partitioned_prefix".to_string(),
                Value::Concrete(ConcreteValue::Map(
                    [(
                        "partition_date_source".to_string(),
                        Value::Concrete(ConcreteValue::String("eventtime".into())),
                    )]
                    .into_iter()
                    .collect(),
                )),
            )]
            .into_iter()
            .collect(),
        ));

        build_key_format(&id(), &schema(), &value)
            .expect_err("a non-exact closed-enum leaf must be rejected");
    }

    #[test]
    fn non_map_partitioned_prefix_is_rejected() {
        let value = Value::Concrete(ConcreteValue::Map(
            [(
                "partitioned_prefix".to_string(),
                Value::Concrete(ConcreteValue::Bool(true)),
            )]
            .into_iter()
            .collect(),
        ));

        build_key_format(&id(), &schema(), &value)
            .expect_err("a present partitioned_prefix must be a map");
    }

    #[test]
    fn non_map_simple_prefix_is_rejected() {
        let value = Value::Concrete(ConcreteValue::Map(
            [(
                "simple_prefix".to_string(),
                Value::Concrete(ConcreteValue::Bool(true)),
            )]
            .into_iter()
            .collect(),
        ));

        let error = build_key_format(&id(), &schema(), &value)
            .expect_err("a present simple_prefix must be a map");
        assert!(error.to_string().contains("simple_prefix must be a map"));
    }
}
