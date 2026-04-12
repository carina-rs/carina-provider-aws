//! vpc_peering_connection schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.ec2
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use super::tags_type;
use super::validate_tags_map;
use carina_core::schema::{AttributeSchema, ResourceSchema};

/// Returns the schema config for ec2.vpc_peering_connection (Smithy: com.amazonaws.ec2)
pub fn ec2_vpc_peering_connection_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::EC2::VPCPeeringConnection",
        resource_type_name: "ec2.vpc_peering_connection",
        has_tags: true,
        schema: ResourceSchema::new("aws.ec2.vpc_peering_connection")
        .with_description("Describes a VPC peering connection.")
        .attribute(
            AttributeSchema::new("peer_owner_id", super::aws_account_id())
                .create_only()
                .with_description("The Amazon Web Services account ID of the owner of the accepter VPC. Default: Your Amazon Web Services account ID")
                .with_provider_name("PeerOwnerId"),
        )
        .attribute(
            AttributeSchema::new("peer_vpc_id", super::vpc_id())
                .required()
                .create_only()
                .with_description("The ID of the VPC with which you are creating the VPC peering connection. You must specify this parameter in the request.")
                .with_provider_name("PeerVpcId"),
        )
        .attribute(
            AttributeSchema::new("vpc_id", super::vpc_id())
                .required()
                .create_only()
                .with_description("The ID of the requester VPC. You must specify this parameter in the request.")
                .with_provider_name("VpcId"),
        )
        .attribute(
            AttributeSchema::new("vpc_peering_connection_id", super::vpc_peering_connection_id())
                .with_description("The ID of the VPC peering connection. (read-only)")
                .with_provider_name("VpcPeeringConnectionId"),
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
    ("ec2.vpc_peering_connection", &[])
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
