//! TransitGateway schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.ec2
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use super::tags_type;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema, StructField, types};
use carina_provider_protocol::types::ValidatorType;

/// Returns the schema config for ec2.TransitGateway (Smithy: com.amazonaws.ec2)
pub fn ec2_transit_gateway_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::EC2::TransitGateway",
        resource_type_name: "ec2.TransitGateway",
        has_tags: true,
        schema: ResourceSchema::new("ec2.TransitGateway")
        .with_description("Describes a transit gateway.")
        .with_coexisting_replacement()
        .attribute(
            AttributeSchema::optional("description", AttributeType::string())
                .with_description("A description of the transit gateway.")
                .with_provider_name("Description")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("options", AttributeType::struct_(
                    "TransitGatewayRequestOptions".to_string(),
                    vec![
                    StructField::optional("amazon_side_asn", AttributeType::int()).with_description("A private Autonomous System Number (ASN) for the Amazon side of a BGP session. The range is 64512 to 65534 for 16-bit ASNs and 4200000000 to 429496729...").with_provider_name("AmazonSideAsn").build(),
                    StructField::optional("auto_accept_shared_attachments", AttributeType::enum_(
    carina_core::schema::enum_identity("AutoAcceptSharedAttachments", Some("aws.ec2.TransitGateway.TransitGatewayRequestOptions")),
    Some(vec!["disable".to_string(), "enable".to_string()]),
    vec![("disable".to_string(), "disable".to_string()), ("enable".to_string(), "enable".to_string())],
    None,
    None,
)).with_description("Enable or disable automatic acceptance of attachment requests. Disabled by default.").with_provider_name("AutoAcceptSharedAttachments").build(),
                    StructField::optional("default_route_table_association", AttributeType::enum_(
    carina_core::schema::enum_identity("DefaultRouteTableAssociation", Some("aws.ec2.TransitGateway.TransitGatewayRequestOptions")),
    Some(vec!["disable".to_string(), "enable".to_string()]),
    vec![("disable".to_string(), "disable".to_string()), ("enable".to_string(), "enable".to_string())],
    None,
    None,
)).with_description("Enable or disable automatic association with the default association route table. Enabled by default.").with_provider_name("DefaultRouteTableAssociation").build(),
                    StructField::optional("default_route_table_propagation", AttributeType::enum_(
    carina_core::schema::enum_identity("DefaultRouteTablePropagation", Some("aws.ec2.TransitGateway.TransitGatewayRequestOptions")),
    Some(vec!["disable".to_string(), "enable".to_string()]),
    vec![("disable".to_string(), "disable".to_string()), ("enable".to_string(), "enable".to_string())],
    None,
    None,
)).with_description("Enable or disable automatic propagation of routes to the default propagation route table. Enabled by default.").with_provider_name("DefaultRouteTablePropagation").build(),
                    StructField::optional("dns_support", AttributeType::enum_(
    carina_core::schema::enum_identity("DnsSupport", Some("aws.ec2.TransitGateway.TransitGatewayRequestOptions")),
    Some(vec!["disable".to_string(), "enable".to_string()]),
    vec![("disable".to_string(), "disable".to_string()), ("enable".to_string(), "enable".to_string())],
    None,
    None,
)).with_description("Enable or disable DNS support. Enabled by default.").with_provider_name("DnsSupport").build(),
                    StructField::optional("multicast_support", AttributeType::enum_(
    carina_core::schema::enum_identity("MulticastSupport", Some("aws.ec2.TransitGateway.TransitGatewayRequestOptions")),
    Some(vec!["disable".to_string(), "enable".to_string()]),
    vec![("disable".to_string(), "disable".to_string()), ("enable".to_string(), "enable".to_string())],
    None,
    None,
)).with_description("Indicates whether multicast is enabled on the transit gateway").with_provider_name("MulticastSupport").build(),
                    StructField::optional("security_group_referencing_support", AttributeType::enum_(
    carina_core::schema::enum_identity("SecurityGroupReferencingSupport", Some("aws.ec2.TransitGateway.TransitGatewayRequestOptions")),
    Some(vec!["disable".to_string(), "enable".to_string()]),
    vec![("disable".to_string(), "disable".to_string()), ("enable".to_string(), "enable".to_string())],
    None,
    None,
)).with_description("Enables you to reference a security group across VPCs attached to a transit gateway to simplify security group management. This option is disabled by ...").with_provider_name("SecurityGroupReferencingSupport").build(),
                    StructField::optional("transit_gateway_cidr_blocks", AttributeType::list(types::ipv4_cidr())).with_description("One or more IPv4 or IPv6 CIDR blocks for the transit gateway. Must be a size /24 CIDR block or larger for IPv4, or a size /64 CIDR block or larger for...").with_provider_name("TransitGatewayCidrBlocks").build(),
                    StructField::optional("vpn_ecmp_support", AttributeType::enum_(
    carina_core::schema::enum_identity("VpnEcmpSupport", Some("aws.ec2.TransitGateway.TransitGatewayRequestOptions")),
    Some(vec!["disable".to_string(), "enable".to_string()]),
    vec![("disable".to_string(), "disable".to_string()), ("enable".to_string(), "enable".to_string())],
    None,
    None,
)).with_description("Enable or disable Equal Cost Multipath Protocol support. Enabled by default.").with_provider_name("VpnEcmpSupport").build()
                    ],
                ))
                .create_only()
                .with_description("The transit gateway options.")
                .with_provider_name("Options")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("transit_gateway_id", super::transit_gateway_id())
                .with_description("The ID of the transit gateway. (read-only)")
                .with_provider_name("TransitGatewayId")
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
    ("ec2.TransitGateway", &[])
}
