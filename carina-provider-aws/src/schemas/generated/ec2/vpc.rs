//! Vpc schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.ec2
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use super::tags_type;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema, types};
use carina_provider_protocol::types::ValidatorType;

const VALID_INSTANCE_TENANCY: &[&str] = &["dedicated", "default", "host"];

/// Returns the schema config for ec2.Vpc (Smithy: com.amazonaws.ec2)
pub fn ec2_vpc_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::EC2::VPC",
        resource_type_name: "ec2.Vpc",
        has_tags: true,
        schema: ResourceSchema::new("ec2.Vpc")
        .with_description("Describes a VPC.")
        .with_conflicting_replacement()
        .attribute(
            AttributeSchema::optional("cidr_block", types::ipv4_cidr())
                .create_only()
                .with_description("The IPv4 network range for the VPC, in CIDR notation. For example, 10.0.0.0/16. We modify the specified CIDR block to its canonical form; for example,...")
                .with_provider_name("CidrBlock")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("enable_dns_hostnames", AttributeType::bool())
                .with_description("Indicates whether the instances launched in the VPC get DNS hostnames. If enabled, instances in the VPC get DNS hostnames; otherwise, they do not. You...")
                .with_provider_name("EnableDnsHostnames")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("enable_dns_support", AttributeType::bool())
                .with_description("Indicates whether the DNS resolution is supported for the VPC. If enabled, queries to the Amazon provided DNS server at the 169.254.169.253 IP address...")
                .with_provider_name("EnableDnsSupport")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("instance_tenancy", AttributeType::enum_(
    carina_core::schema::enum_identity("InstanceTenancy", Some("aws.ec2.Vpc")),
    Some(vec!["dedicated".to_string(), "default".to_string(), "host".to_string()]),
    vec![("dedicated".to_string(), "dedicated".to_string()), ("default".to_string(), "default".to_string()), ("host".to_string(), "host".to_string())],
    None,
    None,
))
                .create_only()
                .with_description("The tenancy options for instances launched into the VPC. For default, instances are launched with shared tenancy by default. You can launch instances ...")
                .with_provider_name("InstanceTenancy")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("ipv4_ipam_pool_id", super::ipam_pool_id())
                .create_only()
                .with_description("The ID of an IPv4 IPAM pool you want to use for allocating this VPC's CIDR. For more information, see What is IPAM? in the Amazon VPC IPAM User Guide.")
                .with_provider_name("Ipv4IpamPoolId")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("ipv4_netmask_length", AttributeType::refined_int(
                None,
                Some((Some(0), Some(32))),
            ))
                .create_only()
                .with_description("The netmask length of the IPv4 CIDR you want to allocate to this VPC from an Amazon VPC IP Address Manager (IPAM) pool. For more information about IPA...")
                .with_provider_name("Ipv4NetmaskLength")
                .build(),
        )
        .attribute(
            AttributeSchema::provider_populated("vpc_id", super::vpc_id())
                .with_description("The ID of the VPC. (read-only)")
                .with_provider_name("VpcId")
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
    ("ec2.Vpc", &[("instance_tenancy", VALID_INSTANCE_TENANCY)])
}
