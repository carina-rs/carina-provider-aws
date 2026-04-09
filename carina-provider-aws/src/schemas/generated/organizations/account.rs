//! account schema definition for AWS
//!
//! Auto-generated from Smithy model: com.amazonaws.organizations
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};

/// Returns the schema config for organizations.account (Smithy: com.amazonaws.organizations)
pub fn organizations_account_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::Organizations::Account",
        resource_type_name: "organizations.account",
        has_tags: true,
        schema: ResourceSchema::new("aws.organizations.account")
            .attribute(
                AttributeSchema::new("name", AttributeType::String)
                    .required()
                    .create_only()
                    .with_description("The friendly name of the member account. (required, create-only)")
                    .with_provider_name("AccountName"),
            )
            .attribute(
                AttributeSchema::new("email", AttributeType::String)
                    .required()
                    .create_only()
                    .with_description("The email address of the owner to assign to the new member account. (required, create-only)")
                    .with_provider_name("Email"),
            )
            .attribute(
                AttributeSchema::new(
                    "iam_user_access_to_billing",
                    AttributeType::StringEnum {
                        name: "IAMUserAccessToBilling".to_string(),
                        values: vec!["ALLOW".to_string(), "DENY".to_string()],
                        namespace: Some("aws.organizations.account".to_string()),
                        to_dsl: None,
                    },
                )
                .create_only()
                .with_description("Whether IAM users can access billing. (create-only)")
                .with_provider_name("IamUserAccessToBilling"),
            )
            .attribute(
                AttributeSchema::new("role_name", AttributeType::String)
                    .create_only()
                    .with_description("The name of an IAM role for cross-account access. (create-only)")
                    .with_provider_name("RoleName"),
            )
            .attribute(
                AttributeSchema::new("parent_id", AttributeType::String)
                    .with_description("The unique identifier of the parent OU or root.")
                    .with_provider_name("ParentId"),
            )
            .attribute(
                AttributeSchema::new("id", AttributeType::String)
                    .read_only()
                    .with_description("The unique identifier (ID) of the account. (read-only)")
                    .with_provider_name("AccountId"),
            )
            .attribute(
                AttributeSchema::new("arn", super::arn())
                    .read_only()
                    .with_description("The Amazon Resource Name (ARN) of the account. (read-only)")
                    .with_provider_name("Arn"),
            )
            .attribute(
                AttributeSchema::new(
                    "status",
                    AttributeType::StringEnum {
                        name: "AccountStatus".to_string(),
                        values: vec![
                            "ACTIVE".to_string(),
                            "SUSPENDED".to_string(),
                            "PENDING_CLOSURE".to_string(),
                        ],
                        namespace: Some("aws.organizations.account".to_string()),
                        to_dsl: None,
                    },
                )
                .read_only()
                .with_description("The status of the account. (read-only)")
                .with_provider_name("Status"),
            )
            .attribute(
                AttributeSchema::new(
                    "joined_method",
                    AttributeType::StringEnum {
                        name: "AccountJoinedMethod".to_string(),
                        values: vec!["CREATED".to_string(), "INVITED".to_string()],
                        namespace: Some("aws.organizations.account".to_string()),
                        to_dsl: None,
                    },
                )
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
                AttributeSchema::new("tags", super::tags_type())
                    .with_description("Tags for the account.")
                    .with_provider_name("Tags"),
            ),
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    (
        "organizations.account",
        &[
            ("iam_user_access_to_billing", &["ALLOW", "DENY"]),
            ("status", &["ACTIVE", "SUSPENDED", "PENDING_CLOSURE"]),
            ("joined_method", &["CREATED", "INVITED"]),
        ],
    )
}

/// Maps DSL alias values back to canonical AWS values for this module.
pub fn enum_alias_reverse(attr_name: &str, value: &str) -> Option<&'static str> {
    let _ = (attr_name, value);
    None
}
