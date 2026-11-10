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
        await_on_read_back: vec![],
        schema: ResourceSchema::new("ec2.SubnetRouteTableAssociation")
            .with_description(
                "Describes an association between a route table and a subnet or gateway.",
            )
            .with_conflicting_replacement()
            .attribute(
                AttributeSchema::required(
                    "route_table_id",
                    carina_aws_types::with_member_identity(
                        AttributeType::string(),
                        carina_aws_types::AwsType::RouteTableId.definition(),
                    ),
                )
                .create_only()
                .with_description("The ID of the route table.")
                .with_provider_name("RouteTableId")
                .build(),
            )
            .attribute(
                AttributeSchema::required(
                    "subnet_id",
                    carina_aws_types::with_member_identity(
                        AttributeType::string(),
                        carina_aws_types::AwsType::SubnetId.definition(),
                    ),
                )
                .create_only()
                .with_description("The ID of the subnet.")
                .with_provider_name("SubnetId")
                .build(),
            ),
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("ec2.SubnetRouteTableAssociation", &[])
}
