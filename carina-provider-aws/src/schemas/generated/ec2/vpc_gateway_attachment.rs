//! VpcGatewayAttachment schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.ec2
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, ResourceSchema};

/// Returns the schema config for ec2.VpcGatewayAttachment (Smithy: com.amazonaws.ec2)
pub fn ec2_vpc_gateway_attachment_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::EC2::VPCGatewayAttachment",
        resource_type_name: "ec2.VpcGatewayAttachment",
        has_tags: false,
        schema: ResourceSchema::new("ec2.VpcGatewayAttachment")
            .with_conflicting_replacement()
            .attribute(
                AttributeSchema::optional("internet_gateway_id", super::internet_gateway_id())
                    .create_only()
                    .with_description("The ID of the internet gateway.")
                    .with_provider_name("InternetGatewayId")
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
                AttributeSchema::optional("vpn_gateway_id", super::vpn_gateway_id())
                    .create_only()
                    .with_description("The ID of the VPN gateway.")
                    .with_provider_name("VpnGatewayId")
                    .build(),
            ),
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("ec2.VpcGatewayAttachment", &[])
}
