use std::collections::HashMap;

use aws_sdk_s3::types::{ObjectOwnership, OwnershipControls, OwnershipControlsRule};
use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};
use carina_core::schema::ResourceSchema;

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{
    RetryPolicy, require_enum_attr, require_string_attr, retry_aws_operation, sdk_error_message,
};
use crate::provider::CreateCompletion;
use crate::services::s3::bucket::is_s3_not_configured_error;

impl AwsProvider {
    pub(crate) async fn read_s3_bucket_ownership_controls(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        let applied_identifier = identifier;
        let bucket = identifier.as_str();

        let result = self
            .s3_client
            .get_bucket_ownership_controls()
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
                if let Some(controls) = output.ownership_controls()
                    && let Some(rule) = controls.rules().first()
                {
                    attributes.insert(
                        "object_ownership".to_string(),
                        Value::Concrete(ConcreteValue::String(
                            rule.object_ownership().as_str().to_string().into(),
                        )),
                    );
                }
                Ok(crate::helpers::found_read(
                    id,
                    applied_identifier,
                    attributes,
                ))
            }
            Err(e) => {
                if is_s3_not_configured_error(&e, "OwnershipControlsNotFoundError")
                    || is_s3_not_configured_error(&e, "NoSuchBucket")
                {
                    return Ok(ReadOutcome::not_found(id.clone()));
                }
                Err(api_error_with_meta(
                    "Failed to get bucket ownership controls",
                    "s3.GetBucketOwnershipControls",
                    e,
                )
                .for_resource(id.clone()))
            }
        }
    }

    pub(crate) async fn create_s3_bucket_ownership_controls(
        &self,
        resource: &Resource,
        schema: &ResourceSchema,
    ) -> ProviderResult<CreateCompletion> {
        let bucket = require_string_attr(resource, "bucket")?;
        let identifier = crate::helpers::applied_identifier(
            &resource.id,
            "s3.PutBucketOwnershipControls",
            bucket,
        )?;
        let outcome = self
            .put_s3_bucket_ownership_controls(&resource.id, &identifier, resource, schema)
            .await?;
        Ok(self.create_outcome_from_read(
            &resource.id,
            identifier,
            resource.resolved_attributes(),
            outcome,
        ))
    }

    pub(crate) async fn update_s3_bucket_ownership_controls(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        _from: &State,
        to: Resource,
        schema: &ResourceSchema,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        self.put_s3_bucket_ownership_controls(&id, identifier, &to, schema)
            .await
    }

    async fn put_s3_bucket_ownership_controls(
        &self,
        id: &ResourceId,
        bucket: &AppliedIdentifier,
        resource: &Resource,
        schema: &ResourceSchema,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        let applied_identifier = bucket;
        let bucket = bucket.as_str();
        let ownership_str = require_enum_attr(resource, schema, "object_ownership")?;

        let rule = OwnershipControlsRule::builder()
            .object_ownership(ObjectOwnership::from(ownership_str.as_str()))
            .build()
            .map_err(|e| {
                ProviderError::api_error(sdk_error_message(
                    "Failed to build OwnershipControlsRule",
                    &e,
                ))
                .for_resource(id.clone())
            })?;
        let controls = OwnershipControls::builder()
            .rules(rule)
            .build()
            .map_err(|e| {
                ProviderError::api_error(sdk_error_message("Failed to build OwnershipControls", &e))
                    .for_resource(id.clone())
            })?;

        self.s3_client
            .put_bucket_ownership_controls()
            .bucket(bucket)
            .ownership_controls(controls)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta(
                    "Failed to put bucket ownership controls",
                    "s3.PutBucketOwnershipControls",
                    e,
                )
                .for_resource(id.clone())
            })?;

        Ok(self
            .read_s3_bucket_ownership_controls(id, applied_identifier)
            .await)
    }

    pub(crate) async fn delete_s3_bucket_ownership_controls_idempotent(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        let identifier = identifier.as_str();
        let result = retry_aws_operation(RetryPolicy::default(), || {
            let client = &self.s3_client;
            client.delete_bucket_ownership_controls().bucket(identifier)
        })
        .await;

        match result {
            Ok(_) => Ok(()),
            Err(e)
                if is_s3_not_configured_error(&e, "OwnershipControlsNotFoundError")
                    || is_s3_not_configured_error(&e, "NoSuchBucket") =>
            {
                Ok(())
            }
            Err(e) => Err(api_error_with_meta(
                "Failed to delete bucket ownership controls",
                "s3.DeleteBucketOwnershipControls",
                e,
            )
            .for_resource(id.clone())),
        }
    }
}
