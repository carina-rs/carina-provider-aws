//! route_table schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.ec2
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use super::tags_type;
use super::validate_tags_map;
use carina_core::schema::{AttributeSchema, ResourceSchema};

/// Returns the schema config for ec2.route_table (Smithy: com.amazonaws.ec2)
pub fn ec2_route_table_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::EC2::RouteTable",
        resource_type_name: "ec2.route_table",
        has_tags: true,
        schema: ResourceSchema::new("aws.ec2.route_table")
            .with_description("Describes a route table.")
            .attribute(
                AttributeSchema::new("vpc_id", super::vpc_id())
                    .required()
                    .create_only()
                    .with_description("The ID of the VPC.")
                    .with_provider_name("VpcId"),
            )
            .attribute(
                AttributeSchema::new("route_table_id", super::route_table_id())
                    .with_description("The ID of the route table. (read-only)")
                    .with_provider_name("RouteTableId"),
            )
            .attribute(
                AttributeSchema::new("tags", tags_type())
                    .with_description("The tags for the resource.")
                    .with_provider_name("Tags"),
            )
            .with_validator(validate_tags_map),
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    ("ec2.route_table", &[])
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
