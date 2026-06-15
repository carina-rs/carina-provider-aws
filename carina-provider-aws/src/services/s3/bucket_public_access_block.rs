use std::collections::HashMap;

use aws_sdk_s3::types::PublicAccessBlockConfiguration;
use carina_core::provider::ProviderResult;
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{RetryPolicy, optional_bool_attr, require_string_attr, retry_aws_operation};
use crate::services::s3::bucket::is_s3_not_configured_error;

impl AwsProvider {
    /// Read an S3 BucketPublicAccessBlock.
    ///
    /// `identifier` is the bucket name. Returns `State::not_found` when no
    /// PublicAccessBlock configuration exists on the bucket
    /// (`NoSuchPublicAccessBlockConfiguration`).
    pub(crate) async fn read_s3_bucket_public_access_block(
        &self,
        id: &ResourceId,
        identifier: Option<&str>,
    ) -> ProviderResult<State> {
        let Some(bucket) = identifier else {
            return Ok(State::not_found(id.clone()));
        };

        let result = self
            .s3_client
            .get_public_access_block()
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

                if let Some(config) = output.public_access_block_configuration() {
                    if let Some(v) = config.block_public_acls() {
                        attributes.insert(
                            "block_public_acls".to_string(),
                            Value::Concrete(ConcreteValue::Bool(v)),
                        );
                    }
                    if let Some(v) = config.ignore_public_acls() {
                        attributes.insert(
                            "ignore_public_acls".to_string(),
                            Value::Concrete(ConcreteValue::Bool(v)),
                        );
                    }
                    if let Some(v) = config.block_public_policy() {
                        attributes.insert(
                            "block_public_policy".to_string(),
                            Value::Concrete(ConcreteValue::Bool(v)),
                        );
                    }
                    if let Some(v) = config.restrict_public_buckets() {
                        attributes.insert(
                            "restrict_public_buckets".to_string(),
                            Value::Concrete(ConcreteValue::Bool(v)),
                        );
                    }
                }

                Ok(State::existing(id.clone(), attributes).with_identifier(bucket.to_string()))
            }
            Err(e) => {
                if is_s3_not_configured_error(&e, "NoSuchPublicAccessBlockConfiguration") {
                    return Ok(State::not_found(id.clone()));
                }
                Err(api_error_with_meta(
                    "Failed to get public access block",
                    "s3.GetPublicAccessBlock",
                    e,
                )
                .for_resource(id.clone()))
            }
        }
    }

    pub(crate) async fn create_s3_bucket_public_access_block(
        &self,
        resource: &Resource,
    ) -> ProviderResult<State> {
        let bucket = require_string_attr(resource, "bucket")?;
        self.put_s3_bucket_public_access_block(&resource.id, &bucket, resource)
            .await
    }

    pub(crate) async fn update_s3_bucket_public_access_block(
        &self,
        id: ResourceId,
        identifier: &str,
        _from: &State,
        to: Resource,
    ) -> ProviderResult<State> {
        self.put_s3_bucket_public_access_block(&id, identifier, &to)
            .await
    }

    async fn put_s3_bucket_public_access_block(
        &self,
        id: &ResourceId,
        bucket: &str,
        resource: &Resource,
    ) -> ProviderResult<State> {
        let mut builder = PublicAccessBlockConfiguration::builder();
        if let Some(v) = optional_bool_attr(resource, "block_public_acls") {
            builder = builder.block_public_acls(v);
        }
        if let Some(v) = optional_bool_attr(resource, "ignore_public_acls") {
            builder = builder.ignore_public_acls(v);
        }
        if let Some(v) = optional_bool_attr(resource, "block_public_policy") {
            builder = builder.block_public_policy(v);
        }
        if let Some(v) = optional_bool_attr(resource, "restrict_public_buckets") {
            builder = builder.restrict_public_buckets(v);
        }
        let config = builder.build();

        self.s3_client
            .put_public_access_block()
            .bucket(bucket)
            .public_access_block_configuration(config)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta(
                    "Failed to put public access block",
                    "s3.PutPublicAccessBlock",
                    e,
                )
                .for_resource(id.clone())
            })?;

        self.read_s3_bucket_public_access_block(id, Some(bucket))
            .await
    }

    /// Delete an S3 BucketPublicAccessBlock idempotently.
    ///
    /// Treats `NoSuchPublicAccessBlockConfiguration` (no PAB existed) and
    /// `NoSuchBucket` (the parent bucket was already removed) as success.
    pub(crate) async fn delete_s3_bucket_public_access_block_idempotent(
        &self,
        id: ResourceId,
        identifier: &str,
    ) -> ProviderResult<()> {
        let result =
            retry_aws_operation("delete public access block", RetryPolicy::default(), || {
                let client = &self.s3_client;
                async move {
                    client
                        .delete_public_access_block()
                        .bucket(identifier)
                        .send()
                        .await
                }
            })
            .await;

        match result {
            Ok(_) => Ok(()),
            Err(e)
                if is_s3_not_configured_error(&e, "NoSuchPublicAccessBlockConfiguration")
                    || is_s3_not_configured_error(&e, "NoSuchBucket") =>
            {
                Ok(())
            }
            Err(e) => Err(api_error_with_meta(
                "Failed to delete public access block",
                "s3.DeletePublicAccessBlock",
                e,
            )
            .for_resource(id.clone())),
        }
    }
}
