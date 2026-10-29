//! sqs.Queue schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.sqs
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use super::tags_type;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};
use carina_provider_protocol::types::ValidatorType;

/// Returns the schema config for sqs.Queue (Smithy: com.amazonaws.sqs)
pub fn sqs_queue_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::SQS::Queue",
        resource_type_name: "sqs.Queue",
        has_tags: true,
        schema: ResourceSchema::new("sqs.Queue")
        .with_description("")
        .with_conflicting_replacement()
        .attribute(
            AttributeSchema::required("queue_name", AttributeType::string())
                .create_only()
                .with_description("The name of the new queue. The following limits apply to this name: A queue name can have up to 80 characters. Valid values: alphanumeric characters, ...")
                .with_provider_name("QueueName")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("policy", super::iam_policy_document())
                .with_description("The queue's access policy, as a JSON-encoded IAM policy document.")
                .with_provider_name("Policy")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("visibility_timeout", AttributeType::int())
                .with_description("The visibility timeout for the queue, in seconds. Defaults to 30. Range 0–43,200.")
                .with_provider_name("VisibilityTimeout")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("message_retention_period", AttributeType::int())
                .with_description("The length of time, in seconds, for which Amazon SQS retains a message. Defaults to 345,600 (4 days). Range 60–1,209,600 (1 minute–14 days).")
                .with_provider_name("MessageRetentionPeriod")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("delay_seconds", AttributeType::int())
                .with_description("The length of time, in seconds, for which the delivery of all messages in the queue is delayed. Defaults to 0. Range 0–900 (15 minutes).")
                .with_provider_name("DelaySeconds")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("maximum_message_size", AttributeType::int())
                .with_description("The limit of how many bytes a message can contain before Amazon SQS rejects it. Defaults to 262,144 (256 KiB). Range 1,024–262,144 (1–256 KiB).")
                .with_provider_name("MaximumMessageSize")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("receive_message_wait_time_seconds", AttributeType::int())
                .with_description("The duration, in seconds, for which a ReceiveMessage call waits for a message to arrive (long polling). Defaults to 0. Range 0–20.")
                .with_provider_name("ReceiveMessageWaitTimeSeconds")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("redrive_policy", super::sqs_redrive_policy())
                .with_description("Dead-letter queue redrive policy, as a JSON document with `deadLetterTargetArn` and `maxReceiveCount` keys.")
                .with_provider_name("RedrivePolicy")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("redrive_allow_policy", super::sqs_redrive_allow_policy())
                .with_description("Permissions for source queues that can use this queue as their dead-letter destination, as a JSON document.")
                .with_provider_name("RedriveAllowPolicy")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("fifo_queue", AttributeType::bool())
                .create_only()
                .with_description("Whether the queue is FIFO. Create-only. The queue name must end with `.fifo` when true.")
                .with_provider_name("FifoQueue")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("content_based_deduplication", AttributeType::bool())
                .with_description("FIFO only: enables content-based message deduplication using a SHA-256 hash of the message body.")
                .with_provider_name("ContentBasedDeduplication")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("deduplication_scope", AttributeType::string())
                .create_only()
                .with_description("FIFO only, create-only: scope of message deduplication. Valid values: `messageGroup`, `queue`.")
                .with_provider_name("DeduplicationScope")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("fifo_throughput_limit", AttributeType::string())
                .create_only()
                .with_description("FIFO only, create-only: per-queue or per-message-group throughput limit. Valid values: `perQueue`, `perMessageGroupId`.")
                .with_provider_name("FifoThroughputLimit")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("kms_master_key_id", super::super::kms::key::id())
                .with_description("The ID of an AWS KMS customer master key (CMK) to use for server-side encryption (SSE-KMS). Use `alias/aws/sqs` for the AWS-managed key.")
                .with_provider_name("KmsMasterKeyId")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("kms_data_key_reuse_period_seconds", AttributeType::int())
                .with_description("Length of time, in seconds, that SQS can reuse a data key before invoking KMS again. Defaults to 300 (5 minutes). Range 60–86,400.")
                .with_provider_name("KmsDataKeyReusePeriodSeconds")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("sqs_managed_sse_enabled", AttributeType::bool())
                .with_description("Enables SQS-managed server-side encryption (SSE-SQS). Mutually exclusive with KmsMasterKeyId.")
                .with_provider_name("SqsManagedSseEnabled")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("queue_arn", super::arn())
                .with_description("The Amazon Resource Name (ARN) of the queue. (read-only)")
                .with_provider_name("QueueArn")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("approximate_number_of_messages", AttributeType::int())
                .with_description("Approximate number of visible messages in the queue. Runtime metric reported by the SQS service. (read-only)")
                .with_provider_name("ApproximateNumberOfMessages")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("approximate_number_of_messages_delayed", AttributeType::int())
                .with_description("Approximate number of messages currently being delayed before becoming visible. Runtime metric. (read-only)")
                .with_provider_name("ApproximateNumberOfMessagesDelayed")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("approximate_number_of_messages_not_visible", AttributeType::int())
                .with_description("Approximate number of messages that are in flight (received but not yet deleted or returned to the queue). Runtime metric. (read-only)")
                .with_provider_name("ApproximateNumberOfMessagesNotVisible")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("created_timestamp", AttributeType::int())
                .with_description("Unix epoch seconds at which the queue was created. (read-only)")
                .with_provider_name("CreatedTimestamp")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("last_modified_timestamp", AttributeType::int())
                .with_description("Unix epoch seconds at which the queue's attributes were last modified. (read-only)")
                .with_provider_name("LastModifiedTimestamp")
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
    ("sqs.Queue", &[])
}
