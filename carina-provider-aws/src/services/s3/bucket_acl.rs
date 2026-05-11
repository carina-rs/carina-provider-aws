use std::collections::HashMap;

use aws_sdk_s3::types::BucketCannedAcl;
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};
use carina_core::utils::convert_enum_value;

use crate::AwsProvider;
use crate::helpers::{require_string_attr, sdk_error_message};
use crate::services::s3::bucket::is_s3_not_configured_error;

impl AwsProvider {
    /// Read an S3 BucketAcl.
    ///
    /// AWS does not return a "canned ACL" name from `GetBucketAcl`; it
    /// returns the underlying grant list. We do not attempt to round-trip
    /// the canned-ACL name on read, only echo back the configured
    /// `acl` attribute when the bucket exists. This is acceptable because
    /// `acl` is a write-only setter from the API's perspective.
    pub(crate) async fn read_s3_bucket_acl(
        &self,
        id: &ResourceId,
        identifier: Option<&str>,
    ) -> ProviderResult<State> {
        let Some(bucket) = identifier else {
            return Ok(State::not_found(id.clone()));
        };

        let result = self.s3_client.get_bucket_acl().bucket(bucket).send().await;

        match result {
            Ok(_) => {
                let mut attributes = HashMap::new();
                attributes.insert(
                    "bucket".to_string(),
                    Value::Concrete(ConcreteValue::String(bucket.to_string())),
                );
                Ok(State::existing(id.clone(), attributes).with_identifier(bucket.to_string()))
            }
            Err(e) => {
                if is_s3_not_configured_error(&e, "NoSuchBucket") {
                    return Ok(State::not_found(id.clone()));
                }
                Err(
                    ProviderError::api_error(sdk_error_message("Failed to get bucket ACL", &e))
                        .for_resource(id.clone()),
                )
            }
        }
    }

    pub(crate) async fn create_s3_bucket_acl(&self, resource: Resource) -> ProviderResult<State> {
        let bucket = require_string_attr(&resource, "bucket")?;
        self.put_s3_bucket_acl(&resource.id, &bucket, &resource)
            .await
    }

    pub(crate) async fn update_s3_bucket_acl(
        &self,
        id: ResourceId,
        identifier: &str,
        _from: &State,
        to: Resource,
    ) -> ProviderResult<State> {
        self.put_s3_bucket_acl(&id, identifier, &to).await
    }

    async fn put_s3_bucket_acl(
        &self,
        id: &ResourceId,
        bucket: &str,
        resource: &Resource,
    ) -> ProviderResult<State> {
        let acl_str = match resource.get_attr("acl") {
            // convert_enum_value normalizes namespaced/typed identifiers
            // (`aws.s3.BucketAcl.Acl.public_read`) and snake-cased aliases
            // (`public_read` ⇄ `public-read`) back to AWS canonical form.
            Some(Value::Concrete(ConcreteValue::String(s))) => convert_enum_value(s).to_string(),
            _ => {
                return Err(
                    ProviderError::invalid_input("acl is required").for_resource(id.clone())
                );
            }
        };

        self.s3_client
            .put_bucket_acl()
            .bucket(bucket)
            .acl(BucketCannedAcl::from(acl_str.as_str()))
            .send()
            .await
            .map_err(|e| {
                ProviderError::api_error(sdk_error_message("Failed to put bucket ACL", &e))
                    .for_resource(id.clone())
            })?;

        self.read_s3_bucket_acl(id, Some(bucket)).await
    }

    /// "Delete" an S3 BucketAcl by resetting it to "private" — there is no
    /// DeleteBucketAcl API. NoSuchBucket is treated as success.
    pub(crate) async fn delete_s3_bucket_acl_reset(
        &self,
        id: ResourceId,
        identifier: &str,
    ) -> ProviderResult<()> {
        let result = self
            .s3_client
            .put_bucket_acl()
            .bucket(identifier)
            .acl(BucketCannedAcl::Private)
            .send()
            .await;

        match result {
            Ok(_) => Ok(()),
            Err(e) if is_s3_not_configured_error(&e, "NoSuchBucket") => Ok(()),
            Err(e) => Err(ProviderError::api_error(sdk_error_message(
                "Failed to reset bucket ACL",
                &e,
            ))
            .for_resource(id.clone())),
        }
    }
}
