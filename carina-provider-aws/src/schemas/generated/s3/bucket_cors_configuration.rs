//! BucketCorsConfiguration schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.s3
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};

/// Returns the schema config for s3.BucketCorsConfiguration (Smithy: com.amazonaws.s3)
pub fn s3_bucket_cors_configuration_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::S3::BucketCorsConfiguration",
        resource_type_name: "s3.BucketCorsConfiguration",
        has_tags: false,
        schema: ResourceSchema::new("s3.BucketCorsConfiguration")
            .attribute(
                AttributeSchema::new("bucket", AttributeType::String)
                    .required()
                    .create_only()
                    .with_description("Specifies the bucket impacted by the corsconfiguration.")
                    .with_provider_name("Bucket"),
            )
            .attribute(
                AttributeSchema::new("cors_rules", super::bucket_cors_rules())
                    .required()
                    .with_description("CORS rules to apply to the bucket.")
                    .with_provider_name("CORSRules"),
            ),
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("s3.BucketCorsConfiguration", &[])
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
