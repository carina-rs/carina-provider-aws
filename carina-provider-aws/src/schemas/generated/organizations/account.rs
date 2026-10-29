//! organizations.Account schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.organizations
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use super::tags_type;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema, types};
use carina_provider_protocol::types::ValidatorType;

const VALID_IAM_USER_ACCESS_TO_BILLING: &[&str] = &["ALLOW", "DENY"];

const VALID_JOINED_METHOD: &[&str] = &["CREATED", "INVITED"];

const VALID_STATUS: &[&str] = &["ACTIVE", "PENDING_CLOSURE", "SUSPENDED"];

pub fn arn() -> AttributeType {
    super::organizations_account_arn()
}

/// Returns the schema config for organizations.Account (Smithy: com.amazonaws.organizations)
pub fn organizations_account_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::Organizations::Account",
        resource_type_name: "organizations.Account",
        has_tags: true,
        schema: ResourceSchema::new("organizations.Account")
        .with_description("Contains information about an Amazon Web Services account that is a member of an organization.")
        .with_conflicting_replacement()
        .attribute(
            AttributeSchema::required("account_name", AttributeType::string())
                .create_only()
                .with_description("The friendly name of the member account.")
                .with_provider_name("AccountName")
                .build(),
        )
        .attribute(
            AttributeSchema::required("email", types::email())
                .create_only()
                .with_description("The email address of the owner to assign to the new member account. This email address must not already be associated with another Amazon Web Services...")
                .with_provider_name("Email")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("iam_user_access_to_billing", AttributeType::enum_(
    carina_core::schema::enum_identity("IamUserAccessToBilling", Some("aws.organizations.Account")),
    Some(vec!["ALLOW".to_string(), "DENY".to_string()]),
    vec![("ALLOW".to_string(), "allow".to_string()), ("DENY".to_string(), "deny".to_string())],
    None,
    None,
))
                .create_only()
                .with_description("If set to ALLOW, the new account enables IAM users to access account billing information if they have the required permissions. If set to DENY, only t...")
                .with_provider_name("IamUserAccessToBilling")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("role_name", AttributeType::string())
                .create_only()
                .with_description("The name of an IAM role that Organizations automatically preconfigures in the new member account. This role trusts the management account, allowing us...")
                .with_provider_name("RoleName")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("arn", self::arn())
                .with_description("The Amazon Resource Name (ARN) of the account. For more information about ARNs in Organizations, see ARN Formats Supported by Organizations in the Ama... (read-only)")
                .with_provider_name("Arn")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("id", AttributeType::string())
                .with_description("The unique identifier (ID) of the account. The regex pattern for an account ID string requires exactly 12 digits. (read-only)")
                .with_provider_name("Id")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("joined_method", AttributeType::enum_(
    carina_core::schema::enum_identity("JoinedMethod", Some("aws.organizations.Account")),
    Some(vec!["CREATED".to_string(), "INVITED".to_string()]),
    vec![("CREATED".to_string(), "created".to_string()), ("INVITED".to_string(), "invited".to_string())],
    None,
    None,
))
                .with_description("The method by which the account joined the organization. (read-only)")
                .with_provider_name("JoinedMethod")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("joined_timestamp", AttributeType::string())
                .with_description("The date the account became a part of the organization. (read-only)")
                .with_provider_name("JoinedTimestamp")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("name", AttributeType::string())
                .with_description("The friendly name of the account. The regex pattern that is used to validate this parameter is a string of any of the characters in the ASCII characte... (read-only)")
                .with_provider_name("Name")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("status", AttributeType::enum_(
    carina_core::schema::enum_identity("Status", Some("aws.organizations.Account")),
    Some(vec!["ACTIVE".to_string(), "PENDING_CLOSURE".to_string(), "SUSPENDED".to_string()]),
    vec![("ACTIVE".to_string(), "active".to_string()), ("PENDING_CLOSURE".to_string(), "pending_closure".to_string()), ("SUSPENDED".to_string(), "suspended".to_string())],
    None,
    None,
))
                .with_description("The status of the account in the organization. The Status parameter in the Account object will be retired on September 9, 2026. Although both the acco... (read-only)")
                .with_provider_name("Status")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("tags", tags_type())
                .with_description("The tags for the resource.")
                .with_provider_name("Tags")
                .build(),
        )
        .with_declared_validators(vec![ValidatorType::TagsKeyValueCheck])
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    (
        "organizations.Account",
        &[
            (
                "iam_user_access_to_billing",
                VALID_IAM_USER_ACCESS_TO_BILLING,
            ),
            ("joined_method", VALID_JOINED_METHOD),
            ("status", VALID_STATUS),
        ],
    )
}
