//! organizations.Organization schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.organizations
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{
    AttributeSchema, AttributeType, ResourceSchema, legacy_validator, types,
};

const VALID_FEATURE_SET: &[&str] = &["ALL", "CONSOLIDATED_BILLING", "all", "consolidated_billing"];

pub fn arn() -> AttributeType {
    AttributeType::custom(
        Some(super::provider_type("organizations", "Organization", "Arn")),
        super::arn(),
        Some("^arn:(aws|aws-cn|aws-us-gov):organizations:.*$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                super::validate_service_arn(s, "organizations", None)
                    .map_err(|reason| format!("Invalid organizations ARN '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

/// Returns the schema config for organizations.Organization (Smithy: com.amazonaws.organizations)
pub fn organizations_organization_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::Organizations::Organization",
        resource_type_name: "organizations.Organization",
        has_tags: false,
        schema: ResourceSchema::new("organizations.Organization")
        .with_description("Contains details about an organization. An organization is a collection of accounts that are centrally managed together using consolidated billing, organized hierarchically with organizational units (...")
        .attribute(
            AttributeSchema::new("feature_set", AttributeType::string_enum(
                "FeatureSet".to_string(),
                vec!["ALL".to_string(), "CONSOLIDATED_BILLING".to_string()],
                Some(carina_core::schema::string_enum_identity("FeatureSet", Some("aws.organizations.Organization"))),
                vec![("ALL".to_string(), "all".to_string()), ("CONSOLIDATED_BILLING".to_string(), "consolidated_billing".to_string())],
            ))
                .create_only()
                .with_description("Specifies the feature set supported by the new organization. Each feature set supports different levels of functionality. CONSOLIDATED_BILLING: All me...")
                .with_provider_name("FeatureSet"),
        )
        .attribute(
            AttributeSchema::new("arn", self::arn())
                .read_only()
                .with_description("The Amazon Resource Name (ARN) of an organization. For more information about ARNs in Organizations, see ARN Formats Supported by Organizations in the... (read-only)")
                .with_provider_name("Arn"),
        )
        .attribute(
            AttributeSchema::new("id", AttributeType::string())
                .read_only()
                .with_description("The unique identifier (ID) of an organization. The regex pattern for an organization ID string requires \"o-\" followed by from 10 to 32 lowercase let... (read-only)")
                .with_provider_name("Id"),
        )
        .attribute(
            AttributeSchema::new("master_account_arn", super::arn())
                .read_only()
                .with_description("The Amazon Resource Name (ARN) of the account that is designated as the management account for the organization. For more information about ARNs in Or... (read-only)")
                .with_provider_name("MasterAccountArn"),
        )
        .attribute(
            AttributeSchema::new("master_account_email", types::email())
                .read_only()
                .with_description("The email address that is associated with the Amazon Web Services account that is designated as the management account for the organization. (read-only)")
                .with_provider_name("MasterAccountEmail"),
        )
        .attribute(
            AttributeSchema::new("master_account_id", super::aws_account_id())
                .read_only()
                .with_description("The unique identifier (ID) of the management account of an organization. The regex pattern for an account ID string requires exactly 12 digits. (read-only)")
                .with_provider_name("MasterAccountId"),
        )
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    (
        "organizations.Organization",
        &[("feature_set", VALID_FEATURE_SET)],
    )
}

/// Maps DSL alias values back to canonical AWS values for this module.
/// e.g., ("ip_protocol", "all") -> Some("-1")
pub fn enum_alias_reverse(attr_name: &str, value: &str) -> Option<&'static str> {
    match (attr_name, value) {
        ("feature_set", "all") => Some("ALL"),
        ("feature_set", "consolidated_billing") => Some("CONSOLIDATED_BILLING"),
        _ => None,
    }
}

/// Returns all enum alias entries as (attr_name, alias, canonical) tuples.
pub fn enum_alias_entries() -> &'static [(&'static str, &'static str, &'static str)] {
    &[
        ("feature_set", "all", "ALL"),
        (
            "feature_set",
            "consolidated_billing",
            "CONSOLIDATED_BILLING",
        ),
    ]
}
