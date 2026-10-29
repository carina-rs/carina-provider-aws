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
                AttributeSchema::required("identity_store_id", AttributeType::string())
                    .with_description("The globally unique identifier for the identity store.")
                    .with_provider_name("IdentityStoreId")
                    .build(),
            )
            .attribute(
                AttributeSchema::optional("user_id", AttributeType::string())
                    .with_description(
                        "The identifier for the user. Provide either user_id or user_name.",
                    )
                    .with_provider_name("UserId")
                    .build(),
            )
            .attribute(
                AttributeSchema::optional("user_name", AttributeType::string())
                    .with_description("The user's user name. Provide either user_id or user_name.")
                    .with_provider_name("UserName")
                    .build(),
            )
            .attribute(
                AttributeSchema::optional("display_name", AttributeType::string())
                    .with_description("Display name of the user.")
                    .with_provider_name("DisplayName")
                    .build(),
            )
            .attribute(
                AttributeSchema::optional("emails", AttributeType::string())
                    .with_description("Email addresses associated with the user.")
                    .with_provider_name("Emails")
                    .build(),
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
