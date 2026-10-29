//! logs.LogGroup schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.cloudwatchlogs
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use super::tags_type;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};
use carina_provider_protocol::types::ValidatorType;

const VALID_LOG_GROUP_CLASS: &[&str] = &["DELIVERY", "INFREQUENT_ACCESS", "STANDARD"];

/// Returns the schema config for logs.LogGroup (Smithy: com.amazonaws.cloudwatchlogs)
pub fn logs_log_group_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::Logs::LogGroup",
        resource_type_name: "logs.LogGroup",
        has_tags: true,
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
            AttributeSchema::optional("kms_key_id", super::super::kms::key::id())
                .create_only()
                .with_description("The Amazon Resource Name (ARN) of the KMS key to use when encrypting log data. For more information, see Amazon Resource Names.")
                .with_provider_name("kmsKeyId")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("log_group_class", AttributeType::enum_(
    carina_core::schema::enum_identity("LogGroupClass", Some("aws.logs.LogGroup")),
    Some(vec!["DELIVERY".to_string(), "INFREQUENT_ACCESS".to_string(), "STANDARD".to_string()]),
    vec![("DELIVERY".to_string(), "delivery".to_string()), ("INFREQUENT_ACCESS".to_string(), "infrequent_access".to_string()), ("STANDARD".to_string(), "standard".to_string())],
    None,
    None,
))
                .create_only()
                .with_description("Use this parameter to specify the log group class for this log group. There are three classes: The Standard log class supports all CloudWatch Logs fea...")
                .with_provider_name("logGroupClass")
                .build(),
        )
        .attribute(
            AttributeSchema::required("log_group_name", AttributeType::string())
                .create_only()
                .with_description("A name for the log group.")
                .with_provider_name("logGroupName")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("retention_in_days", AttributeType::int())
                .with_description("The number of days to retain the log events in the log group. If unset, events never expire. The provider applies changes via PutRetentionPolicy / Del...")
                .with_provider_name("retentionInDays")
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
        "logs.LogGroup",
        &[("log_group_class", VALID_LOG_GROUP_CLASS)],
    )
}
