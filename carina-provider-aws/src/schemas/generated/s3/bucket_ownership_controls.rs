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
        schema: ResourceSchema::new("s3.BucketOwnershipControls")
        .attribute(
            AttributeSchema::new("bucket", AttributeType::string())
                .required()
                .create_only()
                .with_description("The name of the Amazon S3 bucket whose OwnershipControls you want to set.")
                .with_provider_name("Bucket"),
        )
        .attribute(
            AttributeSchema::new("object_ownership", AttributeType::enum_(carina_core::schema::enum_identity("ObjectOwnership", Some("aws.s3.BucketOwnershipControls")), Some(vec!["BucketOwnerEnforced".to_string(), "BucketOwnerPreferred".to_string(), "ObjectWriter".to_string()]), vec![("BucketOwnerEnforced".to_string(), "bucket_owner_enforced".to_string()), ("BucketOwnerPreferred".to_string(), "bucket_owner_preferred".to_string()), ("ObjectWriter".to_string(), "object_writer".to_string())], None, None))
                .required()
                .with_description("Object ownership setting: BucketOwnerEnforced, BucketOwnerPreferred, or ObjectWriter.")
                .with_provider_name("ObjectOwnership"),
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
