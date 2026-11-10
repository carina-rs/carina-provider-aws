//! CallerIdentity schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.sts
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};

pub fn arn() -> AttributeType {
    carina_aws_types::with_member_identity(
        AttributeType::string_member(
            carina_core::schema::StringIdentityLayer::new(None, None, None, None),
            carina_core::schema::StringMemberConstraints::new(
                Some(carina_core::schema::PatternSpec::new(
                    "^[\\u0009\\u000A\\u000D\\u0020-\\u007E\\u0085\\u00A0-\\uD7FF\\uE000-\\uFFFD\\u10000-\\u10FFFF]+$",
                    carina_core::schema::PatternMatch::Full,
                )),
                Some((Some(20), Some(2048))),
            ),
        ),
        carina_aws_types::AwsType::StsCallerIdentityArn.definition(),
    )
}

/// Returns the schema config for sts.CallerIdentity (Smithy: com.amazonaws.sts)
pub fn sts_caller_identity_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::STS::CallerIdentity",
        resource_type_name: "sts.CallerIdentity",
        has_tags: false,
        await_on_read_back: vec![],
        schema: ResourceSchema::new("sts.CallerIdentity")
        .as_data_source()
        .attribute(
            AttributeSchema::provider_populated("account_id", carina_aws_types::with_member_identity(AttributeType::string(), carina_aws_types::AwsType::AccountId.definition()))
                .with_description("The Amazon Web Services account ID number of the account that owns or contains the calling entity.")
                .with_provider_name("Account")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("arn", carina_aws_types::with_member_identity(AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(Some(carina_core::schema::PatternSpec::new("^[\\u0009\\u000A\\u000D\\u0020-\\u007E\\u0085\\u00A0-\\uD7FF\\uE000-\\uFFFD\\u10000-\\u10FFFF]+$", carina_core::schema::PatternMatch::Full)), Some((Some(20), Some(2048))))), carina_aws_types::AwsType::StsCallerIdentityArn.definition()))
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
