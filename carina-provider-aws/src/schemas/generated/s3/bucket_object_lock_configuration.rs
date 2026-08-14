//! BucketObjectLockConfiguration schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.s3
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};

/// Returns the schema config for s3.BucketObjectLockConfiguration (Smithy: com.amazonaws.s3)
pub fn s3_bucket_object_lock_configuration_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::S3::BucketObjectLockConfiguration",
        resource_type_name: "s3.BucketObjectLockConfiguration",
        has_tags: false,
        schema: ResourceSchema::new("s3.BucketObjectLockConfiguration")
        .with_unique_name_attribute("bucket")
        .attribute(
            AttributeSchema::new("bucket", AttributeType::string())
                .required()
                .create_only()
                .with_description("The bucket whose Object Lock configuration you want to create or replace.")
                .with_provider_name("Bucket"),
        )
        .attribute(
            AttributeSchema::new("object_lock_enabled", super::s3_object_lock_enabled())
                .required()
                .with_description("Whether S3 Object Lock is enabled for the bucket.")
                .with_provider_name("ObjectLockEnabled"),
        )
        .attribute(
            AttributeSchema::new("rule", super::bucket_object_lock_rule())
                .with_description("Optional bucket-default Object Lock retention rule. If `default_retention` is set, its `retention_period` must contain exactly one of `days` or `years...")
                .with_provider_name("Rule"),
        )
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("s3.BucketObjectLockConfiguration", &[])
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
