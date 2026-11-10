use carina_core::schema::{AttributeType, StructField};

use crate::{sqs_queue_arn, sqs_redrive_permission};

/// SQS dead-letter queue redrive policy. Returned by the SQS API as a
/// stringified JSON document under the `RedrivePolicy` queue attribute,
/// but it is a fixed, narrow shape (target ARN + retry count) rather
/// than an IAM-style policy — so it gets its own Struct rather than
/// reusing `iam_policy_document()`.
pub fn sqs_redrive_policy() -> AttributeType {
    AttributeType::struct_(
        "RedrivePolicy".to_string(),
        vec![
            StructField::required("dead_letter_target_arn", sqs_queue_arn())
                .with_provider_name("deadLetterTargetArn")
                .with_description("ARN of the dead-letter queue to which Amazon SQS moves messages after `max_receive_count` is exceeded.").build(),
            // The cached sqs.json API prose describes the count but omits bounds;
            // the console guide explicitly gives the range 1..=1000:
            // https://docs.aws.amazon.com/AWSSimpleQueueService/latest/SQSDeveloperGuide/sqs-configure-dead-letter-queue.html
            StructField::required("max_receive_count", crate::with_int_range(AttributeType::int(), Some(1), Some(1000)))
                .with_provider_name("maxReceiveCount")
                .with_description("Number of times a consumer can receive a message before it is moved to the dead-letter queue. Range 1–1,000.").build(),
        ],
    )
}

/// SQS redrive-allow policy. Controls which source queues may use this
/// queue as their dead-letter destination. Distinct from
/// `iam_policy_document()`: the shape is `{redrivePermission, sourceQueueArns?}`,
/// not a generic IAM statement list.
pub fn sqs_redrive_allow_policy() -> AttributeType {
    AttributeType::struct_(
        "RedriveAllowPolicy".to_string(),
        vec![
            StructField::required("redrive_permission", sqs_redrive_permission())
                .with_provider_name("redrivePermission")
                .with_description("Which source queues may redrive into this queue. `allowAll`: any queue in the account; `denyAll`: none; `byQueue`: only the ARNs in `source_queue_arns`.").build(),
            // CreateQueueRequest$Attributes permits sourceQueueArns only for
            // redrivePermission=byQueue, with up to 10 ARNs. This bound is
            // conditional; protocol 23 cannot express that cross-field condition.
            // https://docs.aws.amazon.com/AWSSimpleQueueService/latest/APIReference/API_CreateQueue.html
            StructField::optional("source_queue_arns", AttributeType::list(sqs_queue_arn()))
                .with_provider_name("sourceQueueArns")
                .with_description("Up to 10 source queue ARNs permitted to redrive into this queue. Required when `redrive_permission` is `byQueue`.").build(),
        ],
    )
}
