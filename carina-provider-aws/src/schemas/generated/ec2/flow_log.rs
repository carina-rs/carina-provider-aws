//! FlowLog schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.ec2
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use super::tags_type;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};
use carina_provider_protocol::types::ValidatorType;

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

/// Returns the schema config for ec2.FlowLog (Smithy: com.amazonaws.ec2)
pub fn ec2_flow_log_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::EC2::FlowLog",
        resource_type_name: "ec2.FlowLog",
        has_tags: true,
        schema: ResourceSchema::new("ec2.FlowLog")
        .with_description("Describes a flow log.")
        .with_conflicting_replacement()
        .attribute(
            AttributeSchema::optional("deliver_logs_permission_arn", super::super::iam::role::arn())
                .create_only()
                .with_description("The ARN of the IAM role that allows Amazon EC2 to publish flow logs to the log destination. This parameter is required if the destination type is clou...")
                .with_provider_name("DeliverLogsPermissionArn")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("log_destination", super::arn())
                .create_only()
                .with_description("The destination for the flow log data. The meaning of this parameter depends on the destination type. If the destination type is cloud-watch-logs, spe...")
                .with_provider_name("LogDestination")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("log_destination_type", AttributeType::enum_(
    carina_core::schema::enum_identity("LogDestinationType", Some("aws.ec2.FlowLog")),
    Some(vec!["cloud-watch-logs".to_string(), "kinesis-data-firehose".to_string(), "s3".to_string()]),
    vec![("cloud-watch-logs".to_string(), "cloud_watch_logs".to_string()), ("kinesis-data-firehose".to_string(), "kinesis_data_firehose".to_string()), ("s3".to_string(), "s3".to_string())],
    None,
    None,
))
                .create_only()
                .with_description("The type of destination for the flow log data. Default: cloud-watch-logs")
                .with_provider_name("LogDestinationType")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("log_format", AttributeType::string())
                .create_only()
                .with_description("The fields to include in the flow log record. List the fields in the order in which they should appear. If you omit this parameter, the flow log is cr...")
                .with_provider_name("LogFormat")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("log_group_name", AttributeType::string())
                .create_only()
                .with_description("The name of a new or existing CloudWatch Logs log group where Amazon EC2 publishes your flow logs. This parameter is valid only if the destination typ...")
                .with_provider_name("LogGroupName")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("max_aggregation_interval", AttributeType::int())
                .create_only()
                .with_description("The maximum interval of time during which a flow of packets is captured and aggregated into a flow log record. The possible values are 60 seconds (1 m...")
                .with_provider_name("MaxAggregationInterval")
                .build(),
        )
        .attribute(
            AttributeSchema::required("resource_ids", AttributeType::unordered_list(AttributeType::string()))
                .create_only()
                .with_description("The IDs of the resources to monitor. For example, if the resource type is VPC, specify the IDs of the VPCs. Constraints: Maximum of 25 for transit gat...")
                .with_provider_name("ResourceIds")
                .build(),
        )
        .attribute(
            AttributeSchema::required("resource_type", AttributeType::enum_(
    carina_core::schema::enum_identity("ResourceType", Some("aws.ec2.FlowLog")),
    Some(vec!["NetworkInterface".to_string(), "RegionalNatGateway".to_string(), "Subnet".to_string(), "TransitGateway".to_string(), "TransitGatewayAttachment".to_string(), "VPC".to_string()]),
    vec![("NetworkInterface".to_string(), "network_interface".to_string()), ("RegionalNatGateway".to_string(), "regional_nat_gateway".to_string()), ("Subnet".to_string(), "subnet".to_string()), ("TransitGateway".to_string(), "transit_gateway".to_string()), ("TransitGatewayAttachment".to_string(), "transit_gateway_attachment".to_string()), ("VPC".to_string(), "vpc".to_string())],
    None,
    None,
))
                .create_only()
                .with_description("The type of resource to monitor.")
                .with_provider_name("ResourceType")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("traffic_type", AttributeType::enum_(
    carina_core::schema::enum_identity("TrafficType", Some("aws.ec2.FlowLog")),
    Some(vec!["ACCEPT".to_string(), "ALL".to_string(), "REJECT".to_string()]),
    vec![("ACCEPT".to_string(), "accept".to_string()), ("ALL".to_string(), "all".to_string()), ("REJECT".to_string(), "reject".to_string())],
    None,
    None,
))
                .create_only()
                .with_description("The type of traffic to monitor (accepted traffic, rejected traffic, or all traffic). This parameter is not supported for transit gateway resource type...")
                .with_provider_name("TrafficType")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("flow_log_id", AttributeType::string())
                .with_description("The ID of the flow log. (read-only)")
                .with_provider_name("FlowLogId")
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
    (
        "ec2.FlowLog",
        &[
            ("log_destination_type", VALID_LOG_DESTINATION_TYPE),
            ("resource_type", VALID_RESOURCE_TYPE),
            ("traffic_type", VALID_TRAFFIC_TYPE),
        ],
    )
}
