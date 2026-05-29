//! BucketServerSideEncryptionConfiguration schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.s3
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};

/// Returns the schema config for s3.BucketServerSideEncryptionConfiguration (Smithy: com.amazonaws.s3)
pub fn s3_bucket_server_side_encryption_configuration_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::S3::BucketServerSideEncryptionConfiguration",
        resource_type_name: "s3.BucketServerSideEncryptionConfiguration",
        has_tags: false,
        schema: ResourceSchema::new("s3.BucketServerSideEncryptionConfiguration")
        .attribute(
            AttributeSchema::new("bucket", AttributeType::string())
                .required()
                .create_only()
                .with_description("Specifies default encryption for a bucket using server-side encryption with different key options. Directory buckets - When you use this operation wit...")
                .with_provider_name("Bucket"),
        )
        .attribute(
            AttributeSchema::new("rules", super::bucket_encryption_rules())
                .required()
                .with_description("List of server-side encryption rules to apply to the bucket.")
                .with_provider_name("Rules"),
        )
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("s3.BucketServerSideEncryptionConfiguration", &[])
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
