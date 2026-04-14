//! logs.log_group schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.cloudwatchlogs
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use super::tags_type;
use super::validate_tags_map;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};

const VALID_LOG_GROUP_CLASS: &[&str] = &["DELIVERY", "INFREQUENT_ACCESS", "STANDARD"];

/// Returns the schema config for logs.log_group (Smithy: com.amazonaws.cloudwatchlogs)
pub fn logs_log_group_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::Logs::LogGroup",
        resource_type_name: "logs.log_group",
        has_tags: true,
        schema: ResourceSchema::new("aws.logs.log_group")
        .with_description("Represents a log group.")
        .attribute(
            AttributeSchema::new("deletion_protection_enabled", AttributeType::Bool)
                .create_only()
                .with_description("Use this parameter to enable deletion protection for the new log group. When enabled on a log group, deletion protection blocks all deletion operation...")
                .with_provider_name("deletionProtectionEnabled"),
        )
        .attribute(
            AttributeSchema::new("kms_key_id", AttributeType::String)
                .create_only()
                .with_description("The Amazon Resource Name (ARN) of the KMS key to use when encrypting log data. For more information, see Amazon Resource Names.")
                .with_provider_name("kmsKeyId"),
        )
        .attribute(
            AttributeSchema::new("log_group_class", AttributeType::StringEnum {
                name: "logGroupClass".to_string(),
                values: vec!["DELIVERY".to_string(), "INFREQUENT_ACCESS".to_string(), "STANDARD".to_string()],
                namespace: Some("aws.logs.log_group".to_string()),
                to_dsl: None,
            })
                .create_only()
                .with_description("Use this parameter to specify the log group class for this log group. There are three classes: The Standard log class supports all CloudWatch Logs fea...")
                .with_provider_name("logGroupClass"),
        )
        .attribute(
            AttributeSchema::new("log_group_name", AttributeType::String)
                .required()
                .create_only()
                .with_description("A name for the log group.")
                .with_provider_name("logGroupName"),
        )
        .attribute(
            AttributeSchema::new("tags", AttributeType::map(AttributeType::String))
                .create_only()
                .with_description("The key-value pairs to use for the tags. You can grant users access to certain log groups while preventing them from accessing other log groups. To do...")
                .with_provider_name("tags"),
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
        "logs.log_group",
        &[("log_group_class", VALID_LOG_GROUP_CLASS)],
    )
}

/// Maps DSL alias values back to canonical AWS values for this module.
/// e.g., ("ip_protocol", "all") -> Some("-1")
pub fn enum_alias_reverse(attr_name: &str, value: &str) -> Option<&'static str> {
    let _ = (attr_name, value);
    None
}

/// Returns all enum alias entries as (attr_name, alias, canonical) tuples.
pub fn enum_alias_entries() -> &'static [(&'static str, &'static str, &'static str)] {
    &[]
}
