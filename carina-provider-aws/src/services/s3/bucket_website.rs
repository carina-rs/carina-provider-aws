use std::collections::HashMap;

use aws_sdk_s3::types::{
    ErrorDocument, IndexDocument, Protocol, RedirectAllRequestsTo, WebsiteConfiguration,
};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, ManagedResource, ResourceId, State, Value};
use indexmap::IndexMap;

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{RetryPolicy, require_string_attr, retry_aws_operation, sdk_error_message};
use crate::services::s3::bucket::is_s3_not_configured_error;

impl AwsProvider {
    pub(crate) async fn read_s3_bucket_website_configuration(
        &self,
        id: &ResourceId,
        identifier: Option<&str>,
    ) -> ProviderResult<State> {
        let Some(bucket) = identifier else {
            return Ok(State::not_found(id.clone()));
        };

        let result = self
            .s3_client
            .get_bucket_website()
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
                if let Some(idx) = output.index_document() {
                    let mut m = IndexMap::new();
                    m.insert(
                        "suffix".to_string(),
                        Value::Concrete(ConcreteValue::String(idx.suffix().to_string())),
                    );
                    attributes.insert(
                        "index_document".to_string(),
                        Value::Concrete(ConcreteValue::Map(m)),
                    );
                }
                if let Some(err) = output.error_document() {
                    let mut m = IndexMap::new();
                    m.insert(
                        "key".to_string(),
                        Value::Concrete(ConcreteValue::String(err.key().to_string())),
                    );
                    attributes.insert(
                        "error_document".to_string(),
                        Value::Concrete(ConcreteValue::Map(m)),
                    );
                }
                if let Some(r) = output.redirect_all_requests_to() {
                    let mut m = IndexMap::new();
                    m.insert(
                        "host_name".to_string(),
                        Value::Concrete(ConcreteValue::String(r.host_name().to_string())),
                    );
                    if let Some(p) = r.protocol() {
                        m.insert(
                            "protocol".to_string(),
                            Value::Concrete(ConcreteValue::String(p.as_str().to_string())),
                        );
                    }
                    attributes.insert(
                        "redirect_all_requests_to".to_string(),
                        Value::Concrete(ConcreteValue::Map(m)),
                    );
                }
                Ok(State::existing(id.clone(), attributes).with_identifier(bucket.to_string()))
            }
            Err(e) => {
                if is_s3_not_configured_error(&e, "NoSuchWebsiteConfiguration")
                    || is_s3_not_configured_error(&e, "NoSuchBucket")
                {
                    return Ok(State::not_found(id.clone()));
                }
                Err(api_error_with_meta(
                    "Failed to get bucket website configuration",
                    "s3.GetBucketWebsite",
                    e,
                )
                .for_resource(id.clone()))
            }
        }
    }

    pub(crate) async fn create_s3_bucket_website_configuration(
        &self,
        resource: ManagedResource,
    ) -> ProviderResult<State> {
        let bucket = require_string_attr(&resource, "bucket")?;
        self.put_s3_bucket_website(&resource.id, &bucket, &resource)
            .await
    }

    pub(crate) async fn update_s3_bucket_website_configuration(
        &self,
        id: ResourceId,
        identifier: &str,
        _from: &State,
        to: ManagedResource,
    ) -> ProviderResult<State> {
        self.put_s3_bucket_website(&id, identifier, &to).await
    }

    async fn put_s3_bucket_website(
        &self,
        id: &ResourceId,
        bucket: &str,
        resource: &ManagedResource,
    ) -> ProviderResult<State> {
        let mut config_builder = WebsiteConfiguration::builder();

        if let Some(Value::Concrete(ConcreteValue::Map(map))) = resource.get_attr("index_document")
        {
            let suffix = match map.get("suffix") {
                Some(Value::Concrete(ConcreteValue::String(s))) => s.clone(),
                _ => {
                    return Err(
                        ProviderError::invalid_input("index_document.suffix is required")
                            .for_resource(id.clone()),
                    );
                }
            };
            config_builder = config_builder.index_document(
                IndexDocument::builder()
                    .suffix(suffix)
                    .build()
                    .map_err(|e| {
                        ProviderError::api_error(sdk_error_message(
                            "Failed to build IndexDocument",
                            &e,
                        ))
                        .for_resource(id.clone())
                    })?,
            );
        }
        if let Some(Value::Concrete(ConcreteValue::Map(map))) = resource.get_attr("error_document")
        {
            let key = match map.get("key") {
                Some(Value::Concrete(ConcreteValue::String(s))) => s.clone(),
                _ => {
                    return Err(
                        ProviderError::invalid_input("error_document.key is required")
                            .for_resource(id.clone()),
                    );
                }
            };
            config_builder = config_builder.error_document(
                ErrorDocument::builder().key(key).build().map_err(|e| {
                    ProviderError::api_error(sdk_error_message("Failed to build ErrorDocument", &e))
                        .for_resource(id.clone())
                })?,
            );
        }
        if let Some(Value::Concrete(ConcreteValue::Map(map))) =
            resource.get_attr("redirect_all_requests_to")
        {
            let host_name = match map.get("host_name") {
                Some(Value::Concrete(ConcreteValue::String(s))) => s.clone(),
                _ => {
                    return Err(ProviderError::invalid_input(
                        "redirect_all_requests_to.host_name is required",
                    )
                    .for_resource(id.clone()));
                }
            };
            let mut rb = RedirectAllRequestsTo::builder().host_name(host_name);
            if let Some(Value::Concrete(ConcreteValue::String(p))) = map.get("protocol") {
                rb = rb.protocol(Protocol::from(p.as_str()));
            }
            config_builder = config_builder.redirect_all_requests_to(rb.build().map_err(|e| {
                ProviderError::api_error(sdk_error_message(
                    "Failed to build RedirectAllRequestsTo",
                    &e,
                ))
                .for_resource(id.clone())
            })?);
        }

        let config = config_builder.build();

        self.s3_client
            .put_bucket_website()
            .bucket(bucket)
            .website_configuration(config)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to put bucket website", "s3.PutBucketWebsite", e)
                    .for_resource(id.clone())
            })?;

        self.read_s3_bucket_website_configuration(id, Some(bucket))
            .await
    }

    pub(crate) async fn delete_s3_bucket_website_configuration_idempotent(
        &self,
        id: ResourceId,
        identifier: &str,
    ) -> ProviderResult<()> {
        let result = retry_aws_operation("delete bucket website", RetryPolicy::default(), || {
            let client = &self.s3_client;
            async move {
                client
                    .delete_bucket_website()
                    .bucket(identifier)
                    .send()
                    .await
            }
        })
        .await;

        match result {
            Ok(_) => Ok(()),
            Err(e)
                if is_s3_not_configured_error(&e, "NoSuchWebsiteConfiguration")
                    || is_s3_not_configured_error(&e, "NoSuchBucket") =>
            {
                Ok(())
            }
            Err(e) => Err(api_error_with_meta(
                "Failed to delete bucket website",
                "s3.DeleteBucketWebsite",
                e,
            )
            .for_resource(id.clone())),
        }
    }
}
