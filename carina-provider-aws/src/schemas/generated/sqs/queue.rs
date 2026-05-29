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
            AttributeSchema::new("queue_name", AttributeType::string())
                .required()
                .create_only()
                .with_description("The name of the new queue. The following limits apply to this name: A queue name can have up to 80 characters. Valid values: alphanumeric characters, ...")
                .with_provider_name("QueueName"),
        )
        .attribute(
            AttributeSchema::new("tags", AttributeType::map(AttributeType::string()))
                .create_only()
                .with_description("Add cost allocation tags to the specified Amazon SQS queue. For an overview, see Tagging Your Amazon SQS Queues in the Amazon SQS Developer Guide. Whe...")
                .with_provider_name("tags"),
        )
        .attribute(
            AttributeSchema::new("policy", super::iam_policy_document())
                .with_description("The queue's access policy, as a JSON-encoded IAM policy document.")
                .with_provider_name("Policy"),
        )
        .attribute(
            AttributeSchema::new("visibility_timeout", AttributeType::int())
                .with_description("The visibility timeout for the queue, in seconds. Defaults to 30. Range 0–43,200.")
                .with_provider_name("VisibilityTimeout"),
        )
        .attribute(
            AttributeSchema::new("message_retention_period", AttributeType::int())
                .with_description("The length of time, in seconds, for which Amazon SQS retains a message. Defaults to 345,600 (4 days). Range 60–1,209,600 (1 minute–14 days).")
                .with_provider_name("MessageRetentionPeriod"),
        )
        .attribute(
            AttributeSchema::new("delay_seconds", AttributeType::int())
                .with_description("The length of time, in seconds, for which the delivery of all messages in the queue is delayed. Defaults to 0. Range 0–900 (15 minutes).")
                .with_provider_name("DelaySeconds"),
        )
        .attribute(
            AttributeSchema::new("maximum_message_size", AttributeType::int())
                .with_description("The limit of how many bytes a message can contain before Amazon SQS rejects it. Defaults to 262,144 (256 KiB). Range 1,024–262,144 (1–256 KiB).")
                .with_provider_name("MaximumMessageSize"),
        )
        .attribute(
            AttributeSchema::new("receive_message_wait_time_seconds", AttributeType::int())
                .with_description("The duration, in seconds, for which a ReceiveMessage call waits for a message to arrive (long polling). Defaults to 0. Range 0–20.")
                .with_provider_name("ReceiveMessageWaitTimeSeconds"),
        )
        .attribute(
            AttributeSchema::new("redrive_policy", super::sqs_redrive_policy())
                .with_description("Dead-letter queue redrive policy, as a JSON document with `deadLetterTargetArn` and `maxReceiveCount` keys.")
                .with_provider_name("RedrivePolicy"),
        )
        .attribute(
            AttributeSchema::new("redrive_allow_policy", super::sqs_redrive_allow_policy())
                .with_description("Permissions for source queues that can use this queue as their dead-letter destination, as a JSON document.")
                .with_provider_name("RedriveAllowPolicy"),
        )
        .attribute(
            AttributeSchema::new("fifo_queue", AttributeType::bool())
                .create_only()
                .with_description("Whether the queue is FIFO. Create-only. The queue name must end with `.fifo` when true.")
                .with_provider_name("FifoQueue"),
        )
        .attribute(
            AttributeSchema::new("content_based_deduplication", AttributeType::bool())
                .with_description("FIFO only: enables content-based message deduplication using a SHA-256 hash of the message body.")
                .with_provider_name("ContentBasedDeduplication"),
        )
        .attribute(
            AttributeSchema::new("deduplication_scope", AttributeType::string())
                .create_only()
                .with_description("FIFO only, create-only: scope of message deduplication. Valid values: `messageGroup`, `queue`.")
                .with_provider_name("DeduplicationScope"),
        )
        .attribute(
            AttributeSchema::new("fifo_throughput_limit", AttributeType::string())
                .create_only()
                .with_description("FIFO only, create-only: per-queue or per-message-group throughput limit. Valid values: `perQueue`, `perMessageGroupId`.")
                .with_provider_name("FifoThroughputLimit"),
        )
        .attribute(
            AttributeSchema::new("kms_master_key_id", super::kms_key_id())
                .with_description("The ID of an AWS KMS customer master key (CMK) to use for server-side encryption (SSE-KMS). Use `alias/aws/sqs` for the AWS-managed key.")
                .with_provider_name("KmsMasterKeyId"),
        )
        .attribute(
            AttributeSchema::new("kms_data_key_reuse_period_seconds", AttributeType::int())
                .with_description("Length of time, in seconds, that SQS can reuse a data key before invoking KMS again. Defaults to 300 (5 minutes). Range 60–86,400.")
                .with_provider_name("KmsDataKeyReusePeriodSeconds"),
        )
        .attribute(
            AttributeSchema::new("sqs_managed_sse_enabled", AttributeType::bool())
                .with_description("Enables SQS-managed server-side encryption (SSE-SQS). Mutually exclusive with KmsMasterKeyId.")
                .with_provider_name("SqsManagedSseEnabled"),
        )
        .attribute(
            AttributeSchema::new("queue_arn", super::arn())
                .read_only()
                .with_description("The Amazon Resource Name (ARN) of the queue. (read-only)")
                .with_provider_name("QueueArn"),
        )
        .attribute(
            AttributeSchema::new("approximate_number_of_messages", AttributeType::int())
                .read_only()
                .with_description("Approximate number of visible messages in the queue. Runtime metric reported by the SQS service. (read-only)")
                .with_provider_name("ApproximateNumberOfMessages"),
        )
        .attribute(
            AttributeSchema::new("approximate_number_of_messages_delayed", AttributeType::int())
                .read_only()
                .with_description("Approximate number of messages currently being delayed before becoming visible. Runtime metric. (read-only)")
                .with_provider_name("ApproximateNumberOfMessagesDelayed"),
        )
        .attribute(
            AttributeSchema::new("approximate_number_of_messages_not_visible", AttributeType::int())
                .read_only()
                .with_description("Approximate number of messages that are in flight (received but not yet deleted or returned to the queue). Runtime metric. (read-only)")
                .with_provider_name("ApproximateNumberOfMessagesNotVisible"),
        )
        .attribute(
            AttributeSchema::new("created_timestamp", AttributeType::int())
                .read_only()
                .with_description("Unix epoch seconds at which the queue was created. (read-only)")
                .with_provider_name("CreatedTimestamp"),
        )
        .attribute(
            AttributeSchema::new("last_modified_timestamp", AttributeType::int())
                .read_only()
                .with_description("Unix epoch seconds at which the queue's attributes were last modified. (read-only)")
                .with_provider_name("LastModifiedTimestamp"),
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
