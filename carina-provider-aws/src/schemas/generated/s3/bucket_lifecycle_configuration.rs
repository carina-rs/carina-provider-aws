//! BucketLifecycleConfiguration schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.s3
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};

/// Returns the schema config for s3.BucketLifecycleConfiguration (Smithy: com.amazonaws.s3)
pub fn s3_bucket_lifecycle_configuration_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::S3::BucketLifecycleConfiguration",
        resource_type_name: "s3.BucketLifecycleConfiguration",
        has_tags: false,
        schema: ResourceSchema::new("s3.BucketLifecycleConfiguration")
            .attribute(
                AttributeSchema::new("bucket", AttributeType::string())
                    .required()
                    .create_only()
                    .with_description("The name of the bucket for which to set the configuration.")
                    .with_provider_name("Bucket"),
            )
            .attribute(
                AttributeSchema::new("rules", super::bucket_lifecycle_rules())
                    .required()
                    .with_description("Lifecycle rules to apply to the bucket.")
                    .with_provider_name("Rules"),
            ),
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("s3.BucketLifecycleConfiguration", &[])
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
