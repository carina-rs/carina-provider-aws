//! FlowLog schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.ec2
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use super::tags_type;
use super::validate_tags_map;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};

const VALID_LOG_DESTINATION_TYPE: &[&str] = &[
    "cloud-watch-logs",
    "kinesis-data-firehose",
    "s3",
    "cloud_watch_logs",
    "kinesis_data_firehose",
];

const VALID_RESOURCE_TYPE: &[&str] = &[
    "NetworkInterface",
    "RegionalNatGateway",
    "Subnet",
    "TransitGateway",
    "TransitGatewayAttachment",
    "VPC",
    "network_interface",
    "regional_nat_gateway",
    "subnet",
    "transit_gateway",
    "transit_gateway_attachment",
    "vpc",
];

const VALID_TRAFFIC_TYPE: &[&str] = &["ACCEPT", "ALL", "REJECT", "accept", "all", "reject"];

/// Returns the schema config for ec2.FlowLog (Smithy: com.amazonaws.ec2)
pub fn ec2_flow_log_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::EC2::FlowLog",
        resource_type_name: "ec2.FlowLog",
        has_tags: true,
        schema: ResourceSchema::new("ec2.FlowLog")
        .with_description("Describes a flow log.")
        .attribute(
            AttributeSchema::new("deliver_logs_permission_arn", super::super::iam::role::arn())
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
            AttributeSchema::new("log_destination_type", AttributeType::enum_(
    carina_core::schema::enum_identity("LogDestinationType", Some("aws.ec2.FlowLog")),
    Some(vec!["cloud-watch-logs".to_string(), "kinesis-data-firehose".to_string(), "s3".to_string()]),
    vec![("cloud-watch-logs".to_string(), "cloud_watch_logs".to_string()), ("kinesis-data-firehose".to_string(), "kinesis_data_firehose".to_string()), ("s3".to_string(), "s3".to_string())],
    None,
    None,
))
                .create_only()
                .with_description("The type of destination for the flow log data. Default: cloud-watch-logs")
                .with_provider_name("LogDestinationType"),
        )
        .attribute(
            AttributeSchema::new("log_format", AttributeType::string())
                .create_only()
                .with_description("The fields to include in the flow log record. List the fields in the order in which they should appear. If you omit this parameter, the flow log is cr...")
                .with_provider_name("LogFormat"),
        )
        .attribute(
            AttributeSchema::new("log_group_name", AttributeType::string())
                .create_only()
                .with_description("The name of a new or existing CloudWatch Logs log group where Amazon EC2 publishes your flow logs. This parameter is valid only if the destination typ...")
                .with_provider_name("LogGroupName"),
        )
        .attribute(
            AttributeSchema::new("max_aggregation_interval", AttributeType::int())
                .create_only()
                .with_description("The maximum interval of time during which a flow of packets is captured and aggregated into a flow log record. The possible values are 60 seconds (1 m...")
                .with_provider_name("MaxAggregationInterval"),
        )
        .attribute(
            AttributeSchema::new("resource_ids", AttributeType::list(AttributeType::string()))
                .required()
                .create_only()
                .with_description("The IDs of the resources to monitor. For example, if the resource type is VPC, specify the IDs of the VPCs. Constraints: Maximum of 25 for transit gat...")
                .with_provider_name("ResourceIds"),
        )
        .attribute(
            AttributeSchema::new("resource_type", AttributeType::enum_(
    carina_core::schema::enum_identity("ResourceType", Some("aws.ec2.FlowLog")),
    Some(vec!["NetworkInterface".to_string(), "RegionalNatGateway".to_string(), "Subnet".to_string(), "TransitGateway".to_string(), "TransitGatewayAttachment".to_string(), "VPC".to_string()]),
    vec![("NetworkInterface".to_string(), "network_interface".to_string()), ("RegionalNatGateway".to_string(), "regional_nat_gateway".to_string()), ("Subnet".to_string(), "subnet".to_string()), ("TransitGateway".to_string(), "transit_gateway".to_string()), ("TransitGatewayAttachment".to_string(), "transit_gateway_attachment".to_string()), ("VPC".to_string(), "vpc".to_string())],
    None,
    None,
))
                .required()
                .create_only()
                .with_description("The type of resource to monitor.")
                .with_provider_name("ResourceType"),
        )
        .attribute(
            AttributeSchema::new("traffic_type", AttributeType::enum_(
    carina_core::schema::enum_identity("TrafficType", Some("aws.ec2.FlowLog")),
    Some(vec!["ACCEPT".to_string(), "ALL".to_string(), "REJECT".to_string()]),
    vec![("ACCEPT".to_string(), "accept".to_string()), ("ALL".to_string(), "all".to_string()), ("REJECT".to_string(), "reject".to_string())],
    None,
    None,
))
                .create_only()
                .with_description("The type of traffic to monitor (accepted traffic, rejected traffic, or all traffic). This parameter is not supported for transit gateway resource type...")
                .with_provider_name("TrafficType"),
        )
        .attribute(
            AttributeSchema::new("flow_log_id", AttributeType::string())
                .read_only()
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
        "ec2.FlowLog",
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
    match (attr_name, value) {
        ("log_destination_type", "cloud_watch_logs") => Some("cloud-watch-logs"),
        ("log_destination_type", "kinesis_data_firehose") => Some("kinesis-data-firehose"),
        ("resource_type", "network_interface") => Some("NetworkInterface"),
        ("resource_type", "regional_nat_gateway") => Some("RegionalNatGateway"),
        ("resource_type", "subnet") => Some("Subnet"),
        ("resource_type", "transit_gateway") => Some("TransitGateway"),
        ("resource_type", "transit_gateway_attachment") => Some("TransitGatewayAttachment"),
        ("resource_type", "vpc") => Some("VPC"),
        ("traffic_type", "accept") => Some("ACCEPT"),
        ("traffic_type", "all") => Some("ALL"),
        ("traffic_type", "reject") => Some("REJECT"),
        _ => None,
    }
}

/// Returns all enum alias entries as (attr_name, alias, canonical) tuples.
pub fn enum_alias_entries() -> &'static [(&'static str, &'static str, &'static str)] {
    &[
        (
            "log_destination_type",
            "cloud_watch_logs",
            "cloud-watch-logs",
        ),
        (
            "log_destination_type",
            "kinesis_data_firehose",
            "kinesis-data-firehose",
        ),
        ("resource_type", "network_interface", "NetworkInterface"),
        (
            "resource_type",
            "regional_nat_gateway",
            "RegionalNatGateway",
        ),
        ("resource_type", "subnet", "Subnet"),
        ("resource_type", "transit_gateway", "TransitGateway"),
        (
            "resource_type",
            "transit_gateway_attachment",
            "TransitGatewayAttachment",
        ),
        ("resource_type", "vpc", "VPC"),
        ("traffic_type", "accept", "ACCEPT"),
        ("traffic_type", "all", "ALL"),
        ("traffic_type", "reject", "REJECT"),
    ]
}
