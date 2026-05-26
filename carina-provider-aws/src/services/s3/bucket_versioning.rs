use std::collections::HashMap;

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{RetryPolicy, require_string_attr, retry_aws_operation};
use crate::services::s3::bucket::is_s3_not_configured_error;
use aws_sdk_s3::types::{BucketVersioningStatus, MfaDelete, VersioningConfiguration};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};

impl AwsProvider {
    /// Read an S3 BucketVersioning.
    ///
    /// `identifier` is the bucket name. AWS returns `Status = None` when
    /// versioning has never been enabled; we treat that as `not_found` so a
    /// destroyed-then-redeclared resource gets a clean re-create.
    pub(crate) async fn read_s3_bucket_versioning(
        &self,
        id: &ResourceId,
        identifier: Option<&str>,
    ) -> ProviderResult<State> {
        let Some(bucket) = identifier else {
            return Ok(State::not_found(id.clone()));
        };

        let result = self
            .s3_client
            .get_bucket_versioning()
            .bucket(bucket)
            .send()
            .await;

        match result {
            Ok(output) => {
                let Some(status) = output.status() else {
                    return Ok(State::not_found(id.clone()));
                };
                let mut attributes = HashMap::new();
                attributes.insert(
                    "bucket".to_string(),
                    Value::Concrete(ConcreteValue::String(bucket.to_string())),
                );
                attributes.insert(
                    "status".to_string(),
                    Value::Concrete(ConcreteValue::String(status.as_str().to_string())),
                );
                if let Some(mfa) = output.mfa_delete() {
                    attributes.insert(
                        "mfa_delete".to_string(),
                        Value::Concrete(ConcreteValue::String(mfa.as_str().to_string())),
                    );
                }
                Ok(State::existing(id.clone(), attributes).with_identifier(bucket.to_string()))
            }
            Err(e) => {
                if is_s3_not_configured_error(&e, "NoSuchBucket") {
                    return Ok(State::not_found(id.clone()));
                }
                Err(api_error_with_meta(
                    "Failed to get bucket versioning",
                    "s3.GetBucketVersioning",
                    e,
                )
                .for_resource(id.clone()))
            }
        }
    }

    pub(crate) async fn create_s3_bucket_versioning(
        &self,
        resource: Resource,
    ) -> ProviderResult<State> {
        let bucket = require_string_attr(&resource, "bucket")?;
        self.put_s3_bucket_versioning(&resource.id, &bucket, &resource)
            .await
    }

    pub(crate) async fn update_s3_bucket_versioning(
        &self,
        id: ResourceId,
        identifier: &str,
        _from: &State,
        to: Resource,
    ) -> ProviderResult<State> {
        self.put_s3_bucket_versioning(&id, identifier, &to).await
    }

    async fn put_s3_bucket_versioning(
        &self,
        id: &ResourceId,
        bucket: &str,
        resource: &Resource,
    ) -> ProviderResult<State> {
        let status_str = match resource.get_attr("status") {
            Some(Value::Concrete(ConcreteValue::String(s))) => s.clone(),
            _ => {
                return Err(
                    ProviderError::invalid_input("status is required").for_resource(id.clone())
                );
            }
        };
        let status = BucketVersioningStatus::from(status_str.as_str());

        let mut config_builder = VersioningConfiguration::builder().status(status);
        if let Some(Value::Concrete(ConcreteValue::String(s))) = resource.get_attr("mfa_delete") {
            config_builder = config_builder.mfa_delete(MfaDelete::from(s.as_str()));
        }

        self.s3_client
            .put_bucket_versioning()
            .bucket(bucket)
            .versioning_configuration(config_builder.build())
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta(
                    "Failed to put bucket versioning",
                    "s3.PutBucketVersioning",
                    e,
                )
                .for_resource(id.clone())
            })?;

        self.read_s3_bucket_versioning(id, Some(bucket)).await
    }

    /// "Delete" an S3 BucketVersioning by suspending it.
    ///
    /// There is no DeleteBucketVersioning API: once enabled, versioning can
    /// only be Suspended. This matches Terraform's `aws_s3_bucket_versioning`
    /// destroy behaviour. NoSuchBucket is treated as success so that a
    /// destroy retry after the parent bucket is gone still satisfies its goal.
    pub(crate) async fn delete_s3_bucket_versioning_suspend(
        &self,
        id: ResourceId,
        identifier: &str,
    ) -> ProviderResult<()> {
        let result =
            retry_aws_operation("suspend bucket versioning", RetryPolicy::default(), || {
                let client = &self.s3_client;
                async move {
                    client
                        .put_bucket_versioning()
                        .bucket(identifier)
                        .versioning_configuration(
                            VersioningConfiguration::builder()
                                .status(BucketVersioningStatus::Suspended)
                                .build(),
                        )
                        .send()
                        .await
                }
            })
            .await;

        match result {
            Ok(_) => Ok(()),
            Err(e) if is_s3_not_configured_error(&e, "NoSuchBucket") => Ok(()),
            Err(e) => Err(api_error_with_meta(
                "Failed to suspend bucket versioning",
                "s3.PutBucketVersioning",
                e,
            )
            .for_resource(id.clone())),
        }
    }
}
