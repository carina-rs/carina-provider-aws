//! BucketPublicAccessBlock schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.s3
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};

/// Returns the schema config for s3.BucketPublicAccessBlock (Smithy: com.amazonaws.s3)
pub fn s3_bucket_public_access_block_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::S3::BucketPublicAccessBlock",
        resource_type_name: "s3.BucketPublicAccessBlock",
        has_tags: false,
        schema: ResourceSchema::new("s3.BucketPublicAccessBlock")
        .attribute(
            AttributeSchema::new("bucket", AttributeType::String)
                .required()
                .create_only()
                .with_description("The name of the Amazon S3 bucket whose PublicAccessBlock configuration you want to set.")
                .with_provider_name("Bucket"),
        )
        .attribute(
            AttributeSchema::new("block_public_acls", AttributeType::Bool)
                .with_description("Block public ACLs on the bucket and its objects.")
                .with_provider_name("BlockPublicAcls"),
        )
        .attribute(
            AttributeSchema::new("ignore_public_acls", AttributeType::Bool)
                .with_description("Ignore any public ACLs on the bucket and its objects.")
                .with_provider_name("IgnorePublicAcls"),
        )
        .attribute(
            AttributeSchema::new("block_public_policy", AttributeType::Bool)
                .with_description("Block public bucket policies for the bucket.")
                .with_provider_name("BlockPublicPolicy"),
        )
        .attribute(
            AttributeSchema::new("restrict_public_buckets", AttributeType::Bool)
                .with_description("Restrict access to the bucket to AWS service principals and authorized users when a public policy is set.")
                .with_provider_name("RestrictPublicBuckets"),
        )
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("s3.BucketPublicAccessBlock", &[])
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
