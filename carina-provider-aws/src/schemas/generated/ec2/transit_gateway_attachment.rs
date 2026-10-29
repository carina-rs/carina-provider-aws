//! TransitGatewayAttachment schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.ec2
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use super::tags_type;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema, StructField};
use carina_provider_protocol::types::ValidatorType;

const VALID_APPLIANCE_MODE_SUPPORT: &[&str] = &["disable", "enable"];

const VALID_DNS_SUPPORT: &[&str] = &["disable", "enable"];

const VALID_IPV6_SUPPORT: &[&str] = &["disable", "enable"];

const VALID_SECURITY_GROUP_REFERENCING_SUPPORT: &[&str] = &["disable", "enable"];

/// Returns the schema config for ec2.TransitGatewayAttachment (Smithy: com.amazonaws.ec2)
pub fn ec2_transit_gateway_attachment_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::EC2::TransitGatewayAttachment",
        resource_type_name: "ec2.TransitGatewayAttachment",
        has_tags: true,
        schema: ResourceSchema::new("ec2.TransitGatewayAttachment")
        .with_description("Describes a VPC attachment.")
        .with_conflicting_replacement()
        .attribute(
            AttributeSchema::optional("options", AttributeType::struct_(
                    "CreateTransitGatewayVpcAttachmentRequestOptions".to_string(),
                    vec![
                    StructField::optional("appliance_mode_support", AttributeType::enum_(
    carina_core::schema::enum_identity("ApplianceModeSupport", Some("aws.ec2.TransitGatewayAttachment.CreateTransitGatewayVpcAttachmentRequestOptions")),
    Some(vec!["disable".to_string(), "enable".to_string()]),
    vec![("disable".to_string(), "disable".to_string()), ("enable".to_string(), "enable".to_string())],
    None,
    None,
)).with_description("Enable or disable support for appliance mode. If enabled, a traffic flow between a source and destination uses the same Availability Zone for the VPC ...").with_provider_name("ApplianceModeSupport").build(),
                    StructField::optional("dns_support", AttributeType::enum_(
    carina_core::schema::enum_identity("DnsSupport", Some("aws.ec2.TransitGatewayAttachment.CreateTransitGatewayVpcAttachmentRequestOptions")),
    Some(vec!["disable".to_string(), "enable".to_string()]),
    vec![("disable".to_string(), "disable".to_string()), ("enable".to_string(), "enable".to_string())],
    None,
    None,
)).with_description("Enable or disable DNS support. The default is enable.").with_provider_name("DnsSupport").build(),
                    StructField::optional("ipv6_support", AttributeType::enum_(
    carina_core::schema::enum_identity("Ipv6Support", Some("aws.ec2.TransitGatewayAttachment.CreateTransitGatewayVpcAttachmentRequestOptions")),
    Some(vec!["disable".to_string(), "enable".to_string()]),
    vec![("disable".to_string(), "disable".to_string()), ("enable".to_string(), "enable".to_string())],
    None,
    None,
)).with_description("Specifies whether IPv6 support is enabled for the attachment. When enabled, the transit gateway network interface receives an IPv6 address. When you e...").with_provider_name("Ipv6Support").build(),
                    StructField::optional("security_group_referencing_support", AttributeType::enum_(
    carina_core::schema::enum_identity("SecurityGroupReferencingSupport", Some("aws.ec2.TransitGatewayAttachment.CreateTransitGatewayVpcAttachmentRequestOptions")),
    Some(vec!["disable".to_string(), "enable".to_string()]),
    vec![("disable".to_string(), "disable".to_string()), ("enable".to_string(), "enable".to_string())],
    None,
    None,
)).with_description("Enables you to reference a security group across VPCs attached to a transit gateway to simplify security group management. This option is set to enabl...").with_provider_name("SecurityGroupReferencingSupport").build()
                    ],
                ))
                .create_only()
                .with_description("The VPC attachment options.")
                .with_provider_name("Options")
                .build(),
        )
        .attribute(
            AttributeSchema::required("subnet_ids", AttributeType::list(super::subnet_id()))
                .create_only()
                .with_description("The IDs of one or more subnets. You can specify only one subnet per Availability Zone. You must specify at least one subnet, but we recommend that you...")
                .with_provider_name("SubnetIds")
                .build(),
        )
        .attribute(
            AttributeSchema::required("transit_gateway_id", super::transit_gateway_id())
                .create_only()
                .with_description("The ID of the transit gateway.")
                .with_provider_name("TransitGatewayId")
                .build(),
        )
        .attribute(
            AttributeSchema::required("vpc_id", super::vpc_id())
                .create_only()
                .with_description("The ID of the VPC.")
                .with_provider_name("VpcId")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("transit_gateway_attachment_id", AttributeType::string())
                .with_description("The ID of the attachment. (read-only)")
                .with_provider_name("TransitGatewayAttachmentId")
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
        "ec2.TransitGatewayAttachment",
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
