//! Bucket schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.s3
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};

pub fn arn() -> AttributeType {
    AttributeType::refined_string(
        Some(super::provider_type("s3", "Bucket", "Arn")),
        Some("^arn:(aws|aws-cn|aws-us-gov):s3:::.+$".to_string()),
        None,
        None,
    )
}

/// Returns the schema config for s3.Bucket (Smithy: com.amazonaws.s3)
pub fn s3_bucket_data_source_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::S3::Bucket",
        resource_type_name: "s3.Bucket",
        has_tags: false,
        schema: ResourceSchema::new("s3.Bucket")
        .as_data_source()
        .attribute(
            AttributeSchema::required("bucket", AttributeType::string())
                .with_description("Name of the S3 bucket to look up.")
                .with_provider_name("Bucket")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("arn", self::arn())
                .with_description("ARN of the bucket.")
                .with_provider_name("")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("region", AttributeType::string())
                .with_description("AWS region the bucket is in.")
                .with_provider_name("LocationConstraint")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("bucket_domain_name", AttributeType::string())
                .with_description("Bucket domain name (`<bucket>.s3.amazonaws.com`).")
                .with_provider_name("")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("bucket_regional_domain_name", AttributeType::string())
                .with_description("Region-specific bucket domain name (`<bucket>.s3.<region>.amazonaws.com`).")
                .with_provider_name("")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("hosted_zone_id", super::route53_hosted_zone_id())
                .with_description("Route 53 Hosted Zone ID for the bucket's region.")
                .with_provider_name("")
                .build(),
        )
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("s3.Bucket", &[])
}
