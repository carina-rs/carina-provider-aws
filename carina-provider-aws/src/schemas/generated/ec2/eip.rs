//! Eip schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.ec2
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use super::tags_type;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema, types};
use carina_provider_protocol::types::ValidatorType;

const VALID_DOMAIN: &[&str] = &["standard", "vpc"];

/// Returns the schema config for ec2.Eip (Smithy: com.amazonaws.ec2)
pub fn ec2_eip_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::EC2::EIP",
        resource_type_name: "ec2.Eip",
        has_tags: true,
        schema: ResourceSchema::new("ec2.Eip")
        .with_description("Describes an Elastic IP address, or a carrier IP address.")
        .with_conflicting_replacement()
        .attribute(
            AttributeSchema::optional("address", AttributeType::string())
                .create_only()
                .with_description("The Elastic IP address to recover or an IPv4 address from an address pool.")
                .with_provider_name("Address")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("domain", AttributeType::enum_(
    carina_core::schema::enum_identity("Domain", Some("aws.ec2.Eip")),
    Some(vec!["standard".to_string(), "vpc".to_string()]),
    vec![("standard".to_string(), "standard".to_string()), ("vpc".to_string(), "vpc".to_string())],
    None,
    None,
))
                .create_only()
                .with_description("The network (vpc).")
                .with_provider_name("Domain")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("public_ipv4_pool", AttributeType::string())
                .create_only()
                .with_description("The ID of an address pool that you own. Use this parameter to let Amazon EC2 select an address from the address pool. To specify a specific address fr...")
                .with_provider_name("PublicIpv4Pool")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("allocation_id", super::allocation_id())
                .with_description("The ID representing the allocation of the address. (read-only)")
                .with_provider_name("AllocationId")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("public_ip", types::ipv4_address())
                .with_description("The Elastic IP address. (read-only)")
                .with_provider_name("PublicIp")
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
    ("ec2.Eip", &[("domain", VALID_DOMAIN)])
}
