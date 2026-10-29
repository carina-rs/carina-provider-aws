use aws_sdk_s3::types::{
    ErrorDocument, IndexDocument, Protocol, RedirectAllRequestsTo, WebsiteConfiguration,
};
use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, State, Value};
use carina_core::schema::ResourceSchema;
use indexmap::IndexMap;

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::{
    RetryPolicy, optional_enum_struct_field, require_string_attr, retry_aws_operation,
    sdk_error_message,
};
use crate::provider::CreateCompletion;
use crate::read_model::found_managed_read;
use crate::read_models_generated::S3BucketWebsiteConfigurationReadModel;
use crate::services::s3::bucket::is_s3_not_configured_error;

impl AwsProvider {
    pub(crate) async fn read_s3_bucket_website_configuration(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        let bucket = identifier.as_str();

        let result = self
            .s3_client
            .get_bucket_website()
            .bucket(bucket)
            .send()
            .await;

        match result {
            Ok(output) => {
                let index_document = output.index_document().map(|idx| {
                    let mut m = IndexMap::new();
                    m.insert(
                        "suffix".to_string(),
                        Value::Concrete(ConcreteValue::String(idx.suffix().to_string().into())),
                    );
                    Value::Concrete(ConcreteValue::Map(m))
                });
                let error_document = output.error_document().map(|err| {
                    let mut m = IndexMap::new();
                    m.insert(
                        "key".to_string(),
                        Value::Concrete(ConcreteValue::String(err.key().to_string().into())),
                    );
                    Value::Concrete(ConcreteValue::Map(m))
                });
                let redirect_all_requests_to = output.redirect_all_requests_to().map(|r| {
                    let mut m = IndexMap::new();
                    m.insert(
                        "host_name".to_string(),
                        Value::Concrete(ConcreteValue::String(r.host_name().to_string().into())),
                    );
                    if let Some(p) = r.protocol() {
                        m.insert(
                            "protocol".to_string(),
                            Value::Concrete(ConcreteValue::String(p.as_str().to_string().into())),
                        );
                    }
                    Value::Concrete(ConcreteValue::Map(m))
                });
                found_managed_read(
                    id,
                    identifier,
                    S3BucketWebsiteConfigurationReadModel {
                        index_document,
                        error_document,
                        redirect_all_requests_to,
                    },
                )
            }
            Err(e) => {
                if is_s3_not_configured_error(&e, "NoSuchWebsiteConfiguration")
                    || is_s3_not_configured_error(&e, "NoSuchBucket")
                {
                    return Ok(ReadOutcome::not_found(id.clone()));
                }
                Err(
                    api_error_with_meta("Failed to get bucket website configuration", e)
                        .for_resource(id.clone()),
                )
            }
        }
    }

    pub(crate) async fn create_s3_bucket_website_configuration(
        &self,
        resource: &Resource<impl Sized>,
        schema: &ResourceSchema,
    ) -> ProviderResult<CreateCompletion> {
        let bucket = require_string_attr(resource, "bucket")?;
        let identifier =
            crate::helpers::applied_identifier(&resource.id, "s3.PutBucketWebsite", bucket)?;
        let outcome = self
            .put_s3_bucket_website(&resource.id, &identifier, resource, schema)
            .await?;
        Ok(self.create_outcome_from_read(
            &resource.id,
            identifier,
            resource.resolved_attributes(),
            outcome,
        ))
    }

    pub(crate) async fn update_s3_bucket_website_configuration(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        _from: &State,
        to: Resource,
        schema: &ResourceSchema,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        self.put_s3_bucket_website(&id, identifier, &to, schema)
            .await
    }

    async fn put_s3_bucket_website(
        &self,
        id: &ResourceId,
        bucket: &AppliedIdentifier,
        resource: &Resource<impl Sized>,
        schema: &ResourceSchema,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        let applied_identifier = bucket;
        let bucket = bucket.as_str();
        let config = build_website_configuration(id, resource, schema)?;

        self.s3_client
            .put_bucket_website()
            .bucket(bucket)
            .website_configuration(config)
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to put bucket website", e).for_resource(id.clone())
            })?;

        Ok(self
            .read_s3_bucket_website_configuration(id, applied_identifier)
            .await)
    }

    pub(crate) async fn delete_s3_bucket_website_configuration_idempotent(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        let identifier = identifier.as_str();
        let result = retry_aws_operation(RetryPolicy::default(), || {
            let client = &self.s3_client;
            client.delete_bucket_website().bucket(identifier)
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
            Err(e) => {
                Err(api_error_with_meta("Failed to delete bucket website", e)
                    .for_resource(id.clone()))
            }
        }
    }
}

fn build_website_configuration(
    id: &ResourceId,
    resource: &Resource<impl Sized>,
    schema: &ResourceSchema,
) -> ProviderResult<WebsiteConfiguration> {
    let mut config_builder = WebsiteConfiguration::builder();

    match resource.get_attr("index_document") {
        Some(Value::Concrete(ConcreteValue::Map(map))) => {
            let suffix = match map.get("suffix") {
                Some(Value::Concrete(ConcreteValue::String(s))) => s.expose_plaintext(),
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
        Some(_) => {
            return Err(ProviderError::invalid_input("index_document must be a map")
                .for_resource(id.clone()));
        }
        None => {}
    }

    match resource.get_attr("error_document") {
        Some(Value::Concrete(ConcreteValue::Map(map))) => {
            let key = match map.get("key") {
                Some(Value::Concrete(ConcreteValue::String(s))) => s.expose_plaintext(),
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
        Some(_) => {
            return Err(ProviderError::invalid_input("error_document must be a map")
                .for_resource(id.clone()));
        }
        None => {}
    }

    match resource.get_attr("redirect_all_requests_to") {
        Some(Value::Concrete(ConcreteValue::Map(map))) => {
            let host_name = match map.get("host_name") {
                Some(Value::Concrete(ConcreteValue::String(s))) => s.expose_plaintext(),
                _ => {
                    return Err(ProviderError::invalid_input(
                        "redirect_all_requests_to.host_name is required",
                    )
                    .for_resource(id.clone()));
                }
            };
            let mut rb = RedirectAllRequestsTo::builder().host_name(host_name);
            if let Some(protocol) = optional_enum_struct_field(
                resource,
                schema,
                "redirect_all_requests_to",
                "protocol",
            )? {
                rb = rb.protocol(Protocol::from(protocol));
            }
            config_builder = config_builder.redirect_all_requests_to(rb.build().map_err(|e| {
                ProviderError::api_error(sdk_error_message(
                    "Failed to build RedirectAllRequestsTo",
                    &e,
                ))
                .for_resource(id.clone())
            })?);
        }
        Some(_) => {
            return Err(
                ProviderError::invalid_input("redirect_all_requests_to must be a map")
                    .for_resource(id.clone()),
            );
        }
        None => {}
    }

    Ok(config_builder.build())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema() -> ResourceSchema {
        crate::schemas::generated::s3::bucket_website_configuration::s3_bucket_website_configuration_config().schema
    }

    fn resource_with(attr: &str, value: Value) -> Resource {
        let mut resource =
            Resource::with_provider("aws", "s3.BucketWebsiteConfiguration", "test", None);
        resource.set_attr(attr.to_string(), value);
        resource
    }

    #[test]
    fn non_map_index_document_is_rejected() {
        let resource = resource_with("index_document", Value::Concrete(ConcreteValue::Bool(true)));

        build_website_configuration(&resource.id, &resource, &schema())
            .expect_err("a present index_document must be a map");
    }

    #[test]
    fn non_map_error_document_is_rejected() {
        let resource = resource_with("error_document", Value::Concrete(ConcreteValue::Bool(true)));

        build_website_configuration(&resource.id, &resource, &schema())
            .expect_err("a present error_document must be a map");
    }

    #[test]
    fn non_map_redirect_all_requests_to_is_rejected() {
        let resource = resource_with(
            "redirect_all_requests_to",
            Value::Concrete(ConcreteValue::Bool(true)),
        );

        build_website_configuration(&resource.id, &resource, &schema())
            .expect_err("a present redirect_all_requests_to must be a map");
    }
}
