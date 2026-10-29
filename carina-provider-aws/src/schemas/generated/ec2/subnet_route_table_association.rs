//! SubnetRouteTableAssociation schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.ec2
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};

/// Returns the schema config for ec2.SubnetRouteTableAssociation (Smithy: com.amazonaws.ec2)
pub fn ec2_subnet_route_table_association_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::EC2::SubnetRouteTableAssociation",
        resource_type_name: "ec2.SubnetRouteTableAssociation",
        has_tags: false,
        schema: ResourceSchema::new("ec2.SubnetRouteTableAssociation")
        .with_description("Describes an association between a route table and a subnet or gateway.")
        .with_conflicting_replacement()
        .attribute(
            AttributeSchema::optional("public_ipv4_pool", AttributeType::string())
                .create_only()
                .with_description("The ID of a public IPv4 pool. A public IPv4 pool is a pool of IPv4 addresses that you've brought to Amazon Web Services with BYOIP.")
                .with_provider_name("PublicIpv4Pool")
                .build(),
        )
        .attribute(
            AttributeSchema::required("route_table_id", super::route_table_id())
                .create_only()
                .with_description("The ID of the route table.")
                .with_provider_name("RouteTableId")
                .build(),
        )
        .attribute(
            AttributeSchema::required("subnet_id", super::subnet_id())
                .create_only()
                .with_description("The ID of the subnet.")
                .with_provider_name("SubnetId")
                .build(),
        )
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("ec2.SubnetRouteTableAssociation", &[])
}
