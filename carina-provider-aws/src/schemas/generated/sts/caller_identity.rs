//! CallerIdentity schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.sts
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};

pub fn arn() -> AttributeType {
    super::sts_caller_identity_arn()
}

/// Returns the schema config for sts.CallerIdentity (Smithy: com.amazonaws.sts)
pub fn sts_caller_identity_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::STS::CallerIdentity",
        resource_type_name: "sts.CallerIdentity",
        has_tags: false,
        schema: ResourceSchema::new("sts.CallerIdentity")
        .as_data_source()
        .attribute(
            AttributeSchema::provider_populated("account_id", super::aws_account_id())
                .with_description("The Amazon Web Services account ID number of the account that owns or contains the calling entity.")
                .with_provider_name("Account")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("arn", self::arn())
                .with_description("The Amazon Web Services ARN associated with the calling entity.")
                .with_provider_name("Arn")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("user_id", AttributeType::string())
                .with_description("The unique identifier of the calling entity.")
                .with_provider_name("UserId")
                .build(),
        )
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("sts.CallerIdentity", &[])
}
