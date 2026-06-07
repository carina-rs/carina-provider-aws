use carina_core::schema::{AttributeType, StructField};

fn s3_notification_filter_rule() -> AttributeType {
    AttributeType::struct_(
        "FilterRule".to_string(),
        vec![
            StructField::new("name", AttributeType::string())
                .with_provider_name("Name")
                .required(),
            StructField::new("value", AttributeType::string())
                .with_provider_name("Value")
                .required(),
        ],
    )
}

fn s3_notification_filter() -> AttributeType {
    AttributeType::struct_(
        "NotificationFilter".to_string(),
        vec![
            StructField::new(
                "filter_rules",
                AttributeType::list(s3_notification_filter_rule()),
            )
            .with_provider_name("FilterRules"),
        ],
    )
}

fn s3_topic_configuration() -> AttributeType {
    AttributeType::struct_(
        "TopicConfiguration".to_string(),
        vec![
            StructField::new("id", AttributeType::string()).with_provider_name("Id"),
            StructField::new("topic_arn", AttributeType::string())
                .with_provider_name("TopicArn")
                .required(),
            StructField::new("events", AttributeType::list(AttributeType::string()))
                .with_provider_name("Events")
                .required(),
            StructField::new("filter", s3_notification_filter()).with_provider_name("Filter"),
        ],
    )
}

fn s3_queue_configuration() -> AttributeType {
    AttributeType::struct_(
        "QueueConfiguration".to_string(),
        vec![
            StructField::new("id", AttributeType::string()).with_provider_name("Id"),
            StructField::new("queue_arn", AttributeType::string())
                .with_provider_name("QueueArn")
                .required(),
            StructField::new("events", AttributeType::list(AttributeType::string()))
                .with_provider_name("Events")
                .required(),
            StructField::new("filter", s3_notification_filter()).with_provider_name("Filter"),
        ],
    )
}

fn s3_lambda_function_configuration() -> AttributeType {
    AttributeType::struct_(
        "LambdaFunctionConfiguration".to_string(),
        vec![
            StructField::new("id", AttributeType::string()).with_provider_name("Id"),
            StructField::new("lambda_function_arn", AttributeType::string())
                .with_provider_name("LambdaFunctionArn")
                .required(),
            StructField::new("events", AttributeType::list(AttributeType::string()))
                .with_provider_name("Events")
                .required(),
            StructField::new("filter", s3_notification_filter()).with_provider_name("Filter"),
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
