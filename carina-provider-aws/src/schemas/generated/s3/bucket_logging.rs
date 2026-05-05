//! BucketLogging schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.s3
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};

/// Returns the schema config for s3.BucketLogging (Smithy: com.amazonaws.s3)
pub fn s3_bucket_logging_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::S3::BucketLogging",
        resource_type_name: "s3.BucketLogging",
        has_tags: false,
        schema: ResourceSchema::new("s3.BucketLogging")
            .attribute(
                AttributeSchema::new("bucket", AttributeType::String)
                    .required()
                    .create_only()
                    .with_description(
                        "The name of the bucket for which to set the logging parameters.",
                    )
                    .with_provider_name("Bucket"),
            )
            .attribute(
                AttributeSchema::new("target_bucket", AttributeType::String)
                    .required()
                    .with_description("Destination bucket for server access logs.")
                    .with_provider_name("TargetBucket"),
            )
            .attribute(
                AttributeSchema::new("target_prefix", AttributeType::String)
                    .with_description("Key prefix to apply to log objects.")
                    .with_provider_name("TargetPrefix"),
            )
            .attribute(
                AttributeSchema::new(
                    "target_object_key_format",
                    super::bucket_target_object_key_format(),
                )
                .with_description("Partitioning / simple-prefix selector for log object keys.")
                .with_provider_name("TargetObjectKeyFormat"),
            ),
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("s3.BucketLogging", &[])
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
