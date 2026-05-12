//! InternetGateway schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.ec2
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use super::tags_type;
use super::validate_tags_map;
use carina_core::schema::{AttributeSchema, ResourceSchema};

/// Returns the schema config for ec2.InternetGateway (Smithy: com.amazonaws.ec2)
pub fn ec2_internet_gateway_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::EC2::InternetGateway",
        resource_type_name: "ec2.InternetGateway",
        has_tags: true,
        schema: ResourceSchema::new("ec2.InternetGateway")
        .with_description("Describes an internet gateway.")
        .attribute(
            AttributeSchema::new("vpc_id", super::vpc_id())
                .create_only()
                .with_description("The ID of the VPC to attach the internet gateway to. The provider attaches the IGW after creation and detaches before deletion.")
                .with_provider_name("VpcId"),
        )
        .attribute(
            AttributeSchema::new("internet_gateway_id", super::internet_gateway_id())
                .read_only()
                .with_description("The ID of the internet gateway. (read-only)")
                .with_provider_name("InternetGatewayId"),
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
    ("ec2.InternetGateway", &[])
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
