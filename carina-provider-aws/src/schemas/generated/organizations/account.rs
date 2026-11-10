//! organizations.Account schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.organizations
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};
use carina_provider_protocol::types::ValidatorType;

const VALID_IAM_USER_ACCESS_TO_BILLING: &[&str] = &["ALLOW", "DENY"];

const VALID_JOINED_METHOD: &[&str] = &["CREATED", "INVITED"];

const VALID_STATUS: &[&str] = &["ACTIVE", "PENDING_CLOSURE", "SUSPENDED"];

pub fn arn() -> AttributeType {
    carina_aws_types::with_member_identity(
        AttributeType::string_member(
            carina_core::schema::StringIdentityLayer::new(None, None, None, None),
            carina_core::schema::StringMemberConstraints::new(
                Some(carina_core::schema::PatternSpec::new(
                    "^arn:(?:aws|aws-cn|aws-us-gov|aws-iso|aws-iso-b|aws-iso-e|aws-iso-f|aws-eusc):organizations::[0-9]{12}:account\\/o-[a-z0-9]{10,32}\\/[0-9]{12}$",
                    carina_core::schema::PatternMatch::Full,
                )),
                None,
            ),
        ),
        carina_aws_types::AwsType::OrganizationsAccountArn.definition(),
    )
}

/// Returns the schema config for organizations.Account (Smithy: com.amazonaws.organizations)
pub fn organizations_account_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::Organizations::Account",
        resource_type_name: "organizations.Account",
        has_tags: true,
        await_on_read_back: vec![],
        schema: ResourceSchema::new("organizations.Account")
        .with_description("Contains information about an Amazon Web Services account that is a member of an organization.")
        .default_prevent_destroy()
        .with_conflicting_replacement()
        .attribute(
            AttributeSchema::required("account_name", AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(Some(carina_core::schema::PatternSpec::new("^[\\u0020-\\u007E]+$", carina_core::schema::PatternMatch::Full)), Some((Some(1), Some(50))))))
                .with_description("The friendly name of the member account.")
                .with_provider_name("AccountName")
                .build(),
        )
        .attribute(
            AttributeSchema::required("email", carina_aws_types::with_member_identity(AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(Some(carina_core::schema::PatternSpec::new("^[^\\x09-\\x0D \\u{00A0}\\u{1680}\\u{2000}-\\u{200A}\\u{2028}\\u{2029}\\u{202F}\\u{205F}\\u{3000}\\u{FEFF}@]+@[^\\x09-\\x0D \\u{00A0}\\u{1680}\\u{2000}-\\u{200A}\\u{2028}\\u{2029}\\u{202F}\\u{205F}\\u{3000}\\u{FEFF}@]+\\.[^\\x09-\\x0D \\u{00A0}\\u{1680}\\u{2000}-\\u{200A}\\u{2028}\\u{2029}\\u{202F}\\u{205F}\\u{3000}\\u{FEFF}@]+$", carina_core::schema::PatternMatch::Full)), Some((Some(6), Some(64))))), carina_aws_types::AwsType::OrganizationsAccountEmail.definition()))
                .create_only()
                .with_description("The email address of the owner to assign to the new member account. This email address must not already be associated with another Amazon Web Services...")
                .with_provider_name("Email")
                .build(),
        )
        // Read contract: DescribeAccount does not return the create-time billing-access setting
        .attribute(
            AttributeSchema::optional("iam_user_access_to_billing", AttributeType::enum_(carina_core::schema::enum_identity("IamUserAccessToBilling", Some("aws.organizations.Account")), Some(vec!["ALLOW".to_string(), "DENY".to_string()]), vec![("ALLOW".to_string(), "allow".to_string()), ("DENY".to_string(), "deny".to_string())], None, None))
                .create_only()
                .write_only()
                .with_description("If set to ALLOW, the new account enables IAM users to access account billing information if they have the required permissions. If set to DENY, only t...")
                .with_provider_name("IamUserAccessToBilling")
                .build(),
        )
        // Read contract: DescribeAccount does not return the create-time organization role name
        .attribute(
            AttributeSchema::optional("role_name", carina_aws_types::with_member_identity(AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(Some(carina_core::schema::PatternSpec::new("^[A-Za-z0-9_+=,.@\\-]{1,64}$", carina_core::schema::PatternMatch::Full)), Some((Some(0), Some(64))))), carina_aws_types::AwsType::IamRoleName.definition()))
                .create_only()
                .write_only()
                .with_description("The name of an IAM role that Organizations automatically preconfigures in the new member account. This role trusts the management account, allowing us...")
                .with_provider_name("RoleName")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("arn", carina_aws_types::with_member_identity(AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(Some(carina_core::schema::PatternSpec::new("^arn:(?:aws|aws-cn|aws-us-gov|aws-iso|aws-iso-b|aws-iso-e|aws-iso-f|aws-eusc):organizations::[0-9]{12}:account\\/o-[a-z0-9]{10,32}\\/[0-9]{12}$", carina_core::schema::PatternMatch::Full)), None)), carina_aws_types::AwsType::OrganizationsAccountArn.definition()))
                .with_description("The Amazon Resource Name (ARN) of the account. For more information about ARNs in Organizations, see ARN Formats Supported by Organizations in the Ama... (read-only)")
                .with_provider_name("Arn")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("id", carina_aws_types::with_member_identity(AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(Some(carina_core::schema::PatternSpec::new("^[0-9]{12}$", carina_core::schema::PatternMatch::Full)), Some((Some(0), Some(12))))), carina_aws_types::AwsType::AccountId.definition()))
                .with_description("The unique identifier (ID) of the account. The regex pattern for an account ID string requires exactly 12 digits. (read-only)")
                .with_provider_name("Id")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("joined_method", AttributeType::enum_(carina_core::schema::enum_identity("JoinedMethod", Some("aws.organizations.Account")), Some(vec!["CREATED".to_string(), "INVITED".to_string()]), vec![("CREATED".to_string(), "created".to_string()), ("INVITED".to_string(), "invited".to_string())], None, None))
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
            AttributeSchema::provider_populated("status", AttributeType::enum_(carina_core::schema::enum_identity("Status", Some("aws.organizations.Account")), Some(vec!["ACTIVE".to_string(), "PENDING_CLOSURE".to_string(), "SUSPENDED".to_string()]), vec![("ACTIVE".to_string(), "active".to_string()), ("PENDING_CLOSURE".to_string(), "pending_closure".to_string()), ("SUSPENDED".to_string(), "suspended".to_string())], None, None))
                .with_description("The status of the account in the organization. The Status parameter in the Account object will be retired on September 9, 2026. Although both the acco... (read-only)")
                .with_provider_name("Status")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("tags", AttributeType::map_with_key(AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(Some(carina_core::schema::PatternSpec::new("^([\\p{L}\\p{Z}\\p{N}_.:/=+\\-@]*)$", carina_core::schema::PatternMatch::Full)), Some((Some(1), Some(128))))), AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(Some(carina_core::schema::PatternSpec::new("^([\\p{L}\\p{Z}\\p{N}_.:/=+\\-@]*)$", carina_core::schema::PatternMatch::Full)), Some((Some(0), Some(256)))))))
                .with_description("The tags for the resource.")
                .with_provider_name("Tags")
                .build(),
        )
        .input_rule(carina_core::schema::InputRule::new(carina_core::schema::InputRuleScope::EveryWrite, carina_core::schema::InputRuleTarget::Value(carina_core::schema::InputPath::new("account_name").expect("valid input path")), carina_core::schema::InputRuleCode::new("aws.additional_pattern.pattern_0").expect("valid input rule code"), carina_core::schema::InputPredicate::Matches(carina_core::schema::PatternSpec::new("^[ -;=?-~]+\\z", carina_core::schema::PatternMatch::Full))))
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
