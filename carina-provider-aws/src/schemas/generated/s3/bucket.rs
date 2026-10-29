//! Bucket schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.s3
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use super::tags_type;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};
use carina_provider_protocol::types::ValidatorType;

const VALID_BUCKET_NAMESPACE: &[&str] = &["account-regional", "global"];

pub fn arn() -> AttributeType {
    super::s3_bucket_arn()
}

/// Returns the schema config for s3.Bucket (Smithy: com.amazonaws.s3)
pub fn s3_bucket_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::S3::Bucket",
        resource_type_name: "s3.Bucket",
        has_tags: true,
        schema: ResourceSchema::new("s3.Bucket")
        .with_conflicting_replacement()
        .attribute(
            AttributeSchema::required("bucket", AttributeType::string())
                .create_only()
                .with_description("The name of the bucket to create. General purpose buckets - For information about bucket naming restrictions, see Bucket naming rules in the Amazon S3...")
                .with_provider_name("Bucket")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("bucket_namespace", AttributeType::enum_(
    carina_core::schema::enum_identity("BucketNamespace", Some("aws.s3.Bucket")),
    Some(vec!["account-regional".to_string(), "global".to_string()]),
    vec![("account-regional".to_string(), "account_regional".to_string()), ("global".to_string(), "global".to_string())],
    None,
    None,
))
                .create_only()
                .with_description("Specifies the namespace where you want to create your general purpose bucket. When you create a general purpose bucket, you can choose to create a buc...")
                .with_provider_name("BucketNamespace")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("arn", self::arn())
                .with_description("ARN of the bucket. (read-only)")
                .with_provider_name("Arn")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("region", AttributeType::string())
                .with_description("AWS region the bucket is in. (read-only)")
                .with_provider_name("Region")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("bucket_domain_name", AttributeType::string())
                .with_description("Bucket domain name, in the form NAME.s3.amazonaws.com. (read-only)")
                .with_provider_name("BucketDomainName")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("bucket_regional_domain_name", AttributeType::string())
                .with_description("Region-specific bucket domain name, in the form NAME.s3.REGION.amazonaws.com. (read-only)")
                .with_provider_name("BucketRegionalDomainName")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("hosted_zone_id", super::route53_hosted_zone_id())
                .with_description("Route 53 Hosted Zone ID for the bucket's region. (read-only)")
                .with_provider_name("HostedZoneId")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("tags", tags_type())
                .with_description("The tags for the resource.")
                .with_provider_name("Tags")
                .build(),
        )
        .with_declared_validators(vec![ValidatorType::TagsKeyValueCheck])
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("s3.Bucket", &[("bucket_namespace", VALID_BUCKET_NAMESPACE)])
}
