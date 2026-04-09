//! transit_gateway schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.ec2
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use super::tags_type;
use super::validate_tags_map;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema, StructField, types};

const VALID_AUTO_ACCEPT_SHARED_ATTACHMENTS: &[&str] = &["disable", "enable"];

const VALID_DEFAULT_ROUTE_TABLE_ASSOCIATION: &[&str] = &["disable", "enable"];

const VALID_DEFAULT_ROUTE_TABLE_PROPAGATION: &[&str] = &["disable", "enable"];

const VALID_DNS_SUPPORT: &[&str] = &["disable", "enable"];

const VALID_MULTICAST_SUPPORT: &[&str] = &["disable", "enable"];

const VALID_SECURITY_GROUP_REFERENCING_SUPPORT: &[&str] = &["disable", "enable"];

const VALID_VPN_ECMP_SUPPORT: &[&str] = &["disable", "enable"];

/// Returns the schema config for ec2.transit_gateway (Smithy: com.amazonaws.ec2)
pub fn ec2_transit_gateway_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::EC2::TransitGateway",
        resource_type_name: "ec2.transit_gateway",
        has_tags: true,
        schema: ResourceSchema::new("aws.ec2.transit_gateway")
        .with_description("Describes a transit gateway.")
        .attribute(
            AttributeSchema::new("description", AttributeType::String)
                .with_description("A description of the transit gateway.")
                .with_provider_name("Description"),
        )
        .attribute(
            AttributeSchema::new("options", AttributeType::Struct {
                    name: "TransitGatewayRequestOptions".to_string(),
                    fields: vec![
                    StructField::new("amazon_side_asn", AttributeType::Int).with_description("A private Autonomous System Number (ASN) for the Amazon side of a BGP session. The range is 64512 to 65534 for 16-bit ASNs and 4200000000 to 429496729...").with_provider_name("AmazonSideAsn"),
                    StructField::new("auto_accept_shared_attachments", AttributeType::StringEnum {
                name: "AutoAcceptSharedAttachments".to_string(),
                values: vec!["disable".to_string(), "enable".to_string()],
                namespace: Some("aws.ec2.transit_gateway".to_string()),
                to_dsl: None,
            }).with_description("Enable or disable automatic acceptance of attachment requests. Disabled by default.").with_provider_name("AutoAcceptSharedAttachments"),
                    StructField::new("default_route_table_association", AttributeType::StringEnum {
                name: "DefaultRouteTableAssociation".to_string(),
                values: vec!["disable".to_string(), "enable".to_string()],
                namespace: Some("aws.ec2.transit_gateway".to_string()),
                to_dsl: None,
            }).with_description("Enable or disable automatic association with the default association route table. Enabled by default.").with_provider_name("DefaultRouteTableAssociation"),
                    StructField::new("default_route_table_propagation", AttributeType::StringEnum {
                name: "DefaultRouteTablePropagation".to_string(),
                values: vec!["disable".to_string(), "enable".to_string()],
                namespace: Some("aws.ec2.transit_gateway".to_string()),
                to_dsl: None,
            }).with_description("Enable or disable automatic propagation of routes to the default propagation route table. Enabled by default.").with_provider_name("DefaultRouteTablePropagation"),
                    StructField::new("dns_support", AttributeType::StringEnum {
                name: "DnsSupport".to_string(),
                values: vec!["disable".to_string(), "enable".to_string()],
                namespace: Some("aws.ec2.transit_gateway".to_string()),
                to_dsl: None,
            }).with_description("Enable or disable DNS support. Enabled by default.").with_provider_name("DnsSupport"),
                    StructField::new("multicast_support", AttributeType::StringEnum {
                name: "MulticastSupport".to_string(),
                values: vec!["disable".to_string(), "enable".to_string()],
                namespace: Some("aws.ec2.transit_gateway".to_string()),
                to_dsl: None,
            }).with_description("Indicates whether multicast is enabled on the transit gateway").with_provider_name("MulticastSupport"),
                    StructField::new("security_group_referencing_support", AttributeType::StringEnum {
                name: "SecurityGroupReferencingSupport".to_string(),
                values: vec!["disable".to_string(), "enable".to_string()],
                namespace: Some("aws.ec2.transit_gateway".to_string()),
                to_dsl: None,
            }).with_description("Enables you to reference a security group across VPCs attached to a transit gateway to simplify security group management. This option is disabled by ...").with_provider_name("SecurityGroupReferencingSupport"),
                    StructField::new("transit_gateway_cidr_blocks", AttributeType::list(types::ipv4_cidr())).with_description("One or more IPv4 or IPv6 CIDR blocks for the transit gateway. Must be a size /24 CIDR block or larger for IPv4, or a size /64 CIDR block or larger for...").with_provider_name("TransitGatewayCidrBlocks"),
                    StructField::new("vpn_ecmp_support", AttributeType::StringEnum {
                name: "VpnEcmpSupport".to_string(),
                values: vec!["disable".to_string(), "enable".to_string()],
                namespace: Some("aws.ec2.transit_gateway".to_string()),
                to_dsl: None,
            }).with_description("Enable or disable Equal Cost Multipath Protocol support. Enabled by default.").with_provider_name("VpnEcmpSupport")
                    ],
                })
                .create_only()
                .with_description("The transit gateway options.")
                .with_provider_name("Options"),
        )
        .attribute(
            AttributeSchema::new("transit_gateway_id", super::transit_gateway_id())
                .with_description("The ID of the transit gateway. (read-only)")
                .with_provider_name("TransitGatewayId"),
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
        "ec2.transit_gateway",
        &[
            (
                "auto_accept_shared_attachments",
                VALID_AUTO_ACCEPT_SHARED_ATTACHMENTS,
            ),
            (
                "default_route_table_association",
                VALID_DEFAULT_ROUTE_TABLE_ASSOCIATION,
            ),
            (
                "default_route_table_propagation",
                VALID_DEFAULT_ROUTE_TABLE_PROPAGATION,
            ),
            ("dns_support", VALID_DNS_SUPPORT),
            ("multicast_support", VALID_MULTICAST_SUPPORT),
            (
                "security_group_referencing_support",
                VALID_SECURITY_GROUP_REFERENCING_SUPPORT,
            ),
            ("vpn_ecmp_support", VALID_VPN_ECMP_SUPPORT),
        ],
    )
}

/// Maps DSL alias values back to canonical AWS values for this module.
/// e.g., ("ip_protocol", "all") -> Some("-1")
pub fn enum_alias_reverse(attr_name: &str, value: &str) -> Option<&'static str> {
    let _ = (attr_name, value);
    None
}
