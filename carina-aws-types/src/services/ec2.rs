use carina_core::schema::{AttributeType, TypeIdentity};

use crate::provider_type;

fn resource_id(identity: TypeIdentity, pattern: &str) -> AttributeType {
    AttributeType::refined_string(Some(identity), Some(pattern.to_string()), None, None)
}

/// VPC ID type (e.g., "vpc-1a2b3c4d")
pub fn vpc_id() -> AttributeType {
    resource_id(provider_type("ec2", "Vpc", "Id"), "^vpc-[0-9a-f]{8,}$")
}

/// Subnet ID type (e.g., "subnet-0123456789abcdef0")
pub fn subnet_id() -> AttributeType {
    resource_id(
        provider_type("ec2", "Subnet", "Id"),
        "^subnet-[0-9a-f]{8,}$",
    )
}

/// Security Group ID type (e.g., "sg-12345678")
pub fn security_group_id() -> AttributeType {
    resource_id(
        provider_type("ec2", "SecurityGroup", "Id"),
        "^sg-[0-9a-f]{8,}$",
    )
}

/// Internet Gateway ID type (e.g., "igw-12345678")
pub fn internet_gateway_id() -> AttributeType {
    resource_id(
        provider_type("ec2", "InternetGateway", "Id"),
        "^igw-[0-9a-f]{8,}$",
    )
}

/// Route Table ID type (e.g., "rtb-abcdef12")
pub fn route_table_id() -> AttributeType {
    resource_id(
        provider_type("ec2", "RouteTable", "Id"),
        "^rtb-[0-9a-f]{8,}$",
    )
}

/// NAT Gateway ID type (e.g., "nat-12345678")
pub fn nat_gateway_id() -> AttributeType {
    resource_id(
        provider_type("ec2", "NatGateway", "Id"),
        "^nat-[0-9a-f]{8,}$",
    )
}

/// VPC Peering Connection ID type (e.g., "pcx-12345678")
pub fn vpc_peering_connection_id() -> AttributeType {
    resource_id(
        provider_type("ec2", "VpcPeeringConnection", "Id"),
        "^pcx-[0-9a-f]{8,}$",
    )
}

/// Transit Gateway ID type (e.g., "tgw-12345678")
pub fn transit_gateway_id() -> AttributeType {
    resource_id(
        provider_type("ec2", "TransitGateway", "Id"),
        "^tgw-[0-9a-f]{8,}$",
    )
}

/// VPC CIDR Block Association ID type (e.g., "vpc-cidr-assoc-12345678")
pub fn vpc_cidr_block_association_id() -> AttributeType {
    resource_id(
        provider_type("ec2", "VpcCidrBlockAssociation", "Id"),
        "^vpc-cidr-assoc-[0-9a-f]{8,}$",
    )
}

/// Transit Gateway Route Table ID type (e.g., "tgw-rtb-12345678")
pub fn tgw_route_table_id() -> AttributeType {
    resource_id(
        provider_type("ec2", "TransitGatewayRouteTable", "Id"),
        "^tgw-rtb-[0-9a-f]{8,}$",
    )
}

/// VPN Gateway ID type (e.g., "vgw-12345678")
pub fn vpn_gateway_id() -> AttributeType {
    resource_id(
        provider_type("ec2", "VpnGateway", "Id"),
        "^vgw-[0-9a-f]{8,}$",
    )
}

/// Gateway ID type - union of InternetGatewayId and VpnGatewayId.
pub fn gateway_id() -> AttributeType {
    AttributeType::union(vec![internet_gateway_id(), vpn_gateway_id()])
}

/// Egress Only Internet Gateway ID type (e.g., "eigw-12345678")
pub fn egress_only_internet_gateway_id() -> AttributeType {
    resource_id(
        provider_type("ec2", "EgressOnlyInternetGateway", "Id"),
        "^eigw-[0-9a-f]{8,}$",
    )
}

/// VPC Endpoint ID type (e.g., "vpce-12345678")
pub fn vpc_endpoint_id() -> AttributeType {
    resource_id(
        provider_type("ec2", "VpcEndpoint", "Id"),
        "^vpce-[0-9a-f]{8,}$",
    )
}

/// Instance ID type (e.g., "i-0123456789abcdef0")
pub fn instance_id() -> AttributeType {
    resource_id(provider_type("ec2", "Instance", "Id"), "^i-[0-9a-f]{8,}$")
}

/// Network Interface ID type (e.g., "eni-0123456789abcdef0")
pub fn network_interface_id() -> AttributeType {
    resource_id(
        provider_type("ec2", "NetworkInterface", "Id"),
        "^eni-[0-9a-f]{8,}$",
    )
}

/// EIP Allocation ID type (e.g., "eipalloc-0123456789abcdef0")
#[allow(dead_code)]
pub fn allocation_id() -> AttributeType {
    resource_id(
        provider_type("ec2", "Eip", "AllocationId"),
        "^eipalloc-[0-9a-f]{8,}$",
    )
}

/// Prefix List ID type (e.g., "pl-0123456789abcdef0")
pub fn prefix_list_id() -> AttributeType {
    resource_id(
        provider_type("ec2", "PrefixList", "Id"),
        "^pl-[0-9a-f]{8,}$",
    )
}

/// Carrier Gateway ID type (e.g., "cagw-0123456789abcdef0")
pub fn carrier_gateway_id() -> AttributeType {
    resource_id(
        provider_type("ec2", "CarrierGateway", "Id"),
        "^cagw-[0-9a-f]{8,}$",
    )
}

/// Local Gateway ID type (e.g., "lgw-0123456789abcdef0")
pub fn local_gateway_id() -> AttributeType {
    resource_id(
        provider_type("ec2", "LocalGateway", "Id"),
        "^lgw-[0-9a-f]{8,}$",
    )
}

/// Network ACL ID type (e.g., "acl-0123456789abcdef0")
#[allow(dead_code)]
pub fn network_acl_id() -> AttributeType {
    resource_id(
        provider_type("ec2", "NetworkAcl", "Id"),
        "^acl-[0-9a-f]{8,}$",
    )
}

/// Transit Gateway Attachment ID type (e.g., "tgw-attach-0123456789abcdef0")
pub fn transit_gateway_attachment_id() -> AttributeType {
    resource_id(
        provider_type("ec2", "TransitGatewayAttachment", "Id"),
        "^tgw-attach-[0-9a-f]{8,}$",
    )
}

/// Flow Log ID type (e.g., "fl-0123456789abcdef0")
pub fn flow_log_id() -> AttributeType {
    resource_id(provider_type("ec2", "FlowLog", "Id"), "^fl-[0-9a-f]{8,}$")
}

/// IPAM ID type (e.g., "ipam-0123456789abcdef0")
pub fn ipam_id() -> AttributeType {
    resource_id(provider_type("ec2", "Ipam", "Id"), "^ipam-[0-9a-f]{8,}$")
}

/// Subnet Route Table Association ID type (e.g., "rtbassoc-0123456789abcdef0")
pub fn subnet_route_table_association_id() -> AttributeType {
    resource_id(
        provider_type("ec2", "SubnetRouteTableAssociation", "Id"),
        "^rtbassoc-[0-9a-f]{8,}$",
    )
}

/// Security Group Rule ID type (e.g., "sgr-0123456789abcdef0")
pub fn security_group_rule_id() -> AttributeType {
    resource_id(
        provider_type("ec2", "SecurityGroupRule", "Id"),
        "^sgr-[0-9a-f]{8,}$",
    )
}
