//! User schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.identitystore
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};

/// Returns the schema config for identitystore.User (Smithy: com.amazonaws.identitystore)
pub fn identitystore_user_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::IdentityStore::User",
        resource_type_name: "identitystore.User",
        has_tags: false,
        schema: ResourceSchema::new("identitystore.User")
            .as_data_source()
            .attribute(
                AttributeSchema::new("identity_store_id", AttributeType::string())
                    .required()
                    .with_description("The globally unique identifier for the identity store.")
                    .with_provider_name("IdentityStoreId"),
            )
            .attribute(
                AttributeSchema::new("user_id", AttributeType::string())
                    .with_description(
                        "The identifier for the user. Provide either user_id or user_name.",
                    )
                    .with_provider_name("UserId"),
            )
            .attribute(
                AttributeSchema::new("user_name", AttributeType::string())
                    .with_description("The user's user name. Provide either user_id or user_name.")
                    .with_provider_name("UserName"),
            )
            .attribute(
                AttributeSchema::new("display_name", AttributeType::string())
                    .with_description("Display name of the user.")
                    .with_provider_name("DisplayName"),
            )
            .attribute(
                AttributeSchema::new("emails", AttributeType::string())
                    .with_description("Email addresses associated with the user.")
                    .with_provider_name("Emails"),
            ),
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("identitystore.User", &[])
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
