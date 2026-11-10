//! iam.Role schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.iam
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};
use carina_provider_protocol::types::ValidatorType;

pub fn arn() -> AttributeType {
    carina_aws_types::with_member_identity(
        AttributeType::string_member(
            carina_core::schema::StringIdentityLayer::new(None, None, None, None),
            carina_core::schema::StringMemberConstraints::new(None, Some((Some(20), Some(2048)))),
        ),
        carina_aws_types::AwsType::IamRoleArn.definition(),
    )
}

/// Returns the schema config for iam.Role (Smithy: com.amazonaws.iam)
pub fn iam_role_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::IAM::Role",
        resource_type_name: "iam.Role",
        has_tags: true,
        await_on_read_back: vec![],
        schema: ResourceSchema::new("iam.Role")
        .with_description("Contains information about an IAM role. This structure is returned as a response element in several API operations that interact with roles.")
        .with_unique_name_attribute("role_name")
        .attribute(
            AttributeSchema::required("assume_role_policy_document", super::iam_policy_document())
                .create_only()
                .with_description("The trust relationship policy document that grants an entity permission to assume the role. In IAM, you must provide a JSON policy that has been conve...")
                .with_provider_name("AssumeRolePolicyDocument")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("description", AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(Some(carina_core::schema::PatternSpec::new("^[\\u0009\\u000A\\u000D\\u0020-\\u007E\\u00A1-\\u00FF]*$", carina_core::schema::PatternMatch::Full)), Some((Some(0), Some(1000))))))
                .create_only()
                .with_description("A description of the role.")
                .with_provider_name("Description")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("max_session_duration", AttributeType::int_member(carina_core::schema::IntIdentityLayer::new(None, None), carina_core::schema::IntMemberConstraints::new(Some((Some(3600), Some(43200))))))
                .create_only()
                .with_description("The maximum session duration (in seconds) that you want to set for the specified role. If you do not specify a value for this setting, the default val...")
                .with_provider_name("MaxSessionDuration")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("path", AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(Some(carina_core::schema::PatternSpec::new("^(\\u002F)|(\\u002F[\\u0021-\\u007E]+\\u002F)$", carina_core::schema::PatternMatch::Substring { reason: carina_core::schema::PatternReason::new("Smithy pattern is unanchored").expect("visible pattern reason") })), Some((Some(1), Some(512))))))
                .create_only()
                .with_description("The path to the role. For more information about paths, see IAM Identifiers in the IAM User Guide. This parameter is optional. If it is not included, ...")
                .with_provider_name("Path")
                .build(),
        )
        .attribute(
            AttributeSchema::required("role_name", carina_aws_types::with_member_identity(AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(Some(carina_core::schema::PatternSpec::new("^[A-Za-z0-9_+=,.@\\-]+$", carina_core::schema::PatternMatch::Full)), Some((Some(1), Some(64))))), carina_aws_types::AwsType::IamRoleName.definition()))
                .create_only()
                .identity()
                .with_description("The name of the role to create. IAM user, group, role, and policy names must be unique within the account. Names are not distinguished by case. For ex...")
                .with_provider_name("RoleName")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("arn", carina_aws_types::with_member_identity(AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(None, Some((Some(20), Some(2048))))), carina_aws_types::AwsType::IamRoleArn.definition()))
                .with_description("The Amazon Resource Name (ARN) specifying the role. For more information about ARNs and how to use them in policies, see IAM identifiers in the IAM Us... (read-only)")
                .with_provider_name("Arn")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("role_id", carina_aws_types::with_member_identity(AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(Some(carina_core::schema::PatternSpec::new("^[A-Za-z0-9_]+$", carina_core::schema::PatternMatch::Full)), Some((Some(16), Some(128))))), carina_aws_types::AwsType::IamRoleId.definition()))
                .with_description("The stable and unique string identifying the role. For more information about IDs, see IAM identifiers in the IAM User Guide. (read-only)")
                .with_provider_name("RoleId")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("tags", AttributeType::refined_map(AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(Some(carina_core::schema::PatternSpec::new("^[\\p{L}\\p{Z}\\p{N}_.:/=+\\-@]+$", carina_core::schema::PatternMatch::Full)), Some((Some(1), Some(128))))), AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(Some(carina_core::schema::PatternSpec::new("^[\\p{L}\\p{Z}\\p{N}_.:/=+\\-@]*$", carina_core::schema::PatternMatch::Full)), Some((Some(0), Some(256))))), Some((Some(0), Some(50)))))
                .with_description("The tags for the resource.")
                .with_provider_name("Tags")
                .build(),
        )
        .input_rule(carina_core::schema::InputRule::new(carina_core::schema::InputRuleScope::CreateOnly, carina_core::schema::InputRuleTarget::Value(carina_core::schema::InputPath::new("path").expect("valid input path")), carina_core::schema::InputRuleCode::new("aws.additional_pattern.pattern_0").expect("valid input rule code"), carina_core::schema::InputPredicate::Matches(carina_core::schema::PatternSpec::new("^(?:/|/[\\x21-\\x7E]+/)\\z", carina_core::schema::PatternMatch::Full))))
        .with_declared_validators(vec![ValidatorType::TagsKeyValueCheck])
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("iam.Role", &[])
}
