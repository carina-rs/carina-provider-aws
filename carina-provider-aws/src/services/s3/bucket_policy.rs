use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{
    AbsoluteBindingAddress, NoPlaceholder, Resource, ResourceId, SnapshotState, Value,
};

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{RetryPolicy, retry_aws_operation};
use crate::provider::TypedCreateCompletion;
use crate::read_model::found_managed_read;
use crate::read_models_generated::S3BucketPolicyReadModel;
use crate::send_models_generated::{S3BucketPolicyCreateInput, S3BucketPolicyUpdateInput};
use crate::services::iam::role::{iam_policy_json_to_value, resolve_iam_policy_value};
use crate::services::s3::bucket::is_s3_not_configured_error;

fn decode_bucket_policy(id: &ResourceId, raw: &str) -> ProviderResult<Value> {
    iam_policy_json_to_value(raw).map_err(|error| {
        ProviderError::api_error_caused_by("Failed to parse bucket policy", error)
            .for_resource(id.clone())
    })
}

impl AwsProvider {
    /// Read an S3 BucketPolicy.
    ///
    /// `identifier` is the bucket name. Returns `ReadOutcome::NotFound` when the
    /// bucket has no policy (`NoSuchBucketPolicy`); other errors propagate.
    pub(crate) async fn read_s3_bucket_policy(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        let applied_identifier = identifier;
        let bucket = identifier.as_str();

        let result = self
            .s3_client
            .get_bucket_policy()
            .bucket(bucket)
            .send()
            .await;

        match result {
            Ok(output) => {
                if let Some(policy_json) = output.policy() {
                    let policy = decode_bucket_policy(id, policy_json)?;
                    let model = S3BucketPolicyReadModel {
                        bucket: crate::read_model::applied_identifier_value(identifier),
                        policy,
                    };
                    found_managed_read(id, applied_identifier, model)
                } else {
                    Err(
                        ProviderError::api_error("GetBucketPolicy returned no Policy")
                            .for_resource(id.clone()),
                    )
                }
            }
            Err(e) => {
                if is_s3_not_configured_error(&e, "NoSuchBucketPolicy") {
                    return Ok(ReadOutcome::not_found(id.clone()));
                }
                Err(api_error_with_meta("Failed to get bucket policy", e).for_resource(id.clone()))
            }
        }
    }

    /// Create an S3 BucketPolicy via PutBucketPolicy.
    pub(crate) async fn create_s3_bucket_policy(
        &self,
        resource: &Resource<impl Sized, NoPlaceholder, AbsoluteBindingAddress, impl Sized>,
    ) -> ProviderResult<TypedCreateCompletion> {
        let input = S3BucketPolicyCreateInput::decode(resource)?;
        let sent = input.sent();
        let crate::send_models_generated::S3BucketPolicyCreateParts { bucket, policy } =
            input.into_parts();
        let bucket = bucket.into_inner();
        let policy = policy.into_inner();
        let identifier =
            crate::helpers::applied_identifier(&resource.id, "s3.PutBucketPolicy", bucket)?;
        let outcome = self
            .put_s3_bucket_policy(&resource.id, &identifier, &policy)
            .await?;
        Ok((
            sent,
            self.create_outcome_from_read(
                &resource.id,
                identifier,
                resource.resolved_attributes(),
                outcome,
            ),
        ))
    }

    /// Update an S3 BucketPolicy. PutBucketPolicy replaces the existing policy.
    pub(crate) async fn update_s3_bucket_policy(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        _from: &SnapshotState,
        to: &crate::helpers::UpdateResource,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        let crate::send_models_generated::S3BucketPolicyUpdateParts { policy } =
            S3BucketPolicyUpdateInput::decode(to)?.into_parts();
        let policy = policy.into_inner();
        self.put_s3_bucket_policy(&id, identifier, &policy).await
    }

    async fn put_s3_bucket_policy(
        &self,
        id: &ResourceId,
        bucket: &AppliedIdentifier,
        policy: &Value,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        let applied_identifier = bucket;
        let bucket = bucket.as_str();
        let policy_json = resolve_iam_policy_value(id, "policy", policy)?;

        self.s3_client
            .put_bucket_policy()
            .bucket(bucket)
            .policy(&policy_json)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to put bucket policy", e).for_resource(id.clone())
            })?;

        Ok(self.read_s3_bucket_policy(id, applied_identifier).await)
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
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        let identifier = identifier.as_str();
        let result = retry_aws_operation(RetryPolicy::default(), || {
            let client = &self.s3_client;
            client.delete_bucket_policy().bucket(identifier)
        })
        .await;

        match result {
            Ok(_) => Ok(()),
            Err(e)
                if is_s3_not_configured_error(&e, "NoSuchBucketPolicy")
                    || is_s3_not_configured_error(&e, "NoSuchBucket") =>
            {
                Ok(())
            }
            Err(e) => {
                Err(api_error_with_meta("Failed to delete bucket policy", e)
                    .for_resource(id.clone()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::iam::role::IamPolicyDecodeError;
    use carina_core::provider::ProviderErrorKind;
    use std::error::Error;

    #[test]
    fn bucket_policy_read_decode_failure_keeps_its_typed_cause() {
        let id = ResourceId::with_provider_identity("aws", "s3.BucketPolicy", "test", None);
        for raw in [
            "",
            "{",
            r#"{"Statement":[{"Condition":{"NumericLessThan":{"example:Limit":1e999}}}]}"#,
            r#"{"Statement":[{"Condition":{"NumericLessThan":{"example:Limit":-1e999}}}]}"#,
        ] {
            let error =
                decode_bucket_policy(&id, raw).expect_err("a malformed policy must fail the read");
            assert_eq!(error.kind(), ProviderErrorKind::ApiError);
            let decode = error
                .detail()
                .cause()
                .unwrap()
                .downcast_ref::<IamPolicyDecodeError>()
                .expect("typed IAM policy decode error");
            assert!(matches!(decode, IamPolicyDecodeError::Json(_)));
            assert!(decode.source().unwrap().is::<serde_json::Error>());
        }
    }
}
