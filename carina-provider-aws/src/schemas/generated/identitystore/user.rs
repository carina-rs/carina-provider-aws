//! user schema definition for AWS Identity Store
//!
//! Hand-written because this data source takes user-supplied lookup inputs
//! (`identity_store_id` + one of `user_name` / `user_id`) that don't fit the
//! codegen's `ResourceDef` model. See `services/identitystore/user.rs` for
//! the read implementation that dispatches between `GetUserId` and
//! `DescribeUser` depending on which input the user provided.

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema, StructField};

/// Returns the schema config for identitystore.user
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
                AttributeSchema::new("user_name", AttributeType::String)
                    .with_description(
                        "A unique string used to identify the user (typically an email). \
                         One of `user_name` or `user_id` must be provided.",
                    )
                    .with_provider_name("UserName"),
            )
            .attribute(
                AttributeSchema::new("user_id", AttributeType::String)
                    .with_description(
                        "The identifier for the user. One of `user_name` or `user_id` must \
                         be provided. Populated on read when `user_name` is used for lookup.",
                    )
                    .with_provider_name("UserId"),
            )
            .attribute(
                AttributeSchema::new("display_name", AttributeType::String)
                    .with_description("The name that is typically displayed when the user is referenced. (read-only)")
                    .with_provider_name("DisplayName"),
            )
            .attribute(
                AttributeSchema::new(
                    "name",
                    AttributeType::Struct {
                        name: "Name".to_string(),
                        fields: vec![
                            StructField::new("formatted", AttributeType::String)
                                .with_description("The full name of the user, formatted for display.")
                                .with_provider_name("Formatted"),
                            StructField::new("family_name", AttributeType::String)
                                .with_description("The family name of the user.")
                                .with_provider_name("FamilyName"),
                            StructField::new("given_name", AttributeType::String)
                                .with_description("The given name of the user.")
                                .with_provider_name("GivenName"),
                            StructField::new("middle_name", AttributeType::String)
                                .with_description("The middle name of the user.")
                                .with_provider_name("MiddleName"),
                            StructField::new("honorific_prefix", AttributeType::String)
                                .with_description("The honorific prefix of the user.")
                                .with_provider_name("HonorificPrefix"),
                            StructField::new("honorific_suffix", AttributeType::String)
                                .with_description("The honorific suffix of the user.")
                                .with_provider_name("HonorificSuffix"),
                        ],
                    },
                )
                .with_description("An object containing the user's name components. (read-only)")
                .with_provider_name("Name"),
            )
            .attribute(
                AttributeSchema::new("emails", AttributeType::list(AttributeType::String))
                    .with_description("The email addresses of the user. (read-only)")
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
pub fn enum_alias_reverse(attr_name: &str, value: &str) -> Option<&'static str> {
    let _ = (attr_name, value);
    None
}

pub fn enum_alias_entries() -> &'static [(&'static str, &'static str, &'static str)] {
    &[]
}
