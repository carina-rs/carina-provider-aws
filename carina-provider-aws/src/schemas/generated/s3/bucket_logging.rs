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
            .with_coexisting_replacement()
            // Read contract: GetBucketLogging does not return the targeted bucket name
            .attribute(
                AttributeSchema::required("bucket", AttributeType::string())
                    .create_only()
                    .write_only()
                    .with_description(
                        "The name of the bucket for which to set the logging parameters.",
                    )
                    .with_provider_name("Bucket")
                    .build(),
            )
            .attribute(
                AttributeSchema::required("target_bucket", AttributeType::string())
                    .with_description("Destination bucket for server access logs.")
                    .with_provider_name("TargetBucket")
                    .build(),
            )
            .attribute(
                AttributeSchema::optional("target_prefix", AttributeType::string())
                    .with_description("Key prefix to apply to log objects.")
                    .with_provider_name("TargetPrefix")
                    .build(),
            )
            .attribute(
                AttributeSchema::optional(
                    "target_object_key_format",
                    super::bucket_target_object_key_format(),
                )
                .with_description("Partitioning / simple-prefix selector for log object keys.")
                .with_provider_name("TargetObjectKeyFormat")
                .build(),
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
