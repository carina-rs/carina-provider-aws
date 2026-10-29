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
        .with_coexisting_replacement()
        .attribute(
            AttributeSchema::required("bucket", AttributeType::string())
                .create_only()
                .with_description("The name of the bucket. Directory buckets - When you use this operation with a directory bucket, you must use path-style requests in the format https:...")
                .with_provider_name("Bucket")
                .build(),
        )
        .attribute(
            AttributeSchema::required("policy", super::iam_policy_document())
                .with_description("The bucket policy as a JSON document. For directory buckets, the only IAM action supported in the bucket policy is s3express:CreateSession.")
                .with_provider_name("Policy")
                .build(),
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
