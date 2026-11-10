//! logs.LogGroup schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.cloudwatchlogs
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};
use carina_provider_protocol::types::ValidatorType;

const VALID_LOG_GROUP_CLASS: &[&str] = &["DELIVERY", "ESSENTIALS", "INFREQUENT_ACCESS", "STANDARD"];

/// Returns the schema config for logs.LogGroup (Smithy: com.amazonaws.cloudwatchlogs)
pub fn logs_log_group_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::Logs::LogGroup",
        resource_type_name: "logs.LogGroup",
        has_tags: true,
        await_on_read_back: vec![],
        schema: ResourceSchema::new("logs.LogGroup")
        .with_description("Represents a log group.")
        .with_unique_name_attribute("log_group_name")
        .attribute(
            AttributeSchema::optional("deletion_protection_enabled", AttributeType::bool())
                .with_description("Use this parameter to enable deletion protection for the new log group. When enabled on a log group, deletion protection blocks all deletion operation...")
                .with_provider_name("deletionProtectionEnabled")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("kms_key_id", carina_aws_types::with_member_reference(AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(None, Some((Some(0), Some(256))))), carina_aws_types::kms_key_arn_reference()))
                .create_only()
                .with_description("The Amazon Resource Name (ARN) of the KMS key to use when encrypting log data. For more information, see Amazon Resource Names.")
                .with_provider_name("kmsKeyId")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("log_group_class", AttributeType::enum_(carina_core::schema::enum_identity("LogGroupClass", Some("aws.logs.LogGroup")), Some(vec!["DELIVERY".to_string(), "ESSENTIALS".to_string(), "INFREQUENT_ACCESS".to_string(), "STANDARD".to_string()]), vec![("DELIVERY".to_string(), "delivery".to_string()), ("ESSENTIALS".to_string(), "essentials".to_string()), ("INFREQUENT_ACCESS".to_string(), "infrequent_access".to_string()), ("STANDARD".to_string(), "standard".to_string())], None, None))
                .create_only()
                .with_description("Use this parameter to specify the log group class for this log group. There are four classes: The Standard log class supports all CloudWatch Logs feat...")
                .with_provider_name("logGroupClass")
                .build(),
        )
        .attribute(
            AttributeSchema::required("log_group_name", AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(Some(carina_core::schema::PatternSpec::new("^[\\.\\-_/#A-Za-z0-9]+$", carina_core::schema::PatternMatch::Full)), Some((Some(1), Some(512))))))
                .create_only()
                .identity()
                .with_description("A name for the log group.")
                .with_provider_name("logGroupName")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("retention_in_days", carina_aws_types::with_member_identity(AttributeType::int_member(carina_core::schema::IntIdentityLayer::new(None, None), carina_core::schema::IntMemberConstraints::new(Some((Some(-2147483648), Some(2147483647))))), carina_aws_types::AwsType::LogsLogGroupRetentionInDays.attribute_type()))
                .with_description("The number of days to retain the log events in the log group. If unset, events never expire. The provider applies changes via PutRetentionPolicy / Del...")
                .with_provider_name("retentionInDays")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("tags", AttributeType::refined_map(AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(Some(carina_core::schema::PatternSpec::new("^([\\p{L}\\p{Z}\\p{N}_.:/=+\\-@]+)$", carina_core::schema::PatternMatch::Full)), Some((Some(1), Some(128))))), AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(Some(carina_core::schema::PatternSpec::new("^([\\p{L}\\p{Z}\\p{N}_.:/=+\\-@]*)$", carina_core::schema::PatternMatch::Full)), Some((Some(0), Some(256))))), Some((Some(1), Some(50)))))
                .with_description("The tags for the resource.")
                .with_provider_name("Tags")
                .build(),
        )
        .input_rule(carina_core::schema::InputRule::new(carina_core::schema::InputRuleScope::CreateOnly, carina_core::schema::InputRuleTarget::Value(carina_core::schema::InputPath::new("kms_key_id").expect("valid input path")), carina_core::schema::InputRuleCode::new("aws.additional_pattern.pattern_0").expect("valid input rule code"), carina_core::schema::InputPredicate::Matches(carina_core::schema::PatternSpec::new("^arn:(?:aws|aws-cn|aws-us-gov|aws-iso|aws-iso-b|aws-iso-e|aws-iso-f|aws-eusc):kms:[a-z]{2,4}(-[a-z0-9]+)+-[0-9]+:[0-9]{12}:key/([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}|mrk-[0-9a-fA-F]{32})$", carina_core::schema::PatternMatch::Full))))
        .with_declared_validators(vec![ValidatorType::TagsKeyValueCheck])
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    (
        "logs.LogGroup",
        &[("log_group_class", VALID_LOG_GROUP_CLASS)],
    )
}
