//! FlowLog schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.ec2
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema, StructField};
use carina_provider_protocol::types::ValidatorType;

const VALID_LOG_DESTINATION_TYPE: &[&str] = &["cloud-watch-logs", "kinesis-data-firehose", "s3"];

const VALID_RESOURCE_TYPE: &[&str] = &[
    "CoreNetwork",
    "CoreNetworkAttachment",
    "CoreNetworkEdge",
    "NetworkInterface",
    "RegionalNatGateway",
    "Subnet",
    "TransitGateway",
    "TransitGatewayAttachment",
    "VPC",
];

const NESTED_ENUM_2: &[&str] = &["auto-scaling-group", "instance", "network-interface"];

const VALID_TRAFFIC_TYPE: &[&str] = &["ACCEPT", "ALL", "REJECT"];

/// Returns the schema config for ec2.FlowLog (Smithy: com.amazonaws.ec2)
pub fn ec2_flow_log_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::EC2::FlowLog",
        resource_type_name: "ec2.FlowLog",
        has_tags: true,
        await_on_read_back: vec![],
        schema: ResourceSchema::new("ec2.FlowLog")
        .with_description("Describes a flow log.")
        .with_conflicting_replacement()
        .attribute(
            AttributeSchema::optional("deliver_logs_permission_arn", carina_aws_types::with_member_identity(AttributeType::string(), carina_aws_types::AwsType::IamRoleArn.definition()))
                .create_only()
                .with_description("The ARN of the IAM role that allows Amazon EC2 to publish flow logs to the log destination. This parameter is required if the destination type is clou...")
                .with_provider_name("DeliverLogsPermissionArn")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("log_destination", carina_aws_types::with_member_identity(AttributeType::string(), carina_aws_types::AwsType::Arn.definition()))
                .create_only()
                .with_description("The destination for the flow log data. The meaning of this parameter depends on the destination type. If the destination type is cloud-watch-logs, spe...")
                .with_provider_name("LogDestination")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("log_destination_type", AttributeType::enum_(carina_core::schema::enum_identity("LogDestinationType", Some("aws.ec2.FlowLog")), Some(vec!["cloud-watch-logs".to_string(), "kinesis-data-firehose".to_string(), "s3".to_string()]), vec![("cloud-watch-logs".to_string(), "cloud_watch_logs".to_string()), ("kinesis-data-firehose".to_string(), "kinesis_data_firehose".to_string()), ("s3".to_string(), "s3".to_string())], None, None))
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
            AttributeSchema::optional("log_group_name", carina_aws_types::with_member_identity(AttributeType::string(), carina_aws_types::AwsType::LogsLogGroupName.definition()))
                .create_only()
                .with_description("The name of a new or existing CloudWatch Logs log group where Amazon EC2 publishes your flow logs. This parameter is valid only if the destination typ...")
                .with_provider_name("LogGroupName")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("max_aggregation_interval", carina_aws_types::with_member_identity(AttributeType::int_member(carina_core::schema::IntIdentityLayer::new(None, None), carina_core::schema::IntMemberConstraints::new(Some((Some(-2147483648), Some(2147483647))))), carina_aws_types::AwsType::Ec2FlowLogMaxAggregationInterval.definition()))
                .create_only()
                .with_description("The maximum interval of time during which a flow of packets is captured and aggregated into a flow log record. The possible values are 60 seconds (1 m...")
                .with_provider_name("MaxAggregationInterval")
                .build(),
        )
        .attribute(
            AttributeSchema::required("resource_ids", AttributeType::refined_list(AttributeType::string(), false, None, carina_core::schema::ListElements::AllowDuplicates))
                .create_only()
                .with_description("The IDs of the resources to monitor. For example, if the resource type is VPC, specify the IDs of the VPCs. Constraints: Maximum of 25 for transit gat...")
                .with_provider_name("ResourceIds")
                .build(),
        )
        .attribute(
            AttributeSchema::required("resource_type", AttributeType::enum_(carina_core::schema::enum_identity("ResourceType", Some("aws.ec2.FlowLog")), Some(vec!["CoreNetwork".to_string(), "CoreNetworkAttachment".to_string(), "CoreNetworkEdge".to_string(), "NetworkInterface".to_string(), "RegionalNatGateway".to_string(), "Subnet".to_string(), "TransitGateway".to_string(), "TransitGatewayAttachment".to_string(), "VPC".to_string()]), vec![("CoreNetwork".to_string(), "core_network".to_string()), ("CoreNetworkAttachment".to_string(), "core_network_attachment".to_string()), ("CoreNetworkEdge".to_string(), "core_network_edge".to_string()), ("NetworkInterface".to_string(), "network_interface".to_string()), ("RegionalNatGateway".to_string(), "regional_nat_gateway".to_string()), ("Subnet".to_string(), "subnet".to_string()), ("TransitGateway".to_string(), "transit_gateway".to_string()), ("TransitGatewayAttachment".to_string(), "transit_gateway_attachment".to_string()), ("VPC".to_string(), "vpc".to_string())], None, None))
                .create_only()
                .with_description("The type of resource to monitor.")
                .with_provider_name("ResourceType")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("tag_field_specifications", AttributeType::refined_list(AttributeType::struct_(
                    "TagFieldSpecificationRequest".to_string(),
                    vec![
                    StructField::optional("resource_type", AttributeType::enum_(carina_core::schema::enum_identity("ResourceType", Some("aws.ec2.FlowLog.TagFieldSpecificationRequest")), Some(vec!["auto-scaling-group".to_string(), "instance".to_string(), "network-interface".to_string()]), vec![("auto-scaling-group".to_string(), "auto_scaling_group".to_string()), ("instance".to_string(), "instance".to_string()), ("network-interface".to_string(), "network_interface".to_string())], None, None)).with_description("The resource type for the tag keys associated with the Flow Logs Amazon EC2 Tags feature fields in your custom log format.").with_provider_name("ResourceType").build(),
                    StructField::optional("tag_keys", AttributeType::refined_list(AttributeType::string_member(carina_core::schema::StringIdentityLayer::new(None, None, None, None), carina_core::schema::StringMemberConstraints::new(None, Some((Some(1), Some(128))))), true, Some((Some(1), Some(2))), carina_core::schema::ListElements::AllowDuplicates)).with_description("The tag keys on your tagged resources to be displayed by the Flow Logs Amazon EC2 Tags feature fields in your custom log format.").with_provider_name("TagKeys").build()
                    ],
                ), true, Some((Some(1), Some(3))), carina_core::schema::ListElements::AllowDuplicates))
                .create_only()
                .with_description("The tag configuration associated with the Flow Logs Amazon EC2 Tags feature fields in your custom log format.")
                .with_provider_name("TagFieldSpecifications")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("traffic_type", AttributeType::enum_(carina_core::schema::enum_identity("TrafficType", Some("aws.ec2.FlowLog")), Some(vec!["ACCEPT".to_string(), "ALL".to_string(), "REJECT".to_string()]), vec![("ACCEPT".to_string(), "accept".to_string()), ("ALL".to_string(), "all".to_string()), ("REJECT".to_string(), "reject".to_string())], None, None))
                .create_only()
                .with_description("The type of traffic to monitor (accepted traffic, rejected traffic, or all traffic). This parameter is not supported for transit gateway or Cloud WAN ...")
                .with_provider_name("TrafficType")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("flow_log_id", carina_aws_types::with_member_identity(AttributeType::string(), carina_aws_types::AwsType::FlowLogId.definition()))
                .with_description("The ID of the flow log. (read-only)")
                .with_provider_name("FlowLogId")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("tags", AttributeType::map(AttributeType::string()))
                .with_description("The tags for the resource.")
                .with_provider_name("Tags")
                .build(),
        )
        .input_rule(carina_core::schema::InputRule::new(carina_core::schema::InputRuleScope::CreateOnly, carina_core::schema::InputRuleTarget::Value(carina_core::schema::InputPath::new("log_format").expect("valid input path")), carina_core::schema::InputRuleCode::new("aws.additional_pattern.pattern_0").expect("valid input rule code"), carina_core::schema::InputPredicate::Matches(carina_core::schema::PatternSpec::new("^\\$\\{[a-z0-9]+(?:-[a-z0-9]+)*\\}(?: +\\$\\{[a-z0-9]+(?:-[a-z0-9]+)*\\})*\\z", carina_core::schema::PatternMatch::Full))))
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
            ("tag_field_specifications[].resource_type", NESTED_ENUM_2),
            ("traffic_type", VALID_TRAFFIC_TYPE),
        ],
    )
}
