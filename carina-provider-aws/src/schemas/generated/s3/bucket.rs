//! Bucket schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.s3
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use super::tags_type;
use super::validate_tags_map;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};

const VALID_BUCKET_NAMESPACE: &[&str] = &["account-regional", "global", "account_regional"];

/// Returns the schema config for s3.Bucket (Smithy: com.amazonaws.s3)
pub fn s3_bucket_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::S3::Bucket",
        resource_type_name: "s3.Bucket",
        has_tags: true,
        schema: ResourceSchema::new("s3.Bucket")
        .attribute(
            AttributeSchema::new("bucket", AttributeType::String)
                .required()
                .create_only()
                .with_description("The name of the bucket to create. General purpose buckets - For information about bucket naming restrictions, see Bucket naming rules in the Amazon S3...")
                .with_provider_name("Bucket"),
        )
        .attribute(
            AttributeSchema::new("bucket_namespace", AttributeType::StringEnum {
                name: "BucketNamespace".to_string(),
                values: vec!["account-regional".to_string(), "global".to_string()],
                namespace: Some("aws.s3.Bucket".to_string()),
                dsl_aliases: vec![("account-regional".to_string(), "account_regional".to_string())],
            })
                .create_only()
                .with_description("Specifies the namespace where you want to create your general purpose bucket. When you create a general purpose bucket, you can choose to create a buc...")
                .with_provider_name("BucketNamespace"),
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
