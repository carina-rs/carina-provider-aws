//! BucketWebsiteConfiguration schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.s3
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};

/// Returns the schema config for s3.BucketWebsiteConfiguration (Smithy: com.amazonaws.s3)
pub fn s3_bucket_website_configuration_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::S3::BucketWebsiteConfiguration",
        resource_type_name: "s3.BucketWebsiteConfiguration",
        has_tags: false,
        schema: ResourceSchema::new("s3.BucketWebsiteConfiguration")
        .attribute(
            AttributeSchema::new("bucket", AttributeType::String)
                .required()
                .create_only()
                .with_description("The bucket name.")
                .with_provider_name("Bucket"),
        )
        .attribute(
            AttributeSchema::new("index_document", super::s3_index_document())
                .with_description("Index document for the bucket's website.")
                .with_provider_name("IndexDocument"),
        )
        .attribute(
            AttributeSchema::new("error_document", super::s3_error_document())
                .with_description("Custom error document key.")
                .with_provider_name("ErrorDocument"),
        )
        .attribute(
            AttributeSchema::new("redirect_all_requests_to", super::s3_redirect_all_requests_to())
                .with_description("Redirect all bucket-website requests to another host (alternative to index_document).")
                .with_provider_name("RedirectAllRequestsTo"),
        )
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("s3.BucketWebsiteConfiguration", &[])
}

/// Maps DSL alias values back to canonical AWS values for this module.
/// e.g., ("ip_protocol", "all") -> Some("-1")
pub fn enum_alias_reverse(attr_name: &str, value: &str) -> Option<&'static str> {
    let _ = (attr_name, value);
    None
}

/// Returns all enum alias entries as (attr_name, alias, canonical) tuples.
pub fn enum_alias_entries() -> &'static [(&'static str, &'static str, &'static str)] {
    &[]
}
