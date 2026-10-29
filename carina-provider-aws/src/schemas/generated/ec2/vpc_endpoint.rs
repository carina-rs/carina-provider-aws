//! VpcEndpoint schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.ec2
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use super::tags_type;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema};
use carina_provider_protocol::types::ValidatorType;

const VALID_VPC_ENDPOINT_TYPE: &[&str] = &[
    "Gateway",
    "GatewayLoadBalancer",
    "Interface",
    "Resource",
    "ServiceNetwork",
    "Tunnel",
];

/// Returns the schema config for ec2.VpcEndpoint (Smithy: com.amazonaws.ec2)
pub fn ec2_vpc_endpoint_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::EC2::VPCEndpoint",
        resource_type_name: "ec2.VpcEndpoint",
        has_tags: true,
        schema: ResourceSchema::new("ec2.VpcEndpoint")
        .with_description("Describes a VPC endpoint.")
        .with_conflicting_replacement()
        .attribute(
            AttributeSchema::optional("policy_document", AttributeType::string())
                .with_description("(Interface and gateway endpoints) A policy to attach to the endpoint that controls access to the service. The policy must be in valid JSON format. If ...")
                .with_provider_name("PolicyDocument")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("private_dns_enabled", AttributeType::bool())
                .with_description("(Interface endpoint) Indicates whether to associate a private hosted zone with the specified VPC. The private hosted zone contains a record set for th...")
                .with_provider_name("PrivateDnsEnabled")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("resource_configuration_arn", super::arn())
                .create_only()
                .with_description("(Resource and Tunnel endpoints only) The Amazon Resource Name (ARN) of a resource configuration associated with the VPC endpoint. The type of resource...")
                .with_provider_name("ResourceConfigurationArn")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("route_table_ids", AttributeType::list(super::route_table_id()))
                .create_only()
                .with_description("(Gateway endpoint) The route table IDs.")
                .with_provider_name("RouteTableIds")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("security_group_ids", AttributeType::list(super::security_group_id()))
                .create_only()
                .with_description("(Interface, Resource, ServiceNetwork, and Tunnel endpoints only) The IDs of the security groups to associate with the endpoint network interfaces. If ...")
                .with_provider_name("SecurityGroupIds")
                .build(),
        )
        .attribute(
            AttributeSchema::required("service_name", AttributeType::string())
                .create_only()
                .with_description("The name of the endpoint service.")
                .with_provider_name("ServiceName")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("service_network_arn", super::arn())
                .create_only()
                .with_description("The Amazon Resource Name (ARN) of a service network that will be associated with the VPC endpoint of type service-network.")
                .with_provider_name("ServiceNetworkArn")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("service_region", super::aws_region())
                .create_only()
                .with_description("The Region where the service is hosted. The default is the current Region.")
                .with_provider_name("ServiceRegion")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("subnet_ids", AttributeType::list(super::subnet_id()))
                .create_only()
                .with_description("(Interface, Gateway Load Balancer endpoints, Resource, ServiceNetwork, and Tunnel endpoints only) The IDs of the subnets in which to create endpoint n...")
                .with_provider_name("SubnetIds")
                .build(),
        )
        .attribute(
            AttributeSchema::optional("vpc_endpoint_type", AttributeType::enum_(
    carina_core::schema::enum_identity("VpcEndpointType", Some("aws.ec2.VpcEndpoint")),
    Some(vec!["Gateway".to_string(), "GatewayLoadBalancer".to_string(), "Interface".to_string(), "Resource".to_string(), "ServiceNetwork".to_string(), "Tunnel".to_string()]),
    vec![("Gateway".to_string(), "gateway".to_string()), ("GatewayLoadBalancer".to_string(), "gateway_load_balancer".to_string()), ("Interface".to_string(), "interface".to_string()), ("Resource".to_string(), "resource".to_string()), ("ServiceNetwork".to_string(), "service_network".to_string()), ("Tunnel".to_string(), "tunnel".to_string())],
    None,
    None,
))
                .create_only()
                .with_description("The type of endpoint. For more information about the types of VPC endpoints, see VPC endpoints in the Amazon Web Services PrivateLink User Guide. Defa...")
                .with_provider_name("VpcEndpointType")
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
            AttributeSchema::provider_populated("vpc_endpoint_id", super::vpc_endpoint_id())
                .with_description("The ID of the endpoint. (read-only)")
                .with_provider_name("VpcEndpointId")
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
    (
        "ec2.VpcEndpoint",
        &[("vpc_endpoint_type", VALID_VPC_ENDPOINT_TYPE)],
    )
}
