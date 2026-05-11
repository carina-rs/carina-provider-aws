use std::collections::HashMap;

use aws_sdk_s3::types::{ObjectOwnership, OwnershipControls, OwnershipControlsRule};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};
use carina_core::utils::extract_enum_value;

use crate::AwsProvider;
use crate::helpers::{require_string_attr, sdk_error_message};
use crate::services::s3::bucket::is_s3_not_configured_error;

impl AwsProvider {
    pub(crate) async fn read_s3_bucket_ownership_controls(
        &self,
        id: &ResourceId,
        identifier: Option<&str>,
    ) -> ProviderResult<State> {
        let Some(bucket) = identifier else {
            return Ok(State::not_found(id.clone()));
        };

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
                    Value::Concrete(ConcreteValue::String(bucket.to_string())),
                );
                if let Some(controls) = output.ownership_controls()
                    && let Some(rule) = controls.rules().first()
                {
                    attributes.insert(
                        "object_ownership".to_string(),
                        Value::Concrete(ConcreteValue::String(
                            rule.object_ownership().as_str().to_string(),
                        )),
                    );
                }
                Ok(State::existing(id.clone(), attributes).with_identifier(bucket.to_string()))
            }
            Err(e) => {
                if is_s3_not_configured_error(&e, "OwnershipControlsNotFoundError")
                    || is_s3_not_configured_error(&e, "NoSuchBucket")
                {
                    return Ok(State::not_found(id.clone()));
                }
                Err(ProviderError::api_error(sdk_error_message(
                    "Failed to get bucket ownership controls",
                    &e,
                ))
                .for_resource(id.clone()))
            }
        }
    }

    pub(crate) async fn create_s3_bucket_ownership_controls(
        &self,
        resource: Resource,
    ) -> ProviderResult<State> {
        let bucket = require_string_attr(&resource, "bucket")?;
        self.put_s3_bucket_ownership_controls(&resource.id, &bucket, &resource)
            .await
    }

    pub(crate) async fn update_s3_bucket_ownership_controls(
        &self,
        id: ResourceId,
        identifier: &str,
        _from: &State,
        to: Resource,
    ) -> ProviderResult<State> {
        self.put_s3_bucket_ownership_controls(&id, identifier, &to)
            .await
    }

    async fn put_s3_bucket_ownership_controls(
        &self,
        id: &ResourceId,
        bucket: &str,
        resource: &Resource,
    ) -> ProviderResult<State> {
        let ownership_str = match resource.get_attr("object_ownership") {
            Some(Value::Concrete(ConcreteValue::String(s))) => extract_enum_value(s).to_string(),
            _ => {
                return Err(ProviderError::invalid_input("object_ownership is required")
                    .for_resource(id.clone()));
            }
        };

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
                ProviderError::api_error(sdk_error_message(
                    "Failed to put bucket ownership controls",
                    &e,
                ))
                .for_resource(id.clone())
            })?;

        self.read_s3_bucket_ownership_controls(id, Some(bucket))
            .await
    }

    pub(crate) async fn delete_s3_bucket_ownership_controls_idempotent(
        &self,
        id: ResourceId,
        identifier: &str,
    ) -> ProviderResult<()> {
        let result = self
            .s3_client
            .delete_bucket_ownership_controls()
            .bucket(identifier)
            .send()
            .await;

        match result {
            Ok(_) => Ok(()),
            Err(e)
                if is_s3_not_configured_error(&e, "OwnershipControlsNotFoundError")
                    || is_s3_not_configured_error(&e, "NoSuchBucket") =>
            {
                Ok(())
            }
            Err(e) => Err(ProviderError::api_error(sdk_error_message(
                "Failed to delete bucket ownership controls",
                &e,
            ))
            .for_resource(id.clone())),
        }
    }
}
