//! User schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.identitystore
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema, StructField, types};

/// Returns the schema config for identitystore.User (Smithy: com.amazonaws.identitystore)
pub fn identitystore_user_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::IdentityStore::User",
        resource_type_name: "identitystore.User",
        has_tags: false,
        await_on_read_back: vec![],
        schema: ResourceSchema::new("identitystore.User")
        .as_data_source()
        .attribute(
            AttributeSchema::required("identity_store_id", AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(Some(carina_core::schema::PatternSpec::new("^(arn:aws[a-z\\-]*:identitystore::[0-9]{12}:identitystore/)?(d-[0-9a-f]{10}|[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})$", carina_core::schema::PatternMatch::Full)), Some((Some(1), Some(93))))))
                .with_description("The globally unique identifier for the identity store.")
                .with_provider_name("IdentityStoreId")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("user_id", AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(Some(carina_core::schema::PatternSpec::new("^(arn:aws[a-z\\-]*:identitystore:::(user|group|membership)/)?([0-9a-f]{10}-|)[A-Fa-f0-9]{8}-[A-Fa-f0-9]{4}-[A-Fa-f0-9]{4}-[A-Fa-f0-9]{4}-[A-Fa-f0-9]{12}$", carina_core::schema::PatternMatch::Full)), Some((Some(1), Some(100))))))
                .with_description("The identifier for the user. Provide either user_id or user_name.")
                .with_provider_name("UserId")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("user_name", AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(Some(carina_core::schema::PatternSpec::new("^[\\p{L}\\p{M}\\p{S}\\p{N}\\p{P}]+$", carina_core::schema::PatternMatch::Full)), Some((Some(1), Some(128))))))
                .with_description("The user's user name. Provide either user_id or user_name.")
                .with_provider_name("UserName")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("display_name", AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(Some(carina_core::schema::PatternSpec::new("^[\\p{L}\\p{M}\\p{S}\\p{N}\\p{P}\\t\\n\\r \u{a0}\u{3000}]+$", carina_core::schema::PatternMatch::Full)), Some((Some(1), Some(1024))))))
                .with_description("Display name of the user.")
                .with_provider_name("DisplayName")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("name", AttributeType::struct_("IdentityStoreUserName", vec![StructField::optional("family_name", AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(Some(carina_core::schema::PatternSpec::new("^[\\p{L}\\p{M}\\p{S}\\p{N}\\p{P}\\t\\n\\r \u{a0}\u{3000}]+$", carina_core::schema::PatternMatch::Full)), Some((Some(1), Some(1024)))))).with_provider_name("FamilyName").build(), StructField::optional("formatted", AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(Some(carina_core::schema::PatternSpec::new("^[\\p{L}\\p{M}\\p{S}\\p{N}\\p{P}\\t\\n\\r \u{a0}\u{3000}]+$", carina_core::schema::PatternMatch::Full)), Some((Some(1), Some(1024)))))).with_provider_name("Formatted").build(), StructField::optional("given_name", AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(Some(carina_core::schema::PatternSpec::new("^[\\p{L}\\p{M}\\p{S}\\p{N}\\p{P}\\t\\n\\r \u{a0}\u{3000}]+$", carina_core::schema::PatternMatch::Full)), Some((Some(1), Some(1024)))))).with_provider_name("GivenName").build(), StructField::optional("honorific_prefix", AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(Some(carina_core::schema::PatternSpec::new("^[\\p{L}\\p{M}\\p{S}\\p{N}\\p{P}\\t\\n\\r \u{a0}\u{3000}]+$", carina_core::schema::PatternMatch::Full)), Some((Some(1), Some(1024)))))).with_provider_name("HonorificPrefix").build(), StructField::optional("honorific_suffix", AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(Some(carina_core::schema::PatternSpec::new("^[\\p{L}\\p{M}\\p{S}\\p{N}\\p{P}\\t\\n\\r \u{a0}\u{3000}]+$", carina_core::schema::PatternMatch::Full)), Some((Some(1), Some(1024)))))).with_provider_name("HonorificSuffix").build(), StructField::optional("middle_name", AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(Some(carina_core::schema::PatternSpec::new("^[\\p{L}\\p{M}\\p{S}\\p{N}\\p{P}\\t\\n\\r \u{a0}\u{3000}]+$", carina_core::schema::PatternMatch::Full)), Some((Some(1), Some(1024)))))).with_provider_name("MiddleName").build()]))
                .with_description("Structured components of the user's name.")
                .with_provider_name("Name")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("emails", AttributeType::refined_list(carina_aws_types::with_member_identity(AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(Some(carina_core::schema::PatternSpec::new("^[\\p{L}\\p{M}\\p{S}\\p{N}\\p{P}\\t\\n\\r \u{a0}\u{3000}]+$", carina_core::schema::PatternMatch::Full)), Some((Some(1), Some(1024))))), types::email()), false, Some((Some(1), Some(1))), carina_core::schema::ListElements::AllowDuplicates))
                .with_description("Email addresses associated with the user.")
                .with_provider_name("Emails")
                .build(),
        )
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("identitystore.User", &[])
}
