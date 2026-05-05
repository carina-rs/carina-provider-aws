use std::collections::HashMap;

use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{Resource, ResourceId, State, Value};

use crate::AwsProvider;
use crate::helpers::{require_string_attr, sdk_error_message};
use crate::services::iam::role::{iam_policy_json_to_value, resolve_iam_policy_attr};
use crate::services::s3::bucket::is_s3_not_configured_error;

impl AwsProvider {
    /// Read an S3 BucketPolicy.
    ///
    /// `identifier` is the bucket name. Returns `State::not_found` when the
    /// bucket has no policy (`NoSuchBucketPolicy`); other errors propagate.
    pub(crate) async fn read_s3_bucket_policy(
        &self,
        id: &ResourceId,
        identifier: Option<&str>,
    ) -> ProviderResult<State> {
        let Some(bucket) = identifier else {
            return Ok(State::not_found(id.clone()));
        };

        let result = self
            .s3_client
            .get_bucket_policy()
            .bucket(bucket)
            .send()
            .await;

        match result {
            Ok(output) => {
                let mut attributes = HashMap::new();
                attributes.insert("bucket".to_string(), Value::String(bucket.to_string()));

                if let Some(policy_json) = output.policy() {
                    let policy_value = iam_policy_json_to_value(policy_json).map_err(|e| {
                        ProviderError::new(format!("Failed to parse bucket policy: {}", e))
                            .for_resource(id.clone())
                    })?;
                    attributes.insert("policy".to_string(), policy_value);
                }

                Ok(State::existing(id.clone(), attributes).with_identifier(bucket.to_string()))
            }
            Err(e) => {
                if is_s3_not_configured_error(&e, "NoSuchBucketPolicy") {
                    return Ok(State::not_found(id.clone()));
                }
                Err(
                    ProviderError::new(sdk_error_message("Failed to get bucket policy", &e))
                        .for_resource(id.clone()),
                )
            }
        }
    }

    /// Create an S3 BucketPolicy via PutBucketPolicy.
    pub(crate) async fn create_s3_bucket_policy(
        &self,
        resource: Resource,
    ) -> ProviderResult<State> {
        let bucket = require_string_attr(&resource, "bucket")?;
        self.put_s3_bucket_policy(&resource.id, &bucket, &resource)
            .await
    }

    /// Update an S3 BucketPolicy. PutBucketPolicy replaces the existing policy.
    pub(crate) async fn update_s3_bucket_policy(
        &self,
        id: ResourceId,
        identifier: &str,
        _from: &State,
        to: Resource,
    ) -> ProviderResult<State> {
        self.put_s3_bucket_policy(&id, identifier, &to).await
    }

    async fn put_s3_bucket_policy(
        &self,
        id: &ResourceId,
        bucket: &str,
        resource: &Resource,
    ) -> ProviderResult<State> {
        let policy_json = resolve_iam_policy_attr(resource, "policy")?;

        self.s3_client
            .put_bucket_policy()
            .bucket(bucket)
            .policy(&policy_json)
            .send()
            .await
            .map_err(|e| {
                ProviderError::new(sdk_error_message("Failed to put bucket policy", &e))
                    .for_resource(id.clone())
            })?;

        self.read_s3_bucket_policy(id, Some(bucket)).await
    }

    /// Delete an S3 BucketPolicy idempotently.
    ///
    /// Treats both `NoSuchBucketPolicy` (no policy was attached) and
    /// `NoSuchBucket` (the parent bucket itself was deleted out-of-band)
    /// as success. The policy cannot exist without the bucket, so either
    /// signal means the destroy goal is satisfied.
    pub(crate) async fn delete_s3_bucket_policy_idempotent(
        &self,
        id: ResourceId,
        identifier: &str,
    ) -> ProviderResult<()> {
        let result = self
            .s3_client
            .delete_bucket_policy()
            .bucket(identifier)
            .send()
            .await;

        match result {
            Ok(_) => Ok(()),
            Err(e)
                if is_s3_not_configured_error(&e, "NoSuchBucketPolicy")
                    || is_s3_not_configured_error(&e, "NoSuchBucket") =>
            {
                Ok(())
            }
            Err(e) => Err(ProviderError::new(sdk_error_message(
                "Failed to delete bucket policy",
                &e,
            ))
            .for_resource(id.clone())),
        }
    }
}
