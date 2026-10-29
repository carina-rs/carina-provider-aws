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
        .with_coexisting_replacement()
        .attribute(
            AttributeSchema::required("bucket", AttributeType::string())
                .create_only()
                .with_description("The bucket whose Object Lock configuration you want to create or replace.")
                .with_provider_name("Bucket")
                .build(),
        )
        .attribute(
            AttributeSchema::required("object_lock_enabled", super::s3_object_lock_enabled())
                .with_description("Whether S3 Object Lock is enabled for the bucket.")
                .with_provider_name("ObjectLockEnabled")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("rule", super::bucket_object_lock_rule())
                .with_description("Optional bucket-default Object Lock retention rule. If `default_retention` is set, its `retention_period` must contain exactly one of `days` or `years...")
                .with_provider_name("Rule")
                .build(),
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
