use carina_core::schema::{AttributeType, StructField};

use crate::{arn, enum_with_dsl_aliases};

/// SQS dead-letter queue redrive policy. Returned by the SQS API as a
/// stringified JSON document under the `RedrivePolicy` queue attribute,
/// but it is a fixed, narrow shape (target ARN + retry count) rather
/// than an IAM-style policy — so it gets its own Struct rather than
/// reusing `iam_policy_document()`.
pub fn sqs_redrive_policy() -> AttributeType {
    AttributeType::struct_(
        "RedrivePolicy".to_string(),
        vec![
            StructField::new("dead_letter_target_arn", arn())
                .with_provider_name("deadLetterTargetArn")
                .required()
                .with_description("ARN of the dead-letter queue to which Amazon SQS moves messages after `max_receive_count` is exceeded."),
            StructField::new("max_receive_count", AttributeType::int())
                .with_provider_name("maxReceiveCount")
                .required()
                .with_description("Number of times a consumer can receive a message before it is moved to the dead-letter queue. Range 1–1,000."),
        ],
    )
}

/// `redrive_permission` enum used inside `sqs_redrive_allow_policy`.
fn sqs_redrive_permission() -> AttributeType {
    enum_with_dsl_aliases(
        &["allowAll", "denyAll", "byQueue"],
        carina_core::schema::enum_identity(
            "RedrivePermission",
            Some("aws.sqs.Queue.RedriveAllowPolicy"),
        ),
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
            StructField::new("redrive_permission", sqs_redrive_permission())
                .with_provider_name("redrivePermission")
                .required()
                .with_description("Which source queues may redrive into this queue. `allowAll`: any queue in the account; `denyAll`: none; `byQueue`: only the ARNs in `source_queue_arns`."),
            StructField::new("source_queue_arns", AttributeType::list(arn()))
                .with_provider_name("sourceQueueArns")
                .with_description("Up to 10 source queue ARNs permitted to redrive into this queue. Required when `redrive_permission` is `byQueue`."),
        ],
    )
}
