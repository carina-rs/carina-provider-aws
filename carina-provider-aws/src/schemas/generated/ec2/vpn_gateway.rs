//! VpnGateway schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.ec2
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use super::tags_type;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};
use carina_provider_protocol::types::ValidatorType;

const VALID_TYPE: &[&str] = &["ipsec.1"];

/// Returns the schema config for ec2.VpnGateway (Smithy: com.amazonaws.ec2)
pub fn ec2_vpn_gateway_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::EC2::VPNGateway",
        resource_type_name: "ec2.VpnGateway",
        has_tags: true,
        schema: ResourceSchema::new("ec2.VpnGateway")
        .with_description("Describes a virtual private gateway.")
        .with_coexisting_replacement()
        .attribute(
            AttributeSchema::optional("amazon_side_asn", AttributeType::int())
                .create_only()
                .with_description("A private Autonomous System Number (ASN) for the Amazon side of a BGP session. If you're using a 16-bit ASN, it must be in the 64512 to 65534 range. I...")
                .with_provider_name("AmazonSideAsn")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("availability_zone", super::availability_zone())
                .create_only()
                .with_description("The Availability Zone for the virtual private gateway.")
                .with_provider_name("AvailabilityZone")
                .build(),
        )
        .attribute(
            AttributeSchema::required("type", AttributeType::enum_(
    carina_core::schema::enum_identity("Type", Some("aws.ec2.VpnGateway")),
    Some(vec!["ipsec.1".to_string()]),
    vec![("ipsec.1".to_string(), "ipsec_1".to_string())],
    None,
    None,
))
                .create_only()
                .with_description("The type of VPN connection this virtual private gateway supports.")
                .with_provider_name("Type")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("vpn_gateway_id", super::vpn_gateway_id())
                .with_description("The ID of the virtual private gateway. (read-only)")
                .with_provider_name("VpnGatewayId")
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
    ("ec2.VpnGateway", &[("type", VALID_TYPE)])
}
