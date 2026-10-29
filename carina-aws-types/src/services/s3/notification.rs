use carina_core::schema::{AttributeType, StructField};

use crate::enum_with_dsl_aliases_and_legacy;

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
            StructField::required("events", AttributeType::list(AttributeType::string()))
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
            StructField::required("events", AttributeType::list(AttributeType::string()))
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
            StructField::required("events", AttributeType::list(AttributeType::string()))
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
