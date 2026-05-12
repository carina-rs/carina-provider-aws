//! sqs.Queue schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.sqs
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use super::tags_type;
use super::validate_tags_map;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};

/// Returns the schema config for sqs.Queue (Smithy: com.amazonaws.sqs)
pub fn sqs_queue_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::SQS::Queue",
        resource_type_name: "sqs.Queue",
        has_tags: true,
        schema: ResourceSchema::new("sqs.Queue")
        .with_description("")
        .attribute(
            AttributeSchema::new("queue_name", AttributeType::String)
                .required()
                .create_only()
                .with_description("The name of the new queue. The following limits apply to this name: A queue name can have up to 80 characters. Valid values: alphanumeric characters, ...")
                .with_provider_name("QueueName"),
        )
        .attribute(
            AttributeSchema::new("tags", AttributeType::map(AttributeType::String))
                .create_only()
                .with_description("Add cost allocation tags to the specified Amazon SQS queue. For an overview, see Tagging Your Amazon SQS Queues in the Amazon SQS Developer Guide. Whe...")
                .with_provider_name("tags"),
        )
        .attribute(
            AttributeSchema::new("visibility_timeout", AttributeType::Int)
                .with_description("The visibility timeout for the queue, in seconds. Defaults to 30. Range 0–43,200.")
                .with_provider_name("VisibilityTimeout"),
        )
        .attribute(
            AttributeSchema::new("message_retention_period", AttributeType::Int)
                .with_description("The length of time, in seconds, for which Amazon SQS retains a message. Defaults to 345,600 (4 days). Range 60–1,209,600 (1 minute–14 days).")
                .with_provider_name("MessageRetentionPeriod"),
        )
        .attribute(
            AttributeSchema::new("delay_seconds", AttributeType::Int)
                .with_description("The length of time, in seconds, for which the delivery of all messages in the queue is delayed. Defaults to 0. Range 0–900 (15 minutes).")
                .with_provider_name("DelaySeconds"),
        )
        .attribute(
            AttributeSchema::new("maximum_message_size", AttributeType::Int)
                .with_description("The limit of how many bytes a message can contain before Amazon SQS rejects it. Defaults to 262,144 (256 KiB). Range 1,024–262,144 (1–256 KiB).")
                .with_provider_name("MaximumMessageSize"),
        )
        .attribute(
            AttributeSchema::new("queue_arn", super::arn())
                .read_only()
                .with_description("The Amazon Resource Name (ARN) of the queue. (read-only)")
                .with_provider_name("QueueArn"),
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
    ("sqs.Queue", &[])
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
