//! BucketNotificationConfiguration schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.s3
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};

/// Returns the schema config for s3.BucketNotificationConfiguration (Smithy: com.amazonaws.s3)
pub fn s3_bucket_notification_configuration_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::S3::BucketNotificationConfiguration",
        resource_type_name: "s3.BucketNotificationConfiguration",
        has_tags: false,
        schema: ResourceSchema::new("s3.BucketNotificationConfiguration")
            .with_coexisting_replacement()
            .attribute(
                AttributeSchema::required("bucket", AttributeType::string())
                    .create_only()
                    .with_description("The name of the bucket.")
                    .with_provider_name("Bucket")
                    .build(),
            )
            .attribute(
                AttributeSchema::optional(
                    "topic_configurations",
                    super::bucket_topic_configurations(),
                )
                .with_description("SNS topic notification configurations.")
                .with_provider_name("TopicConfigurations")
                .build(),
            )
            .attribute(
                AttributeSchema::optional(
                    "queue_configurations",
                    super::bucket_queue_configurations(),
                )
                .with_description("SQS queue notification configurations.")
                .with_provider_name("QueueConfigurations")
                .build(),
            )
            .attribute(
                AttributeSchema::optional(
                    "lambda_function_configurations",
                    super::bucket_lambda_function_configurations(),
                )
                .with_description("Lambda function notification configurations.")
                .with_provider_name("LambdaFunctionConfigurations")
                .build(),
            )
            .attribute(
                AttributeSchema::optional(
                    "event_bridge_configuration",
                    super::bucket_event_bridge_configuration(),
                )
                .with_description("Enables EventBridge notifications when present.")
                .with_provider_name("EventBridgeConfiguration")
                .build(),
            ),
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("s3.BucketNotificationConfiguration", &[])
}
