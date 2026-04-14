//! user schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.identitystore
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};

/// Returns the schema config for identitystore.user (Smithy: com.amazonaws.identitystore)
pub fn identitystore_user_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::IdentityStore::User",
        resource_type_name: "identitystore.user",
        has_tags: false,
        schema: ResourceSchema::new("aws.identitystore.user")
            .as_data_source()
            .attribute(
                AttributeSchema::new("identity_store_id", AttributeType::String)
                    .required()
                    .with_description("The globally unique identifier for the identity store.")
                    .with_provider_name("IdentityStoreId"),
            )
            .attribute(
                AttributeSchema::new("user_id", AttributeType::String)
                    .with_description(
                        "The identifier for the user. Provide either user_id or user_name.",
                    )
                    .with_provider_name("UserId"),
            )
            .attribute(
                AttributeSchema::new("user_name", AttributeType::String)
                    .with_description("The user's user name. Provide either user_id or user_name.")
                    .with_provider_name("UserName"),
            )
            .attribute(
                AttributeSchema::new("display_name", AttributeType::String)
                    .with_description("<p>The display name of the user.</p> (read-only)")
                    .with_provider_name("DisplayName"),
            )
            .attribute(
                AttributeSchema::new("emails", AttributeType::String)
                    .with_description("<p>The email address of the user.</p> (read-only)")
                    .with_provider_name("Emails"),
            ),
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("identitystore.user", &[])
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
