//! transit_gateway_attachment schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.ec2
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use super::tags_type;
use super::validate_tags_map;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema, StructField};

const VALID_APPLIANCE_MODE_SUPPORT: &[&str] = &["disable", "enable"];

const VALID_DNS_SUPPORT: &[&str] = &["disable", "enable"];

const VALID_IPV6_SUPPORT: &[&str] = &["disable", "enable"];

const VALID_SECURITY_GROUP_REFERENCING_SUPPORT: &[&str] = &["disable", "enable"];

/// Returns the schema config for ec2.transit_gateway_attachment (Smithy: com.amazonaws.ec2)
pub fn ec2_transit_gateway_attachment_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::EC2::TransitGatewayAttachment",
        resource_type_name: "ec2.transit_gateway_attachment",
        has_tags: true,
        schema: ResourceSchema::new("aws.ec2.transit_gateway_attachment")
        .with_description("Describes a VPC attachment.")
        .attribute(
            AttributeSchema::new("options", AttributeType::Struct {
                    name: "CreateTransitGatewayVpcAttachmentRequestOptions".to_string(),
                    fields: vec![
                    StructField::new("appliance_mode_support", AttributeType::StringEnum {
                name: "ApplianceModeSupport".to_string(),
                values: vec!["disable".to_string(), "enable".to_string()],
                namespace: Some("aws.ec2.transit_gateway_attachment".to_string()),
                to_dsl: None,
            }).with_description("Enable or disable support for appliance mode. If enabled, a traffic flow between a source and destination uses the same Availability Zone for the VPC ...").with_provider_name("ApplianceModeSupport"),
                    StructField::new("dns_support", AttributeType::StringEnum {
                name: "DnsSupport".to_string(),
                values: vec!["disable".to_string(), "enable".to_string()],
                namespace: Some("aws.ec2.transit_gateway_attachment".to_string()),
                to_dsl: None,
            }).with_description("Enable or disable DNS support. The default is enable.").with_provider_name("DnsSupport"),
                    StructField::new("ipv6_support", AttributeType::StringEnum {
                name: "Ipv6Support".to_string(),
                values: vec!["disable".to_string(), "enable".to_string()],
                namespace: Some("aws.ec2.transit_gateway_attachment".to_string()),
                to_dsl: None,
            }).with_description("Enable or disable IPv6 support. The default is disable.").with_provider_name("Ipv6Support"),
                    StructField::new("security_group_referencing_support", AttributeType::StringEnum {
                name: "SecurityGroupReferencingSupport".to_string(),
                values: vec!["disable".to_string(), "enable".to_string()],
                namespace: Some("aws.ec2.transit_gateway_attachment".to_string()),
                to_dsl: None,
            }).with_description("Enables you to reference a security group across VPCs attached to a transit gateway to simplify security group management. This option is set to enabl...").with_provider_name("SecurityGroupReferencingSupport")
                    ],
                })
                .create_only()
                .with_description("The VPC attachment options.")
                .with_provider_name("Options"),
        )
        .attribute(
            AttributeSchema::new("subnet_ids", AttributeType::list(super::subnet_id()))
                .required()
                .create_only()
                .with_description("The IDs of one or more subnets. You can specify only one subnet per Availability Zone. You must specify at least one subnet, but we recommend that you...")
                .with_provider_name("SubnetIds"),
        )
        .attribute(
            AttributeSchema::new("transit_gateway_id", super::transit_gateway_id())
                .required()
                .create_only()
                .with_description("The ID of the transit gateway.")
                .with_provider_name("TransitGatewayId"),
        )
        .attribute(
            AttributeSchema::new("vpc_id", super::vpc_id())
                .required()
                .create_only()
                .with_description("The ID of the VPC.")
                .with_provider_name("VpcId"),
        )
        .attribute(
            AttributeSchema::new("transit_gateway_attachment_id", AttributeType::String)
                .with_description("The ID of the attachment. (read-only)")
                .with_provider_name("TransitGatewayAttachmentId"),
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
        "ec2.transit_gateway_attachment",
        &[
            ("appliance_mode_support", VALID_APPLIANCE_MODE_SUPPORT),
            ("dns_support", VALID_DNS_SUPPORT),
            ("ipv6_support", VALID_IPV6_SUPPORT),
            (
                "security_group_referencing_support",
                VALID_SECURITY_GROUP_REFERENCING_SUPPORT,
            ),
        ],
    )
}

/// Maps DSL alias values back to canonical AWS values for this module.
/// e.g., ("ip_protocol", "all") -> Some("-1")
pub fn enum_alias_reverse(attr_name: &str, value: &str) -> Option<&'static str> {
    let _ = (attr_name, value);
    None
}
