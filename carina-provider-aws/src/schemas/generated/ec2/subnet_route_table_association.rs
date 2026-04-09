//! subnet_route_table_association schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.ec2
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};

/// Returns the schema config for ec2.subnet_route_table_association (Smithy: com.amazonaws.ec2)
pub fn ec2_subnet_route_table_association_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "UNKNOWN",
        resource_type_name: "ec2.subnet_route_table_association",
        has_tags: false,
        schema: ResourceSchema::new("aws.ec2.subnet_route_table_association")
        .with_description("Describes an association between a route table and a subnet or gateway.")
        .attribute(
            AttributeSchema::new("public_ipv4_pool", AttributeType::String)
                .create_only()
                .with_description("The ID of a public IPv4 pool. A public IPv4 pool is a pool of IPv4 addresses that you've brought to Amazon Web Services with BYOIP.")
                .with_provider_name("PublicIpv4Pool"),
        )
        .attribute(
            AttributeSchema::new("route_table_id", super::route_table_id())
                .required()
                .create_only()
                .with_description("The ID of the route table.")
                .with_provider_name("RouteTableId"),
        )
        .attribute(
            AttributeSchema::new("subnet_id", super::subnet_id())
                .required()
                .create_only()
                .with_description("The ID of the subnet.")
                .with_provider_name("SubnetId"),
        )
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("ec2.subnet_route_table_association", &[])
}

/// Maps DSL alias values back to canonical AWS values for this module.
/// e.g., ("ip_protocol", "all") -> Some("-1")
pub fn enum_alias_reverse(attr_name: &str, value: &str) -> Option<&'static str> {
    let _ = (attr_name, value);
    None
}
