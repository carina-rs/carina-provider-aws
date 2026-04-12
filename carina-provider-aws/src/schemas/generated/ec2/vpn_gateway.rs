//! vpn_gateway schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.ec2
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use super::tags_type;
use super::validate_tags_map;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};

const VALID_TYPE: &[&str] = &["ipsec.1"];

/// Returns the schema config for ec2.vpn_gateway (Smithy: com.amazonaws.ec2)
pub fn ec2_vpn_gateway_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::EC2::VPNGateway",
        resource_type_name: "ec2.vpn_gateway",
        has_tags: true,
        schema: ResourceSchema::new("aws.ec2.vpn_gateway")
        .with_description("Describes a virtual private gateway.")
        .attribute(
            AttributeSchema::new("amazon_side_asn", AttributeType::Int)
                .create_only()
                .with_description("A private Autonomous System Number (ASN) for the Amazon side of a BGP session. If you're using a 16-bit ASN, it must be in the 64512 to 65534 range. I...")
                .with_provider_name("AmazonSideAsn"),
        )
        .attribute(
            AttributeSchema::new("availability_zone", super::availability_zone())
                .create_only()
                .with_description("The Availability Zone for the virtual private gateway.")
                .with_provider_name("AvailabilityZone"),
        )
        .attribute(
            AttributeSchema::new("type", AttributeType::StringEnum {
                name: "Type".to_string(),
                values: vec!["ipsec.1".to_string()],
                namespace: Some("aws.ec2.vpn_gateway".to_string()),
                to_dsl: None,
            })
                .required()
                .create_only()
                .with_description("The type of VPN connection this virtual private gateway supports.")
                .with_provider_name("Type"),
        )
        .attribute(
            AttributeSchema::new("vpn_gateway_id", super::vpn_gateway_id())
                .with_description("The ID of the virtual private gateway. (read-only)")
                .with_provider_name("VpnGatewayId"),
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
    ("ec2.vpn_gateway", &[("type", VALID_TYPE)])
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
