//! organizations.Organization schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.organizations
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema, types};

const VALID_FEATURE_SET: &[&str] = &["ALL", "CONSOLIDATED_BILLING", "all", "consolidated_billing"];

/// Returns the schema config for organizations.Organization (Smithy: com.amazonaws.organizations)
pub fn organizations_organization_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::Organizations::Organization",
        resource_type_name: "organizations.Organization",
        has_tags: false,
        schema: ResourceSchema::new("organizations.Organization")
        .with_description("Contains details about an organization. An organization is a collection of accounts that are centrally managed together using consolidated billing, organized hierarchically with organizational units (...")
        .attribute(
            AttributeSchema::new("feature_set", AttributeType::StringEnum {
                name: "FeatureSet".to_string(),
                values: vec!["ALL".to_string(), "CONSOLIDATED_BILLING".to_string()],
                namespace: Some("aws.organizations.Organization".to_string()),
                to_dsl: Some(|s: &str| match s { "ALL" => "all".to_string(), "CONSOLIDATED_BILLING" => "consolidated_billing".to_string(), _ => s.to_string() }),
            })
                .create_only()
                .with_description("Specifies the feature set supported by the new organization. Each feature set supports different levels of functionality. CONSOLIDATED_BILLING: All me...")
                .with_provider_name("FeatureSet"),
        )
        .attribute(
            AttributeSchema::new("arn", super::arn())
                .with_description("The Amazon Resource Name (ARN) of an organization. For more information about ARNs in Organizations, see ARN Formats Supported by Organizations in the... (read-only)")
                .with_provider_name("Arn"),
        )
        .attribute(
            AttributeSchema::new("id", AttributeType::String)
                .with_description("The unique identifier (ID) of an organization. The regex pattern for an organization ID string requires \"o-\" followed by from 10 to 32 lowercase let... (read-only)")
                .with_provider_name("Id"),
        )
        .attribute(
            AttributeSchema::new("master_account_arn", super::arn())
                .with_description("The Amazon Resource Name (ARN) of the account that is designated as the management account for the organization. For more information about ARNs in Or... (read-only)")
                .with_provider_name("MasterAccountArn"),
        )
        .attribute(
            AttributeSchema::new("master_account_email", types::email())
                .with_description("The email address that is associated with the Amazon Web Services account that is designated as the management account for the organization. (read-only)")
                .with_provider_name("MasterAccountEmail"),
        )
        .attribute(
            AttributeSchema::new("master_account_id", super::aws_account_id())
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
