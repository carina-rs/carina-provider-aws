//! iam.Role schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.iam
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};
use carina_provider_protocol::types::ValidatorType;

pub fn arn() -> AttributeType {
    super::iam_role_arn()
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
            AttributeSchema::optional("description", AttributeType::string())
                .create_only()
                .with_description("A description of the role.")
                .with_provider_name("Description")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("max_session_duration", AttributeType::refined_int(
                None,
                Some((Some(3600), Some(43200))),
            ))
                .create_only()
                .with_description("The maximum session duration (in seconds) that you want to set for the specified role. If you do not specify a value for this setting, the default val...")
                .with_provider_name("MaxSessionDuration")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("path", carina_aws_types::iam_path())
                .create_only()
                .with_description("The path to the role. For more information about paths, see IAM Identifiers in the IAM User Guide. This parameter is optional. If it is not included, ...")
                .with_provider_name("Path")
                .build(),
        )
        .attribute(
            AttributeSchema::required("role_name", carina_aws_types::iam_role_name())
                .create_only()
                .identity()
                .with_description("The name of the role to create. IAM user, group, role, and policy names must be unique within the account. Names are not distinguished by case. For ex...")
                .with_provider_name("RoleName")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("arn", self::arn())
                .with_description("The Amazon Resource Name (ARN) specifying the role. For more information about ARNs and how to use them in policies, see IAM identifiers in the IAM Us... (read-only)")
                .with_provider_name("Arn")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("role_id", super::iam_role_id())
                .with_description("The stable and unique string identifying the role. For more information about IDs, see IAM identifiers in the IAM User Guide. (read-only)")
                .with_provider_name("RoleId")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("tags", AttributeType::refined_map(AttributeType::refined_string(None, Some(carina_core::schema::PatternSpec::new("^[\\p{L}\\p{Z}\\p{N}_.:/=+\\-@]+$", carina_core::schema::PatternMatch::Substring { reason: carina_core::schema::PatternReason::new("Smithy tag patterns retain their authored search semantics.").expect("visible pattern reason") })), Some((Some(1), Some(128))), None), AttributeType::refined_string(None, Some(carina_core::schema::PatternSpec::new("^[\\p{L}\\p{Z}\\p{N}_.:/=+\\-@]*$", carina_core::schema::PatternMatch::Substring { reason: carina_core::schema::PatternReason::new("Smithy tag patterns retain their authored search semantics.").expect("visible pattern reason") })), Some((Some(0), Some(256))), None), Some((Some(0), Some(50)))))
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
    ("iam.Role", &[])
}
