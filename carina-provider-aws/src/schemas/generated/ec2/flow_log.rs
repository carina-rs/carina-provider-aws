//! flow_log schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.ec2
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use super::tags_type;
use super::validate_tags_map;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};

const VALID_LOG_DESTINATION_TYPE: &[&str] = &["cloud-watch-logs", "kinesis-data-firehose", "s3"];

const VALID_RESOURCE_TYPE: &[&str] = &[
    "NetworkInterface",
    "RegionalNatGateway",
    "Subnet",
    "TransitGateway",
    "TransitGatewayAttachment",
    "VPC",
];

const VALID_TRAFFIC_TYPE: &[&str] = &["ACCEPT", "ALL", "REJECT"];

/// Returns the schema config for ec2.flow_log (Smithy: com.amazonaws.ec2)
pub fn ec2_flow_log_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::EC2::FlowLog",
        resource_type_name: "ec2.flow_log",
        has_tags: true,
        schema: ResourceSchema::new("aws.ec2.flow_log")
        .with_description("Describes a flow log.")
        .attribute(
            AttributeSchema::new("deliver_logs_permission_arn", super::iam_role_arn())
                .create_only()
                .with_description("The ARN of the IAM role that allows Amazon EC2 to publish flow logs to the log destination. This parameter is required if the destination type is clou...")
                .with_provider_name("DeliverLogsPermissionArn"),
        )
        .attribute(
            AttributeSchema::new("log_destination", super::arn())
                .create_only()
                .with_description("The destination for the flow log data. The meaning of this parameter depends on the destination type. If the destination type is cloud-watch-logs, spe...")
                .with_provider_name("LogDestination"),
        )
        .attribute(
            AttributeSchema::new("log_destination_type", AttributeType::StringEnum {
                name: "LogDestinationType".to_string(),
                values: vec!["cloud-watch-logs".to_string(), "kinesis-data-firehose".to_string(), "s3".to_string()],
                namespace: Some("aws.ec2.flow_log".to_string()),
                to_dsl: Some(|s: &str| s.replace('-', "_")),
            })
                .create_only()
                .with_description("The type of destination for the flow log data. Default: cloud-watch-logs")
                .with_provider_name("LogDestinationType"),
        )
        .attribute(
            AttributeSchema::new("log_format", AttributeType::String)
                .create_only()
                .with_description("The fields to include in the flow log record. List the fields in the order in which they should appear. If you omit this parameter, the flow log is cr...")
                .with_provider_name("LogFormat"),
        )
        .attribute(
            AttributeSchema::new("log_group_name", AttributeType::String)
                .create_only()
                .with_description("The name of a new or existing CloudWatch Logs log group where Amazon EC2 publishes your flow logs. This parameter is valid only if the destination typ...")
                .with_provider_name("LogGroupName"),
        )
        .attribute(
            AttributeSchema::new("max_aggregation_interval", AttributeType::Int)
                .create_only()
                .with_description("The maximum interval of time during which a flow of packets is captured and aggregated into a flow log record. The possible values are 60 seconds (1 m...")
                .with_provider_name("MaxAggregationInterval"),
        )
        .attribute(
            AttributeSchema::new("resource_ids", AttributeType::list(AttributeType::String))
                .required()
                .create_only()
                .with_description("The IDs of the resources to monitor. For example, if the resource type is VPC, specify the IDs of the VPCs. Constraints: Maximum of 25 for transit gat...")
                .with_provider_name("ResourceIds"),
        )
        .attribute(
            AttributeSchema::new("resource_type", AttributeType::StringEnum {
                name: "ResourceType".to_string(),
                values: vec!["NetworkInterface".to_string(), "RegionalNatGateway".to_string(), "Subnet".to_string(), "TransitGateway".to_string(), "TransitGatewayAttachment".to_string(), "VPC".to_string()],
                namespace: Some("aws.ec2.flow_log".to_string()),
                to_dsl: None,
            })
                .required()
                .create_only()
                .with_description("The type of resource to monitor.")
                .with_provider_name("ResourceType"),
        )
        .attribute(
            AttributeSchema::new("traffic_type", AttributeType::StringEnum {
                name: "TrafficType".to_string(),
                values: vec!["ACCEPT".to_string(), "ALL".to_string(), "REJECT".to_string()],
                namespace: Some("aws.ec2.flow_log".to_string()),
                to_dsl: None,
            })
                .create_only()
                .with_description("The type of traffic to monitor (accepted traffic, rejected traffic, or all traffic). This parameter is not supported for transit gateway resource type...")
                .with_provider_name("TrafficType"),
        )
        .attribute(
            AttributeSchema::new("flow_log_id", AttributeType::String)
                .with_description("The ID of the flow log. (read-only)")
                .with_provider_name("FlowLogId"),
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
    (
        "ec2.flow_log",
        &[
            ("log_destination_type", VALID_LOG_DESTINATION_TYPE),
            ("resource_type", VALID_RESOURCE_TYPE),
            ("traffic_type", VALID_TRAFFIC_TYPE),
        ],
    )
}

/// Maps DSL alias values back to canonical AWS values for this module.
/// e.g., ("ip_protocol", "all") -> Some("-1")
pub fn enum_alias_reverse(attr_name: &str, value: &str) -> Option<&'static str> {
    let _ = (attr_name, value);
    None
}
