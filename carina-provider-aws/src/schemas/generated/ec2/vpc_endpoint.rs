//! VpcEndpoint schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.ec2
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use super::tags_type;
use super::validate_tags_map;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};

const VALID_VPC_ENDPOINT_TYPE: &[&str] = &[
    "Gateway",
    "GatewayLoadBalancer",
    "Interface",
    "Resource",
    "ServiceNetwork",
    "gateway",
    "gateway_load_balancer",
    "interface",
    "resource",
    "service_network",
];

/// Returns the schema config for ec2.VpcEndpoint (Smithy: com.amazonaws.ec2)
pub fn ec2_vpc_endpoint_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::EC2::VPCEndpoint",
        resource_type_name: "ec2.VpcEndpoint",
        has_tags: true,
        schema: ResourceSchema::new("ec2.VpcEndpoint")
        .with_description("Describes a VPC endpoint.")
        .attribute(
            AttributeSchema::new("policy_document", AttributeType::string())
                .with_description("(Interface and gateway endpoints) A policy to attach to the endpoint that controls access to the service. The policy must be in valid JSON format. If ...")
                .with_provider_name("PolicyDocument"),
        )
        .attribute(
            AttributeSchema::new("private_dns_enabled", AttributeType::bool())
                .with_description("(Interface endpoint) Indicates whether to associate a private hosted zone with the specified VPC. The private hosted zone contains a record set for th...")
                .with_provider_name("PrivateDnsEnabled"),
        )
        .attribute(
            AttributeSchema::new("resource_configuration_arn", super::arn())
                .create_only()
                .with_description("The Amazon Resource Name (ARN) of a resource configuration that will be associated with the VPC endpoint of type resource.")
                .with_provider_name("ResourceConfigurationArn"),
        )
        .attribute(
            AttributeSchema::new("route_table_ids", AttributeType::list(super::route_table_id()))
                .create_only()
                .with_description("(Gateway endpoint) The route table IDs.")
                .with_provider_name("RouteTableIds"),
        )
        .attribute(
            AttributeSchema::new("security_group_ids", AttributeType::list(super::security_group_id()))
                .create_only()
                .with_description("(Interface endpoint) The IDs of the security groups to associate with the endpoint network interfaces. If this parameter is not specified, we use the ...")
                .with_provider_name("SecurityGroupIds"),
        )
        .attribute(
            AttributeSchema::new("service_name", AttributeType::string())
                .required()
                .create_only()
                .with_description("The name of the endpoint service.")
                .with_provider_name("ServiceName"),
        )
        .attribute(
            AttributeSchema::new("service_network_arn", super::arn())
                .create_only()
                .with_description("The Amazon Resource Name (ARN) of a service network that will be associated with the VPC endpoint of type service-network.")
                .with_provider_name("ServiceNetworkArn"),
        )
        .attribute(
            AttributeSchema::new("service_region", super::aws_region())
                .create_only()
                .with_description("The Region where the service is hosted. The default is the current Region.")
                .with_provider_name("ServiceRegion"),
        )
        .attribute(
            AttributeSchema::new("subnet_ids", AttributeType::list(super::subnet_id()))
                .create_only()
                .with_description("(Interface and Gateway Load Balancer endpoints) The IDs of the subnets in which to create endpoint network interfaces. For a Gateway Load Balancer end...")
                .with_provider_name("SubnetIds"),
        )
        .attribute(
            AttributeSchema::new("vpc_endpoint_type", AttributeType::string_enum(
                "VpcEndpointType".to_string(),
                vec!["Gateway".to_string(), "GatewayLoadBalancer".to_string(), "Interface".to_string(), "Resource".to_string(), "ServiceNetwork".to_string()],
                Some(carina_core::schema::string_enum_identity("VpcEndpointType", Some("aws.ec2.VpcEndpoint"))),
                vec![("Gateway".to_string(), "gateway".to_string()), ("GatewayLoadBalancer".to_string(), "gateway_load_balancer".to_string()), ("Interface".to_string(), "interface".to_string()), ("Resource".to_string(), "resource".to_string()), ("ServiceNetwork".to_string(), "service_network".to_string())],
            ))
                .create_only()
                .with_description("The type of endpoint. Default: Gateway")
                .with_provider_name("VpcEndpointType"),
        )
        .attribute(
            AttributeSchema::new("vpc_id", super::vpc_id())
                .required()
                .create_only()
                .with_description("The ID of the VPC.")
                .with_provider_name("VpcId"),
        )
        .attribute(
            AttributeSchema::new("vpc_endpoint_id", super::vpc_endpoint_id())
                .read_only()
                .with_description("The ID of the endpoint. (read-only)")
                .with_provider_name("VpcEndpointId"),
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
    (
        "ec2.VpcEndpoint",
        &[("vpc_endpoint_type", VALID_VPC_ENDPOINT_TYPE)],
    )
}

/// Maps DSL alias values back to canonical AWS values for this module.
/// e.g., ("ip_protocol", "all") -> Some("-1")
pub fn enum_alias_reverse(attr_name: &str, value: &str) -> Option<&'static str> {
    match (attr_name, value) {
        ("vpc_endpoint_type", "gateway") => Some("Gateway"),
        ("vpc_endpoint_type", "gateway_load_balancer") => Some("GatewayLoadBalancer"),
        ("vpc_endpoint_type", "interface") => Some("Interface"),
        ("vpc_endpoint_type", "resource") => Some("Resource"),
        ("vpc_endpoint_type", "service_network") => Some("ServiceNetwork"),
        _ => None,
    }
}

/// Returns all enum alias entries as (attr_name, alias, canonical) tuples.
pub fn enum_alias_entries() -> &'static [(&'static str, &'static str, &'static str)] {
    &[
        ("vpc_endpoint_type", "gateway", "Gateway"),
        (
            "vpc_endpoint_type",
            "gateway_load_balancer",
            "GatewayLoadBalancer",
        ),
        ("vpc_endpoint_type", "interface", "Interface"),
        ("vpc_endpoint_type", "resource", "Resource"),
        ("vpc_endpoint_type", "service_network", "ServiceNetwork"),
    ]
}
