//! BucketPolicy schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.s3
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};

/// Returns the schema config for s3.BucketPolicy (Smithy: com.amazonaws.s3)
pub fn s3_bucket_policy_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::S3::BucketPolicy",
        resource_type_name: "s3.BucketPolicy",
        has_tags: false,
        schema: ResourceSchema::new("s3.BucketPolicy")
        .attribute(
            AttributeSchema::new("bucket", AttributeType::String)
                .required()
                .create_only()
                .with_description("The name of the bucket. Directory buckets - When you use this operation with a directory bucket, you must use path-style requests in the format https:...")
                .with_provider_name("Bucket"),
        )
        .attribute(
            AttributeSchema::new("policy", super::iam_policy_document())
                .required()
                .with_description("The bucket policy as a JSON document. For directory buckets, the only IAM action supported in the bucket policy is s3express:CreateSession.")
                .with_provider_name("Policy"),
        )
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("s3.BucketPolicy", &[])
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
