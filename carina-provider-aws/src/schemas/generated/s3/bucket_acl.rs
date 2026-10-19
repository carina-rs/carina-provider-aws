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
];

/// Returns the schema config for s3.BucketAcl (Smithy: com.amazonaws.s3)
pub fn s3_bucket_acl_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::S3::BucketAcl",
        resource_type_name: "s3.BucketAcl",
        has_tags: false,
        schema: ResourceSchema::new("s3.BucketAcl")
            .with_coexisting_replacement()
            .attribute(
                AttributeSchema::new(
                    "acl",
                    AttributeType::enum_(
                        carina_core::schema::enum_identity("Acl", Some("aws.s3.BucketAcl")),
                        Some(vec![
                            "authenticated-read".to_string(),
                            "private".to_string(),
                            "public-read".to_string(),
                            "public-read-write".to_string(),
                        ]),
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
                        None,
                        None,
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
