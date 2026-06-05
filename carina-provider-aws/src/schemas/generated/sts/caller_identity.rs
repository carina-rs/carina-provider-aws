//! CallerIdentity schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.sts
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema, legacy_validator};

pub fn arn() -> AttributeType {
    AttributeType::custom(
        Some(super::provider_type("sts", "CallerIdentity", "Arn")),
        super::arn(),
        Some("^arn:(aws|aws-cn|aws-us-gov):sts:.*$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                super::validate_service_arn(s, "sts", None)
                    .map_err(|reason| format!("Invalid sts ARN '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
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
            AttributeSchema::new("account_id", super::aws_account_id())
                .with_description("The Amazon Web Services account ID number of the account that owns or contains the calling entity.")
                .with_provider_name("Account"),
        )
        .attribute(
            AttributeSchema::new("arn", self::arn())
                .with_description("The Amazon Web Services ARN associated with the calling entity.")
                .with_provider_name("Arn"),
        )
        .attribute(
            AttributeSchema::new("user_id", AttributeType::string())
                .with_description("The unique identifier of the calling entity.")
                .with_provider_name("UserId"),
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
