//! SecurityGroupEgress schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.ec2
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema, types};

const VALID_IP_PROTOCOL: &[&str] = &["tcp", "udp", "icmp", "icmpv6", "-1"];

/// Returns the schema config for ec2.SecurityGroupEgress (Smithy: com.amazonaws.ec2)
pub fn ec2_security_group_egress_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::EC2::SecurityGroupEgress",
        resource_type_name: "ec2.SecurityGroupEgress",
        has_tags: false,
        await_on_read_back: vec![],
        schema: ResourceSchema::new("ec2.SecurityGroupEgress")
            .with_description("Describes a security group rule.")
            .with_conflicting_replacement()
            .attribute(
                AttributeSchema::optional(
                    "cidr_ip",
                    carina_aws_types::with_member_identity(
                        AttributeType::string(),
                        types::ipv4_cidr(),
                    ),
                )
                .create_only()
                .with_description("Not supported. Use IP permissions instead.")
                .with_provider_name("CidrIp")
                .build(),
            )
            .attribute(
                AttributeSchema::optional(
                    "cidr_ipv6",
                    carina_aws_types::with_member_identity(
                        AttributeType::string(),
                        types::ipv6_cidr(),
                    ),
                )
                .create_only()
                .with_description("The IPv6 CIDR range.")
                .with_provider_name("CidrIpv6")
                .build(),
            )
            .attribute(
                AttributeSchema::optional("description", AttributeType::string())
                    .create_only()
                    .with_description("The security group rule description.")
                    .with_provider_name("Description")
                    .build(),
            )
            .attribute(
                AttributeSchema::optional(
                    "destination_prefix_list_id",
                    carina_aws_types::with_member_identity(
                        AttributeType::string(),
                        carina_aws_types::AwsType::PrefixListId.attribute_type(),
                    ),
                )
                .create_only()
                .with_description("The ID of the destination prefix list.")
                .with_provider_name("DestinationPrefixListId")
                .build(),
            )
            .attribute(
                AttributeSchema::optional(
                    "from_port",
                    AttributeType::int_member(
                        carina_core::schema::IntIdentityLayer::new(None, None),
                        carina_core::schema::IntMemberConstraints::new(Some((
                            Some(-1),
                            Some(65535),
                        ))),
                    ),
                )
                .create_only()
                .with_description("Not supported. Use IP permissions instead.")
                .with_provider_name("FromPort")
                .build(),
            )
            .attribute(
                AttributeSchema::required(
                    "group_id",
                    carina_aws_types::with_member_identity(
                        AttributeType::string(),
                        carina_aws_types::AwsType::SecurityGroupId.attribute_type(),
                    ),
                )
                .create_only()
                .with_description("The ID of the security group.")
                .with_provider_name("GroupId")
                .build(),
            )
            .attribute(
                AttributeSchema::required(
                    "ip_protocol",
                    AttributeType::enum_(
                        carina_core::schema::enum_identity(
                            "IpProtocol",
                            Some("aws.ec2.SecurityGroupEgress"),
                        ),
                        Some(vec![
                            "tcp".to_string(),
                            "udp".to_string(),
                            "icmp".to_string(),
                            "icmpv6".to_string(),
                            "-1".to_string(),
                        ]),
                        vec![
                            ("-1".to_string(), "all".to_string()),
                            ("tcp".to_string(), "tcp".to_string()),
                            ("udp".to_string(), "udp".to_string()),
                            ("icmp".to_string(), "icmp".to_string()),
                            ("icmpv6".to_string(), "icmpv6".to_string()),
                        ],
                        None,
                        None,
                    ),
                )
                .create_only()
                .with_description("Not supported. Use IP permissions instead.")
                .with_provider_name("IpProtocol")
                .build(),
            )
            .attribute(
                AttributeSchema::optional(
                    "source_security_group_owner_id",
                    carina_aws_types::with_member_identity(
                        AttributeType::string(),
                        carina_aws_types::AwsType::AccountId.attribute_type(),
                    ),
                )
                .create_only()
                .with_description("Not supported. Use IP permissions instead.")
                .with_provider_name("SourceSecurityGroupOwnerId")
                .build(),
            )
            .attribute(
                AttributeSchema::optional(
                    "to_port",
                    AttributeType::int_member(
                        carina_core::schema::IntIdentityLayer::new(None, None),
                        carina_core::schema::IntMemberConstraints::new(Some((
                            Some(-1),
                            Some(65535),
                        ))),
                    ),
                )
                .create_only()
                .with_description("Not supported. Use IP permissions instead.")
                .with_provider_name("ToPort")
                .build(),
            )
            .attribute(
                AttributeSchema::optional(
                    "destination_security_group_id",
                    carina_aws_types::with_member_identity(
                        AttributeType::string(),
                        carina_aws_types::AwsType::SecurityGroupId.attribute_type(),
                    ),
                )
                .create_only()
                .with_description("The ID of the destination security group.")
                .with_provider_name("DestinationSecurityGroupId")
                .build(),
            )
            .attribute(
                AttributeSchema::provider_populated(
                    "security_group_rule_id",
                    carina_aws_types::with_member_identity(
                        AttributeType::string(),
                        carina_aws_types::AwsType::SecurityGroupRuleId.attribute_type(),
                    ),
                )
                .with_description("The ID of the security group rule. (read-only)")
                .with_provider_name("SecurityGroupRuleId")
                .build(),
            ),
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    (
        "ec2.SecurityGroupEgress",
        &[("ip_protocol", VALID_IP_PROTOCOL)],
    )
}
