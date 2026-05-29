//! Bucket schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.s3
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};

/// Returns the schema config for s3.Bucket (Smithy: com.amazonaws.s3)
pub fn s3_bucket_data_source_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::S3::Bucket",
        resource_type_name: "s3.Bucket",
        has_tags: false,
        schema: ResourceSchema::new("s3.Bucket")
        .as_data_source()
        .attribute(
            AttributeSchema::new("bucket", AttributeType::string())
            .required()
                .with_description("Name of the S3 bucket to look up.")
                .with_provider_name("Bucket"),
        )
        .attribute(
            AttributeSchema::new("arn", super::arn())
                .with_description("ARN of the bucket.")
                .with_provider_name(""),
        )
        .attribute(
            AttributeSchema::new("region", AttributeType::string())
                .with_description("AWS region the bucket is in.")
                .with_provider_name("LocationConstraint"),
        )
        .attribute(
            AttributeSchema::new("bucket_domain_name", AttributeType::string())
                .with_description("Bucket domain name (`<bucket>.s3.amazonaws.com`).")
                .with_provider_name(""),
        )
        .attribute(
            AttributeSchema::new("bucket_regional_domain_name", AttributeType::string())
                .with_description("Region-specific bucket domain name (`<bucket>.s3.<region>.amazonaws.com`).")
                .with_provider_name(""),
        )
        .attribute(
            AttributeSchema::new("hosted_zone_id", AttributeType::string())
                .with_description("Route 53 Hosted Zone ID for the bucket's region.")
                .with_provider_name(""),
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
