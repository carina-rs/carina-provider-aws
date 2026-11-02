//! BucketVersioning schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.s3
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};

/// Returns the schema config for s3.BucketVersioning (Smithy: com.amazonaws.s3)
pub fn s3_bucket_versioning_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::S3::BucketVersioning",
        resource_type_name: "s3.BucketVersioning",
        has_tags: false,
        await_on_read_back: vec![],
        schema: ResourceSchema::new("s3.BucketVersioning")
            .with_coexisting_replacement()
            .attribute(
                AttributeSchema::required("bucket", AttributeType::string())
                    .create_only()
                    .with_description("The bucket name.")
                    .with_provider_name("Bucket")
                    .build(),
            )
            .attribute(
                AttributeSchema::required("status", carina_aws_types::s3_versioning_status())
                    .with_description("Versioning state of the bucket: Enabled or Suspended.")
                    .with_provider_name("Status")
                    .build(),
            ),
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("s3.BucketVersioning", &[])
}
