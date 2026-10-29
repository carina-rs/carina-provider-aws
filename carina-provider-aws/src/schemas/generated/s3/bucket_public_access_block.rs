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
        .with_coexisting_replacement()
        // Read contract: GetPublicAccessBlock does not return the targeted bucket name
        .attribute(
            AttributeSchema::required("bucket", AttributeType::string())
                .create_only()
                .write_only()
                .with_description("The name of the Amazon S3 bucket whose PublicAccessBlock configuration you want to set.")
                .with_provider_name("Bucket")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("block_public_acls", AttributeType::bool())
                .with_description("Block public ACLs on the bucket and its objects.")
                .with_provider_name("BlockPublicAcls")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("ignore_public_acls", AttributeType::bool())
                .with_description("Ignore any public ACLs on the bucket and its objects.")
                .with_provider_name("IgnorePublicAcls")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("block_public_policy", AttributeType::bool())
                .with_description("Block public bucket policies for the bucket.")
                .with_provider_name("BlockPublicPolicy")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("restrict_public_buckets", AttributeType::bool())
                .with_description("Restrict access to the bucket to AWS service principals and authorized users when a public policy is set.")
                .with_provider_name("RestrictPublicBuckets")
                .build(),
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
