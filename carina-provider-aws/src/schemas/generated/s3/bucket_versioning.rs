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
        schema: ResourceSchema::new("s3.BucketVersioning")
        .attribute(
            AttributeSchema::new("bucket", AttributeType::string())
                .required()
                .create_only()
                .with_description("The bucket name.")
                .with_provider_name("Bucket"),
        )
        .attribute(
            AttributeSchema::new("status", AttributeType::enum_(carina_core::schema::enum_identity("VersioningStatus", Some("aws.s3.BucketVersioning")), Some(vec!["Enabled".to_string(), "Suspended".to_string()]), vec![("Enabled".to_string(), "enabled".to_string()), ("Suspended".to_string(), "suspended".to_string())], None, None))
                .required()
                .with_description("Versioning state of the bucket: Enabled or Suspended.")
                .with_provider_name("Status"),
        )
        .attribute(
            AttributeSchema::new("mfa_delete", AttributeType::enum_(carina_core::schema::enum_identity("MFADelete", Some("aws.s3.BucketVersioning")), Some(vec!["Enabled".to_string(), "Disabled".to_string()]), vec![("Enabled".to_string(), "enabled".to_string()), ("Disabled".to_string(), "disabled".to_string())], None, None))
                .with_description("MFA-delete state. Specifies whether MFA delete is enabled in the bucket versioning configuration.")
                .with_provider_name("MFADelete"),
        )
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("s3.BucketVersioning", &[])
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
