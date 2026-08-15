//! Bucket schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.s3
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use super::tags_type;
use super::validate_tags_map;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};

const VALID_BUCKET_NAMESPACE: &[&str] = &["account-regional", "global"];

pub fn arn() -> AttributeType {
    AttributeType::refined_string(
        Some(super::provider_type("s3", "Bucket", "Arn")),
        Some("^arn:(aws|aws-cn|aws-us-gov):s3:::.+$".to_string()),
        None,
        None,
    )
}

/// Returns the schema config for s3.Bucket (Smithy: com.amazonaws.s3)
pub fn s3_bucket_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::S3::Bucket",
        resource_type_name: "s3.Bucket",
        has_tags: true,
        schema: ResourceSchema::new("s3.Bucket")
        .with_unique_name_attribute("bucket")
        .attribute(
            AttributeSchema::new("bucket", AttributeType::string())
                .required()
                .create_only()
                .with_description("The name of the bucket to create. General purpose buckets - For information about bucket naming restrictions, see Bucket naming rules in the Amazon S3...")
                .with_provider_name("Bucket"),
        )
        .attribute(
            AttributeSchema::new("bucket_namespace", AttributeType::enum_(
    carina_core::schema::enum_identity("BucketNamespace", Some("aws.s3.Bucket")),
    Some(vec!["account-regional".to_string(), "global".to_string()]),
    vec![("account-regional".to_string(), "account_regional".to_string()), ("global".to_string(), "global".to_string())],
    None,
    None,
))
                .create_only()
                .with_description("Specifies the namespace where you want to create your general purpose bucket. When you create a general purpose bucket, you can choose to create a buc...")
                .with_provider_name("BucketNamespace"),
        )
        .attribute(
            AttributeSchema::new("arn", self::arn())
                .read_only()
                .with_description("ARN of the bucket. (read-only)")
                .with_provider_name("Arn"),
        )
        .attribute(
            AttributeSchema::new("region", AttributeType::string())
                .read_only()
                .with_description("AWS region the bucket is in. (read-only)")
                .with_provider_name("Region"),
        )
        .attribute(
            AttributeSchema::new("bucket_domain_name", AttributeType::string())
                .read_only()
                .with_description("Bucket domain name, in the form NAME.s3.amazonaws.com. (read-only)")
                .with_provider_name("BucketDomainName"),
        )
        .attribute(
            AttributeSchema::new("bucket_regional_domain_name", AttributeType::string())
                .read_only()
                .with_description("Region-specific bucket domain name, in the form NAME.s3.REGION.amazonaws.com. (read-only)")
                .with_provider_name("BucketRegionalDomainName"),
        )
        .attribute(
            AttributeSchema::new("hosted_zone_id", AttributeType::string())
                .read_only()
                .with_description("Route 53 Hosted Zone ID for the bucket's region. (read-only)")
                .with_provider_name("HostedZoneId"),
        )
        .attribute(
            AttributeSchema::new("tags", tags_type())
                .with_description("The tags for the resource.")
                .with_provider_name("Tags"),
        )
        .with_validator(validate_tags_map)
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("s3.Bucket", &[("bucket_namespace", VALID_BUCKET_NAMESPACE)])
}

/// Maps DSL alias values back to canonical AWS values for this module.
/// e.g., ("ip_protocol", "all") -> Some("-1")
pub fn enum_alias_reverse(attr_name: &str, value: &str) -> Option<&'static str> {
    match (attr_name, value) {
        ("bucket_namespace", "account_regional") => Some("account-regional"),
        _ => None,
    }
}

/// Returns all enum alias entries as (attr_name, alias, canonical) tuples.
pub fn enum_alias_entries() -> &'static [(&'static str, &'static str, &'static str)] {
    &[("bucket_namespace", "account_regional", "account-regional")]
}
