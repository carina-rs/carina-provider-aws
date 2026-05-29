//! BucketAcl schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.s3
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};

const VALID_ACL: &[&str] = &[
    "authenticated-read",
    "private",
    "public-read",
    "public-read-write",
    "authenticated_read",
    "public_read",
    "public_read_write",
];

/// Returns the schema config for s3.BucketAcl (Smithy: com.amazonaws.s3)
pub fn s3_bucket_acl_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::S3::BucketAcl",
        resource_type_name: "s3.BucketAcl",
        has_tags: false,
        schema: ResourceSchema::new("s3.BucketAcl")
            .attribute(
                AttributeSchema::new(
                    "acl",
                    AttributeType::string_enum(
                        "Acl".to_string(),
                        vec![
                            "authenticated-read".to_string(),
                            "private".to_string(),
                            "public-read".to_string(),
                            "public-read-write".to_string(),
                        ],
                        Some(carina_core::schema::string_enum_identity(
                            "Acl",
                            Some("aws.s3.BucketAcl"),
                        )),
                        vec![
                            (
                                "authenticated-read".to_string(),
                                "authenticated_read".to_string(),
                            ),
                            ("public-read".to_string(), "public_read".to_string()),
                            (
                                "public-read-write".to_string(),
                                "public_read_write".to_string(),
                            ),
                            ("private".to_string(), "private".to_string()),
                        ],
                    ),
                )
                .required()
                .with_description("The canned ACL to apply to the bucket.")
                .with_provider_name("ACL"),
            )
            .attribute(
                AttributeSchema::new("bucket", AttributeType::string())
                    .required()
                    .create_only()
                    .with_description("The bucket to which to apply the ACL.")
                    .with_provider_name("Bucket"),
            ),
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("s3.BucketAcl", &[("acl", VALID_ACL)])
}

/// Maps DSL alias values back to canonical AWS values for this module.
/// e.g., ("ip_protocol", "all") -> Some("-1")
pub fn enum_alias_reverse(attr_name: &str, value: &str) -> Option<&'static str> {
    match (attr_name, value) {
        ("acl", "authenticated_read") => Some("authenticated-read"),
        ("acl", "public_read") => Some("public-read"),
        ("acl", "public_read_write") => Some("public-read-write"),
        _ => None,
    }
}

/// Returns all enum alias entries as (attr_name, alias, canonical) tuples.
pub fn enum_alias_entries() -> &'static [(&'static str, &'static str, &'static str)] {
    &[
        ("acl", "authenticated_read", "authenticated-read"),
        ("acl", "public_read", "public-read"),
        ("acl", "public_read_write", "public-read-write"),
    ]
}
