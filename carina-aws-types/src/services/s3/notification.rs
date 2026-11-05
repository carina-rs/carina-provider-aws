use carina_core::schema::{AttributeType, StructField};

use crate::enum_with_dsl_aliases_and_legacy;

// TopicConfiguration.Events and the installed SDK share these wire values.
// https://docs.aws.amazon.com/AmazonS3/latest/API/API_TopicConfiguration.html
fn s3_notification_event() -> AttributeType {
    crate::enum_with_dsl_aliases(
        &[
            "s3:ReducedRedundancyLostObject",
            "s3:ObjectCreated:*",
            "s3:ObjectCreated:Put",
            "s3:ObjectCreated:Post",
            "s3:ObjectCreated:Copy",
            "s3:ObjectCreated:CompleteMultipartUpload",
            "s3:ObjectRemoved:*",
            "s3:ObjectRemoved:Delete",
            "s3:ObjectRemoved:DeleteMarkerCreated",
            "s3:ObjectRestore:*",
            "s3:ObjectRestore:Post",
            "s3:ObjectRestore:Completed",
            "s3:Replication:*",
            "s3:Replication:OperationFailedReplication",
            "s3:Replication:OperationNotTracked",
            "s3:Replication:OperationMissedThreshold",
            "s3:Replication:OperationReplicatedAfterThreshold",
            "s3:ObjectRestore:Delete",
            "s3:LifecycleTransition",
            "s3:IntelligentTiering",
            "s3:ObjectAcl:Put",
            "s3:LifecycleExpiration:*",
            "s3:LifecycleExpiration:Delete",
            "s3:LifecycleExpiration:DeleteMarkerCreated",
            "s3:ObjectTagging:*",
            "s3:ObjectTagging:Put",
            "s3:ObjectTagging:Delete",
            "s3:ObjectAnnotation:*",
            "s3:ObjectAnnotation:Put",
            "s3:ObjectAnnotation:Delete",
            "s3:ObjectRetention:Put",
        ],
        carina_core::schema::enum_identity("Event", Some("aws.s3.BucketNotificationConfiguration")),
    )
}

// Events is required for each of the three destination types.
// https://docs.aws.amazon.com/AmazonS3/latest/API/API_TopicConfiguration.html
// https://docs.aws.amazon.com/AmazonS3/latest/API/API_QueueConfiguration.html
// https://docs.aws.amazon.com/AmazonS3/latest/API/API_LambdaFunctionConfiguration.html
fn s3_notification_events() -> AttributeType {
    super::nonempty_list(s3_notification_event())
}

fn s3_notification_filter_rule() -> AttributeType {
    AttributeType::struct_(
        "FilterRule".to_string(),
        vec![
            StructField::required(
                "name",
                enum_with_dsl_aliases_and_legacy(
                    &["prefix", "suffix"],
                    &[("prefix", "Prefix"), ("suffix", "Suffix")],
                    carina_core::schema::enum_identity(
                        "Name",
                        Some(
                            "aws.s3.BucketNotificationConfiguration.NotificationFilter.FilterRule",
                        ),
                    ),
                ),
            )
            .with_provider_name("Name")
            .build(),
            StructField::required("value", AttributeType::string())
                .with_provider_name("Value")
                .build(),
        ],
    )
}

fn s3_notification_filter() -> AttributeType {
    AttributeType::struct_(
        "NotificationFilter".to_string(),
        vec![
            StructField::optional(
                "filter_rules",
                AttributeType::list(s3_notification_filter_rule()),
            )
            .with_provider_name("FilterRules")
            .build(),
        ],
    )
}

fn s3_topic_configuration() -> AttributeType {
    AttributeType::struct_(
        "TopicConfiguration".to_string(),
        vec![
            StructField::optional("id", AttributeType::string())
                .with_provider_name("Id")
                .build(),
            StructField::required("topic_arn", AttributeType::string())
                .with_provider_name("TopicArn")
                .build(),
            StructField::required("events", s3_notification_events())
                .with_provider_name("Events")
                .build(),
            StructField::optional("filter", s3_notification_filter())
                .with_provider_name("Filter")
                .build(),
        ],
    )
}

fn s3_queue_configuration() -> AttributeType {
    AttributeType::struct_(
        "QueueConfiguration".to_string(),
        vec![
            StructField::optional("id", AttributeType::string())
                .with_provider_name("Id")
                .build(),
            StructField::required("queue_arn", AttributeType::string())
                .with_provider_name("QueueArn")
                .build(),
            StructField::required("events", s3_notification_events())
                .with_provider_name("Events")
                .build(),
            StructField::optional("filter", s3_notification_filter())
                .with_provider_name("Filter")
                .build(),
        ],
    )
}

fn s3_lambda_function_configuration() -> AttributeType {
    AttributeType::struct_(
        "LambdaFunctionConfiguration".to_string(),
        vec![
            StructField::optional("id", AttributeType::string())
                .with_provider_name("Id")
                .build(),
            StructField::required("lambda_function_arn", AttributeType::string())
                .with_provider_name("LambdaFunctionArn")
                .build(),
            StructField::required("events", s3_notification_events())
                .with_provider_name("Events")
                .build(),
            StructField::optional("filter", s3_notification_filter())
                .with_provider_name("Filter")
                .build(),
        ],
    )
}

pub fn bucket_topic_configurations() -> AttributeType {
    AttributeType::list(s3_topic_configuration())
}

pub fn bucket_queue_configurations() -> AttributeType {
    AttributeType::list(s3_queue_configuration())
}

pub fn bucket_lambda_function_configurations() -> AttributeType {
    AttributeType::list(s3_lambda_function_configuration())
}

pub fn bucket_event_bridge_configuration() -> AttributeType {
    AttributeType::struct_("EventBridgeConfiguration".to_string(), vec![])
}
