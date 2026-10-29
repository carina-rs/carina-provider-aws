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
        .with_coexisting_replacement()
        .attribute(
            AttributeSchema::required("bucket", AttributeType::string())
                .create_only()
                .with_description("Specifies default encryption for a bucket using server-side encryption with different key options. Directory buckets - When you use this operation wit...")
                .with_provider_name("Bucket")
                .build(),
        )
        .attribute(
            AttributeSchema::required("rules", super::bucket_encryption_rules())
                .with_description("List of server-side encryption rules to apply to the bucket.")
                .with_provider_name("Rules")
                .build(),
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
