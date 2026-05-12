//! organizations.Account schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.organizations
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use super::tags_type;
use super::validate_tags_map;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema, types};

const VALID_IAM_USER_ACCESS_TO_BILLING: &[&str] = &["ALLOW", "DENY", "allow", "deny"];

const VALID_JOINED_METHOD: &[&str] = &["CREATED", "INVITED", "created", "invited"];

const VALID_STATUS: &[&str] = &[
    "ACTIVE",
    "PENDING_CLOSURE",
    "SUSPENDED",
    "active",
    "pending_closure",
    "suspended",
];

/// Returns the schema config for organizations.Account (Smithy: com.amazonaws.organizations)
pub fn organizations_account_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::Organizations::Account",
        resource_type_name: "organizations.Account",
        has_tags: true,
        schema: ResourceSchema::new("organizations.Account")
        .with_description("Contains information about an Amazon Web Services account that is a member of an organization.")
        .attribute(
            AttributeSchema::new("account_name", AttributeType::String)
                .required()
                .create_only()
                .with_description("The friendly name of the member account.")
                .with_provider_name("AccountName"),
        )
        .attribute(
            AttributeSchema::new("email", types::email())
                .required()
                .create_only()
                .with_description("The email address of the owner to assign to the new member account. This email address must not already be associated with another Amazon Web Services...")
                .with_provider_name("Email"),
        )
        .attribute(
            AttributeSchema::new("iam_user_access_to_billing", AttributeType::StringEnum {
                name: "IamUserAccessToBilling".to_string(),
                values: vec!["ALLOW".to_string(), "DENY".to_string()],
                namespace: Some("aws.organizations.Account".to_string()),
                dsl_aliases: vec![("ALLOW".to_string(), "allow".to_string()), ("DENY".to_string(), "deny".to_string())],
            })
                .create_only()
                .with_description("If set to ALLOW, the new account enables IAM users to access account billing information if they have the required permissions. If set to DENY, only t...")
                .with_provider_name("IamUserAccessToBilling"),
        )
        .attribute(
            AttributeSchema::new("role_name", AttributeType::String)
                .create_only()
                .with_description("The name of an IAM role that Organizations automatically preconfigures in the new member account. This role trusts the management account, allowing us...")
                .with_provider_name("RoleName"),
        )
        .attribute(
            AttributeSchema::new("arn", super::arn())
                .read_only()
                .with_description("The Amazon Resource Name (ARN) of the account. For more information about ARNs in Organizations, see ARN Formats Supported by Organizations in the Ama... (read-only)")
                .with_provider_name("Arn"),
        )
        .attribute(
            AttributeSchema::new("id", AttributeType::String)
                .read_only()
                .with_description("The unique identifier (ID) of the account. The regex pattern for an account ID string requires exactly 12 digits. (read-only)")
                .with_provider_name("Id"),
        )
        .attribute(
            AttributeSchema::new("joined_method", AttributeType::StringEnum {
                name: "JoinedMethod".to_string(),
                values: vec!["CREATED".to_string(), "INVITED".to_string()],
                namespace: Some("aws.organizations.Account".to_string()),
                dsl_aliases: vec![("CREATED".to_string(), "created".to_string()), ("INVITED".to_string(), "invited".to_string())],
            })
                .read_only()
                .with_description("The method by which the account joined the organization. (read-only)")
                .with_provider_name("JoinedMethod"),
        )
        .attribute(
            AttributeSchema::new("joined_timestamp", AttributeType::String)
                .read_only()
                .with_description("The date the account became a part of the organization. (read-only)")
                .with_provider_name("JoinedTimestamp"),
        )
        .attribute(
            AttributeSchema::new("name", AttributeType::String)
                .read_only()
                .with_description("The friendly name of the account. The regex pattern that is used to validate this parameter is a string of any of the characters in the ASCII characte... (read-only)")
                .with_provider_name("Name"),
        )
        .attribute(
            AttributeSchema::new("status", AttributeType::StringEnum {
                name: "Status".to_string(),
                values: vec!["ACTIVE".to_string(), "PENDING_CLOSURE".to_string(), "SUSPENDED".to_string()],
                namespace: Some("aws.organizations.Account".to_string()),
                dsl_aliases: vec![("ACTIVE".to_string(), "active".to_string()), ("PENDING_CLOSURE".to_string(), "pending_closure".to_string()), ("SUSPENDED".to_string(), "suspended".to_string())],
            })
                .read_only()
                .with_description("The status of the account in the organization. The Status parameter in the Account object will be retired on September 9, 2026. Although both the acco... (read-only)")
                .with_provider_name("Status"),
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

/// Maps DSL alias values back to canonical AWS values for this module.
/// e.g., ("ip_protocol", "all") -> Some("-1")
pub fn enum_alias_reverse(attr_name: &str, value: &str) -> Option<&'static str> {
    match (attr_name, value) {
        ("iam_user_access_to_billing", "allow") => Some("ALLOW"),
        ("iam_user_access_to_billing", "deny") => Some("DENY"),
        ("joined_method", "created") => Some("CREATED"),
        ("joined_method", "invited") => Some("INVITED"),
        ("status", "active") => Some("ACTIVE"),
        ("status", "pending_closure") => Some("PENDING_CLOSURE"),
        ("status", "suspended") => Some("SUSPENDED"),
        _ => None,
    }
}

/// Returns all enum alias entries as (attr_name, alias, canonical) tuples.
pub fn enum_alias_entries() -> &'static [(&'static str, &'static str, &'static str)] {
    &[
        ("iam_user_access_to_billing", "allow", "ALLOW"),
        ("iam_user_access_to_billing", "deny", "DENY"),
        ("joined_method", "created", "CREATED"),
        ("joined_method", "invited", "INVITED"),
        ("status", "active", "ACTIVE"),
        ("status", "pending_closure", "PENDING_CLOSURE"),
        ("status", "suspended", "SUSPENDED"),
    ]
}
