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
            .attribute(
                AttributeSchema::new("bucket", AttributeType::String)
                    .required()
                    .create_only()
                    .with_description("The name of the bucket.")
                    .with_provider_name("Bucket"),
            )
            .attribute(
                AttributeSchema::new("topic_configurations", super::bucket_topic_configurations())
                    .with_description("SNS topic notification configurations.")
                    .with_provider_name("TopicConfigurations"),
            )
            .attribute(
                AttributeSchema::new("queue_configurations", super::bucket_queue_configurations())
                    .with_description("SQS queue notification configurations.")
                    .with_provider_name("QueueConfigurations"),
            )
            .attribute(
                AttributeSchema::new(
                    "lambda_function_configurations",
                    super::bucket_lambda_function_configurations(),
                )
                .with_description("Lambda function notification configurations.")
                .with_provider_name("LambdaFunctionConfigurations"),
            )
            .attribute(
                AttributeSchema::new(
                    "event_bridge_configuration",
                    super::bucket_event_bridge_configuration(),
                )
                .with_description("Enables EventBridge notifications when present.")
                .with_provider_name("EventBridgeConfiguration"),
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
