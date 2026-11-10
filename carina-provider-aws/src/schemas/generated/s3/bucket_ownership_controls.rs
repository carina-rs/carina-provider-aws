//! BucketOwnershipControls schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.s3
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};

/// Returns the schema config for s3.BucketOwnershipControls (Smithy: com.amazonaws.s3)
pub fn s3_bucket_ownership_controls_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::S3::BucketOwnershipControls",
        resource_type_name: "s3.BucketOwnershipControls",
        has_tags: false,
        await_on_read_back: vec![],
        schema: ResourceSchema::new("s3.BucketOwnershipControls")
        .with_coexisting_replacement()
        .attribute(
            AttributeSchema::required("bucket", AttributeType::string())
                .create_only()
                .with_description("The name of the Amazon S3 bucket whose OwnershipControls you want to set.")
                .with_provider_name("Bucket")
                .build(),
        )
        .attribute(
            AttributeSchema::required("object_ownership", carina_aws_types::with_member_enum_values(AttributeType::string(), carina_core::schema::enum_identity("ObjectOwnership", Some("aws.s3.BucketOwnershipControls")), &["BucketOwnerPreferred", "ObjectWriter", "BucketOwnerEnforced"], &[("BucketOwnerPreferred", "bucket_owner_preferred"), ("ObjectWriter", "object_writer"), ("BucketOwnerEnforced", "bucket_owner_enforced")]))
                .with_description("Object ownership setting: BucketOwnerEnforced, BucketOwnerPreferred, or ObjectWriter.")
                .with_provider_name("ObjectOwnership")
                .build(),
        )
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("s3.BucketOwnershipControls", &[])
}
