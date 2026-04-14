//! iam.role schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.iam
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use super::tags_type;
use super::validate_tags_map;
use carina_core::resource::Value;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};

fn validate_max_session_duration_range(value: &Value) -> Result<(), String> {
    if let Value::Int(n) = value {
        if *n < 3600 || *n > 43200 {
            Err(format!("Value {} is out of range 3600..=43200", n))
        } else {
            Ok(())
        }
    } else {
        Err("Expected integer".to_string())
    }
}

/// Returns the schema config for iam.role (Smithy: com.amazonaws.iam)
pub fn iam_role_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::IAM::Role",
        resource_type_name: "iam.role",
        has_tags: true,
        schema: ResourceSchema::new("aws.iam.role")
        .with_description("Contains information about an IAM role. This structure is returned as a response element in several API operations that interact with roles.")
        .attribute(
            AttributeSchema::new("assume_role_policy_document", super::iam_policy_document())
                .required()
                .create_only()
                .with_description("The trust relationship policy document that grants an entity permission to assume the role. In IAM, you must provide a JSON policy that has been conve...")
                .with_provider_name("AssumeRolePolicyDocument"),
        )
        .attribute(
            AttributeSchema::new("description", AttributeType::String)
                .create_only()
                .with_description("A description of the role.")
                .with_provider_name("Description"),
        )
        .attribute(
            AttributeSchema::new("max_session_duration", AttributeType::Custom {
                name: "Int(3600..=43200)".to_string(),
                base: Box::new(AttributeType::Int),
                validate: validate_max_session_duration_range,
                namespace: None,
                to_dsl: None,
            })
                .create_only()
                .with_description("The maximum session duration (in seconds) that you want to set for the specified role. If you do not specify a value for this setting, the default val...")
                .with_provider_name("MaxSessionDuration"),
        )
        .attribute(
            AttributeSchema::new("path", AttributeType::String)
                .create_only()
                .with_description("The path to the role. For more information about paths, see IAM Identifiers in the IAM User Guide. This parameter is optional. If it is not included, ...")
                .with_provider_name("Path"),
        )
        .attribute(
            AttributeSchema::new("role_name", AttributeType::String)
                .required()
                .create_only()
                .with_description("The name of the role to create. IAM user, group, role, and policy names must be unique within the account. Names are not distinguished by case. For ex...")
                .with_provider_name("RoleName"),
        )
        .attribute(
            AttributeSchema::new("arn", super::iam_role_arn())
                .with_description("The Amazon Resource Name (ARN) specifying the role. For more information about ARNs and how to use them in policies, see IAM identifiers in the IAM Us... (read-only)")
                .with_provider_name("Arn"),
        )
        .attribute(
            AttributeSchema::new("role_id", super::iam_role_id())
                .with_description("The stable and unique string identifying the role. For more information about IDs, see IAM identifiers in the IAM User Guide. (read-only)")
                .with_provider_name("RoleId"),
        )
        .attribute(
            AttributeSchema::new("tags", tags_type())
                .with_description("The tags for the resource.")
                .with_provider_name("Tags"),
        )
        .with_validator(validate_tags_map)
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("iam.role", &[])
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
