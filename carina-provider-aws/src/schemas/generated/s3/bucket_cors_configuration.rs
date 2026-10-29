//! BucketCorsConfiguration schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.s3
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};

/// Returns the schema config for s3.BucketCorsConfiguration (Smithy: com.amazonaws.s3)
pub fn s3_bucket_cors_configuration_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::S3::BucketCorsConfiguration",
        resource_type_name: "s3.BucketCorsConfiguration",
        has_tags: false,
        schema: ResourceSchema::new("s3.BucketCorsConfiguration")
            .with_coexisting_replacement()
            // Read contract: GetBucketCors does not return the targeted bucket name
            .attribute(
                AttributeSchema::required("bucket", AttributeType::string())
                    .create_only()
                    .write_only()
                    .with_description("Specifies the bucket impacted by the corsconfiguration.")
                    .with_provider_name("Bucket")
                    .build(),
            )
            .attribute(
                AttributeSchema::required("cors_rules", super::bucket_cors_rules())
                    .with_description("CORS rules to apply to the bucket.")
                    .with_provider_name("CORSRules")
                    .build(),
            ),
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("s3.BucketCorsConfiguration", &[])
}
