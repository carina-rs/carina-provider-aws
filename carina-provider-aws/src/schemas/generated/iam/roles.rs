//! Roles schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.iam
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};

/// Returns the schema config for iam.Roles (Smithy: com.amazonaws.iam)
pub fn iam_roles_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::IAM::Roles",
        resource_type_name: "iam.Roles",
        has_tags: false,
        schema: ResourceSchema::new("iam.Roles")
        .as_data_source()
        .attribute(
            AttributeSchema::new("path_prefix", AttributeType::string())
                .with_description("Path prefix for filtering the results. If it is not included, it defaults to `/`, listing all roles.")
                .with_provider_name("PathPrefix"),
        )
        .attribute(
            AttributeSchema::new("name_regex", AttributeType::string())
                .with_description("Regular expression string applied client-side to filter results by role name.")
                .with_provider_name(""),
        )
        .attribute(
            AttributeSchema::new("arns", AttributeType::unordered_list(super::super::iam::role::arn()))
                .with_description("Set of ARNs of the matched IAM roles.")
                .with_provider_name(""),
        )
        .attribute(
            AttributeSchema::new("names", AttributeType::unordered_list(AttributeType::string()))
                .with_description("Set of names of the matched IAM roles.")
                .with_provider_name(""),
        )
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("iam.Roles", &[])
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
