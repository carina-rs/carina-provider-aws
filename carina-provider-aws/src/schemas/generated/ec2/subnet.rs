//! Subnet schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.ec2
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use super::tags_type;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema, StructField, types};
use carina_provider_protocol::types::ValidatorType;

/// Returns the schema config for ec2.Subnet (Smithy: com.amazonaws.ec2)
pub fn ec2_subnet_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::EC2::Subnet",
        resource_type_name: "ec2.Subnet",
        has_tags: true,
        schema: ResourceSchema::new("ec2.Subnet")
        .with_description("Describes a subnet.")
        .with_conflicting_replacement()
        .attribute(
            AttributeSchema::optional("assign_ipv6_address_on_creation", AttributeType::bool())
                .with_description("Indicates whether a network interface created in this subnet (including a network interface created by RunInstances) receives an IPv6 address.")
                .with_provider_name("AssignIpv6AddressOnCreation")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("availability_zone", super::availability_zone())
                .create_only()
                .with_description("The Availability Zone or Local Zone for the subnet. Default: Amazon Web Services selects one for you. If you create more than one subnet in your VPC, ...")
                .with_provider_name("AvailabilityZone")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("availability_zone_id", super::availability_zone_id())
                .create_only()
                .with_description("The AZ ID or the Local Zone ID of the subnet.")
                .with_provider_name("AvailabilityZoneId")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("cidr_block", types::ipv4_cidr())
                .create_only()
                .with_description("The IPv4 network range for the subnet, in CIDR notation. For example, 10.0.0.0/24. We modify the specified CIDR block to its canonical form; for examp...")
                .with_provider_name("CidrBlock")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("enable_dns64", AttributeType::bool())
                .with_description("Indicates whether DNS queries made to the Amazon-provided DNS Resolver in this subnet should return synthetic IPv6 addresses for IPv4-only destination...")
                .with_provider_name("EnableDns64")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("enable_lni_at_device_index", AttributeType::int())
                .with_description("Indicates the device position for local network interfaces in this subnet. For example, 1 indicates local network interfaces in this subnet are the se...")
                .with_provider_name("EnableLniAtDeviceIndex")
                .build(),
        )
        // Read contract: CreateSubnet accepts the IPv4 IPAM pool ID, but DescribeSubnets returns only allocated CIDRs
        .attribute(
            AttributeSchema::optional("ipv4_ipam_pool_id", super::ipam_pool_id())
                .create_only()
                .write_only()
                .with_description("An IPv4 IPAM pool ID for the subnet.")
                .with_provider_name("Ipv4IpamPoolId")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("ipv4_netmask_length", AttributeType::refined_int(
                None,
                Some((Some(0), Some(32))),
            ))
                .create_only()
                .with_description("An IPv4 netmask length for the subnet.")
                .with_provider_name("Ipv4NetmaskLength")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("ipv6_cidr_block", types::ipv6_cidr())
                .create_only()
                .with_description("The IPv6 network range for the subnet, in CIDR notation. This parameter is required for an IPv6 only subnet.")
                .with_provider_name("Ipv6CidrBlock")
                .build(),
        )
        // Read contract: CreateSubnet accepts the IPv6 IPAM pool ID, but DescribeSubnets returns only allocated CIDRs
        .attribute(
            AttributeSchema::optional("ipv6_ipam_pool_id", super::ipam_pool_id())
                .create_only()
                .write_only()
                .with_description("An IPv6 IPAM pool ID for the subnet.")
                .with_provider_name("Ipv6IpamPoolId")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("ipv6_native", AttributeType::bool())
                .create_only()
                .with_description("Indicates whether to create an IPv6 only subnet.")
                .with_provider_name("Ipv6Native")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("ipv6_netmask_length", AttributeType::refined_int(
                None,
                Some((Some(0), Some(128))),
            ))
                .create_only()
                .with_description("An IPv6 netmask length for the subnet.")
                .with_provider_name("Ipv6NetmaskLength")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("map_public_ip_on_launch", AttributeType::bool())
                .with_description("Indicates whether instances launched in this subnet receive a public IPv4 address. Amazon Web Services charges for all public IPv4 addresses, includin...")
                .with_provider_name("MapPublicIpOnLaunch")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("outpost_arn", super::arn())
                .create_only()
                .with_description("The Amazon Resource Name (ARN) of the Outpost. If you specify an Outpost ARN, you must also specify the Availability Zone of the Outpost subnet.")
                .with_provider_name("OutpostArn")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("private_dns_name_options_on_launch", AttributeType::struct_(
                    "PrivateDnsNameOptionsOnLaunch".to_string(),
                    vec![
                    StructField::optional("enable_resource_name_dns_aaaa_record", AttributeType::bool()).with_description("Indicates whether to respond to DNS queries for instance hostname with DNS AAAA records.").with_provider_name("EnableResourceNameDnsAAAARecord").build(),
                    StructField::optional("enable_resource_name_dns_a_record", AttributeType::bool()).with_description("Indicates whether to respond to DNS queries for instance hostnames with DNS A records.").with_provider_name("EnableResourceNameDnsARecord").build(),
                    StructField::optional("hostname_type", AttributeType::enum_(
    carina_core::schema::enum_identity("HostnameType", Some("aws.ec2.Subnet.PrivateDnsNameOptionsOnLaunch")),
    Some(vec!["ip-name".to_string(), "resource-name".to_string()]),
    vec![("ip-name".to_string(), "ip_name".to_string()), ("resource-name".to_string(), "resource_name".to_string())],
    None,
    None,
)).with_description("The type of hostname for EC2 instances. For IPv4 only subnets, an instance DNS name must be based on the instance IPv4 address. For IPv6 only subnets,...").with_provider_name("HostnameType").build()
                    ],
                ))
                .with_description("The type of hostnames to assign to instances in the subnet at launch. An instance hostname is based on the IPv4 address or ID of the instance.")
                .with_provider_name("PrivateDnsNameOptionsOnLaunch")
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
            AttributeSchema::provider_populated("subnet_id", super::subnet_id())
                .with_description("The ID of the subnet. (read-only)")
                .with_provider_name("SubnetId")
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
    ("ec2.Subnet", &[])
}
