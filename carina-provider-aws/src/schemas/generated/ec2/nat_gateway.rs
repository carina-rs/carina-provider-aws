//! nat_gateway schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.ec2
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use super::tags_type;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema, StructField, types};

const VALID_AVAILABILITY_MODE: &[&str] = &["regional", "zonal"];

const VALID_CONNECTIVITY_TYPE: &[&str] = &["private", "public"];

/// Returns the schema config for ec2.nat_gateway (Smithy: com.amazonaws.ec2)
pub fn ec2_nat_gateway_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "UNKNOWN",
        resource_type_name: "ec2.nat_gateway",
        has_tags: true,
        schema: ResourceSchema::new("aws.ec2.nat_gateway")
        .with_description("Describes a NAT gateway.")
        .attribute(
            AttributeSchema::new("allocation_id", super::allocation_id())
                .create_only()
                .with_description("[Public NAT gateways only] The allocation ID of an Elastic IP address to associate with the NAT gateway. You cannot specify an Elastic IP address with...")
                .with_provider_name("AllocationId"),
        )
        .attribute(
            AttributeSchema::new("availability_mode", AttributeType::StringEnum {
                name: "AvailabilityMode".to_string(),
                values: vec!["regional".to_string(), "zonal".to_string()],
                namespace: Some("aws.ec2.nat_gateway".to_string()),
                to_dsl: None,
            })
                .create_only()
                .with_description("Specifies whether to create a zonal (single-AZ) or regional (multi-AZ) NAT gateway. Defaults to zonal. A zonal NAT gateway is a NAT Gateway that provi...")
                .with_provider_name("AvailabilityMode"),
        )
        .attribute(
            AttributeSchema::new("availability_zone_addresses", AttributeType::list(AttributeType::Struct {
                    name: "AvailabilityZoneAddress".to_string(),
                    fields: vec![
                    StructField::new("allocation_ids", AttributeType::list(super::allocation_id())).with_description("The allocation IDs of the Elastic IP addresses (EIPs) to be used for handling outbound NAT traffic in this specific Availability Zone.").with_provider_name("AllocationIds"),
                    StructField::new("availability_zone", super::availability_zone()).with_description("For regional NAT gateways only: The Availability Zone where this specific NAT gateway configuration will be active. Each AZ in a regional NAT gateway ...").with_provider_name("AvailabilityZone"),
                    StructField::new("availability_zone_id", super::availability_zone_id()).with_description("For regional NAT gateways only: The ID of the Availability Zone where this specific NAT gateway configuration will be active. Each AZ in a regional NA...").with_provider_name("AvailabilityZoneId")
                    ],
                }))
                .create_only()
                .with_description("For regional NAT gateways only: Specifies which Availability Zones you want the NAT gateway to support and the Elastic IP addresses (EIPs) to use in e...")
                .with_provider_name("AvailabilityZoneAddresses"),
        )
        .attribute(
            AttributeSchema::new("connectivity_type", AttributeType::StringEnum {
                name: "ConnectivityType".to_string(),
                values: vec!["private".to_string(), "public".to_string()],
                namespace: Some("aws.ec2.nat_gateway".to_string()),
                to_dsl: None,
            })
                .create_only()
                .with_description("Indicates whether the NAT gateway supports public or private connectivity. The default is public connectivity.")
                .with_provider_name("ConnectivityType"),
        )
        .attribute(
            AttributeSchema::new("private_ip_address", types::ipv4_address())
                .create_only()
                .with_description("The private IPv4 address to assign to the NAT gateway. If you don't provide an address, a private IPv4 address will be automatically assigned.")
                .with_provider_name("PrivateIpAddress"),
        )
        .attribute(
            AttributeSchema::new("subnet_id", super::subnet_id())
                .required()
                .create_only()
                .with_description("The ID of the subnet in which to create the NAT gateway.")
                .with_provider_name("SubnetId"),
        )
        .attribute(
            AttributeSchema::new("vpc_id", super::vpc_id())
                .create_only()
                .with_description("The ID of the VPC where you want to create a regional NAT gateway.")
                .with_provider_name("VpcId"),
        )
        .attribute(
            AttributeSchema::new("nat_gateway_id", super::nat_gateway_id())
                .with_description("The ID of the NAT gateway. (read-only)")
                .with_provider_name("NatGatewayId"),
        )
        .attribute(
            AttributeSchema::new("tags", tags_type())
                .with_description("The tags for the resource.")
                .with_provider_name("Tags"),
        )
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    (
        "ec2.nat_gateway",
        &[
            ("availability_mode", VALID_AVAILABILITY_MODE),
            ("connectivity_type", VALID_CONNECTIVITY_TYPE),
        ],
    )
}

/// Maps DSL alias values back to canonical AWS values for this module.
/// e.g., ("ip_protocol", "all") -> Some("-1")
pub fn enum_alias_reverse(attr_name: &str, value: &str) -> Option<&'static str> {
    let _ = (attr_name, value);
    None
}
