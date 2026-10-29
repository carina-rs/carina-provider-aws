//! EgressOnlyInternetGateway schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.ec2
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use super::tags_type;
use carina_core::schema::{AttributeSchema, ResourceSchema};
use carina_provider_protocol::types::ValidatorType;

/// Returns the schema config for ec2.EgressOnlyInternetGateway (Smithy: com.amazonaws.ec2)
pub fn ec2_egress_only_internet_gateway_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::EC2::EgressOnlyInternetGateway",
        resource_type_name: "ec2.EgressOnlyInternetGateway",
        has_tags: true,
        schema: ResourceSchema::new("ec2.EgressOnlyInternetGateway")
            .with_description("Describes an egress-only internet gateway.")
            .with_coexisting_replacement()
            .attribute(
                AttributeSchema::required("vpc_id", super::vpc_id())
                    .create_only()
                    .with_description(
                        "The ID of the VPC for which to create the egress-only internet gateway.",
                    )
                    .with_provider_name("VpcId")
                    .build(),
            )
            .attribute(
                AttributeSchema::provider_populated(
                    "egress_only_internet_gateway_id",
                    super::egress_only_internet_gateway_id(),
                )
                .with_description("The ID of the egress-only internet gateway. (read-only)")
                .with_provider_name("EgressOnlyInternetGatewayId")
                .build(),
            )
            .attribute(
                AttributeSchema::optional("tags", tags_type())
                    .with_description("The tags for the resource.")
                    .with_provider_name("Tags")
                    .build(),
            )
            .with_declared_validators(vec![ValidatorType::TagsKeyValueCheck]),
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("ec2.EgressOnlyInternetGateway", &[])
}
