//! BucketReplicationConfiguration schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.s3
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};

/// Returns the schema config for s3.BucketReplicationConfiguration (Smithy: com.amazonaws.s3)
pub fn s3_bucket_replication_configuration_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::S3::BucketReplicationConfiguration",
        resource_type_name: "s3.BucketReplicationConfiguration",
        has_tags: false,
        schema: ResourceSchema::new("s3.BucketReplicationConfiguration")
            .with_coexisting_replacement()
            // Read contract: GetBucketReplication does not return the targeted bucket name
            .attribute(
                AttributeSchema::required("bucket", AttributeType::string())
                    .create_only()
                    .write_only()
                    .with_description("The name of the bucket")
                    .with_provider_name("Bucket")
                    .build(),
            )
            .attribute(
                AttributeSchema::required("role", AttributeType::string())
                    .with_description("ARN of the IAM role S3 uses to perform the replication.")
                    .with_provider_name("Role")
                    .build(),
            )
            .attribute(
                AttributeSchema::required("rules", super::bucket_replication_rules())
                    .with_description("Replication rules — at least one is required.")
                    .with_provider_name("Rules")
                    .build(),
            ),
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("s3.BucketReplicationConfiguration", &[])
}
