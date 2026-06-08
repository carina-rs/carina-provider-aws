use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{AttributeType, legacy_validator};

use crate::{aws_resource_id, provider_type, validate_prefixed_resource_id};

/// VPC ID type (e.g., "vpc-1a2b3c4d")
pub fn vpc_id() -> AttributeType {
    AttributeType::custom(
        Some(provider_type("ec2", "Vpc", "Id")),
        aws_resource_id(),
        Some("^vpc-[0-9a-f]{8,}$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "vpc")
                    .map_err(|reason| format!("Invalid VPC ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

/// Subnet ID type (e.g., "subnet-0123456789abcdef0")
pub fn subnet_id() -> AttributeType {
    AttributeType::custom(
        Some(provider_type("ec2", "Subnet", "Id")),
        aws_resource_id(),
        Some("^subnet-[0-9a-f]{8,}$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "subnet")
                    .map_err(|reason| format!("Invalid Subnet ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

/// Security Group ID type (e.g., "sg-12345678")
pub fn security_group_id() -> AttributeType {
    AttributeType::custom(
        Some(provider_type("ec2", "SecurityGroup", "Id")),
        aws_resource_id(),
        Some("^sg-[0-9a-f]{8,}$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "sg")
                    .map_err(|reason| format!("Invalid Security Group ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

/// Internet Gateway ID type (e.g., "igw-12345678")
pub fn internet_gateway_id() -> AttributeType {
    AttributeType::custom(
        Some(provider_type("ec2", "InternetGateway", "Id")),
        aws_resource_id(),
        Some("^igw-[0-9a-f]{8,}$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "igw")
                    .map_err(|reason| format!("Invalid Internet Gateway ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

/// Route Table ID type (e.g., "rtb-abcdef12")
pub fn route_table_id() -> AttributeType {
    AttributeType::custom(
        Some(provider_type("ec2", "RouteTable", "Id")),
        aws_resource_id(),
        Some("^rtb-[0-9a-f]{8,}$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "rtb")
                    .map_err(|reason| format!("Invalid Route Table ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

/// NAT Gateway ID type (e.g., "nat-12345678")
pub fn nat_gateway_id() -> AttributeType {
    AttributeType::custom(
        Some(provider_type("ec2", "NatGateway", "Id")),
        aws_resource_id(),
        Some("^nat-[0-9a-f]{8,}$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "nat")
                    .map_err(|reason| format!("Invalid NAT Gateway ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

/// VPC Peering Connection ID type (e.g., "pcx-12345678")
pub fn vpc_peering_connection_id() -> AttributeType {
    AttributeType::custom(
        Some(provider_type("ec2", "VpcPeeringConnection", "Id")),
        aws_resource_id(),
        Some("^pcx-[0-9a-f]{8,}$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "pcx").map_err(|reason| {
                    format!("Invalid VPC Peering Connection ID '{}': {}", s, reason)
                })
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

/// Transit Gateway ID type (e.g., "tgw-12345678")
pub fn transit_gateway_id() -> AttributeType {
    AttributeType::custom(
        Some(provider_type("ec2", "TransitGateway", "Id")),
        aws_resource_id(),
        Some("^tgw-[0-9a-f]{8,}$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "tgw")
                    .map_err(|reason| format!("Invalid Transit Gateway ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

/// VPC CIDR Block Association ID type (e.g., "vpc-cidr-assoc-12345678")
pub fn vpc_cidr_block_association_id() -> AttributeType {
    AttributeType::custom(
        Some(provider_type("ec2", "VpcCidrBlockAssociation", "Id")),
        aws_resource_id(),
        Some("^vpc-cidr-assoc-[0-9a-f]{8,}$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "vpc-cidr-assoc").map_err(|reason| {
                    format!("Invalid VPC CIDR Block Association ID '{}': {}", s, reason)
                })
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

/// Transit Gateway Route Table ID type (e.g., "tgw-rtb-12345678")
pub fn tgw_route_table_id() -> AttributeType {
    AttributeType::custom(
        Some(provider_type("ec2", "TransitGatewayRouteTable", "Id")),
        aws_resource_id(),
        Some("^tgw-rtb-[0-9a-f]{8,}$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "tgw-rtb")
                    .map_err(|reason| format!("Invalid TGW Route Table ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

/// VPN Gateway ID type (e.g., "vgw-12345678")
pub fn vpn_gateway_id() -> AttributeType {
    AttributeType::custom(
        Some(provider_type("ec2", "VpnGateway", "Id")),
        aws_resource_id(),
        Some("^vgw-[0-9a-f]{8,}$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "vgw")
                    .map_err(|reason| format!("Invalid VPN Gateway ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

/// Gateway ID type — union of InternetGatewayId and VpnGatewayId.
pub fn gateway_id() -> AttributeType {
    AttributeType::union(vec![internet_gateway_id(), vpn_gateway_id()])
}

/// Egress Only Internet Gateway ID type (e.g., "eigw-12345678")
pub fn egress_only_internet_gateway_id() -> AttributeType {
    AttributeType::custom(
        Some(provider_type("ec2", "EgressOnlyInternetGateway", "Id")),
        aws_resource_id(),
        Some("^eigw-[0-9a-f]{8,}$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "eigw").map_err(|reason| {
                    format!(
                        "Invalid Egress Only Internet Gateway ID '{}': {}",
                        s, reason
                    )
                })
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

/// VPC Endpoint ID type (e.g., "vpce-12345678")
pub fn vpc_endpoint_id() -> AttributeType {
    AttributeType::custom(
        Some(provider_type("ec2", "VpcEndpoint", "Id")),
        aws_resource_id(),
        Some("^vpce-[0-9a-f]{8,}$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "vpce")
                    .map_err(|reason| format!("Invalid VPC Endpoint ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

/// Instance ID type (e.g., "i-0123456789abcdef0")
pub fn instance_id() -> AttributeType {
    AttributeType::custom(
        Some(provider_type("ec2", "Instance", "Id")),
        aws_resource_id(),
        Some("^i-[0-9a-f]{8,}$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "i")
                    .map_err(|reason| format!("Invalid Instance ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

/// Network Interface ID type (e.g., "eni-0123456789abcdef0")
pub fn network_interface_id() -> AttributeType {
    AttributeType::custom(
        Some(provider_type("ec2", "NetworkInterface", "Id")),
        aws_resource_id(),
        Some("^eni-[0-9a-f]{8,}$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "eni")
                    .map_err(|reason| format!("Invalid Network Interface ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

/// EIP Allocation ID type (e.g., "eipalloc-0123456789abcdef0")
#[allow(dead_code)]
pub fn allocation_id() -> AttributeType {
    AttributeType::custom(
        Some(provider_type("ec2", "Eip", "AllocationId")),
        aws_resource_id(),
        Some("^eipalloc-[0-9a-f]{8,}$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "eipalloc")
                    .map_err(|reason| format!("Invalid Allocation ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

/// Prefix List ID type (e.g., "pl-0123456789abcdef0")
pub fn prefix_list_id() -> AttributeType {
    AttributeType::custom(
        Some(provider_type("ec2", "PrefixList", "Id")),
        aws_resource_id(),
        Some("^pl-[0-9a-f]{8,}$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "pl")
                    .map_err(|reason| format!("Invalid Prefix List ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

/// Carrier Gateway ID type (e.g., "cagw-0123456789abcdef0")
pub fn carrier_gateway_id() -> AttributeType {
    AttributeType::custom(
        Some(provider_type("ec2", "CarrierGateway", "Id")),
        aws_resource_id(),
        Some("^cagw-[0-9a-f]{8,}$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "cagw")
                    .map_err(|reason| format!("Invalid Carrier Gateway ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

/// Local Gateway ID type (e.g., "lgw-0123456789abcdef0")
pub fn local_gateway_id() -> AttributeType {
    AttributeType::custom(
        Some(provider_type("ec2", "LocalGateway", "Id")),
        aws_resource_id(),
        Some("^lgw-[0-9a-f]{8,}$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "lgw")
                    .map_err(|reason| format!("Invalid Local Gateway ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

/// Network ACL ID type (e.g., "acl-0123456789abcdef0")
#[allow(dead_code)]
pub fn network_acl_id() -> AttributeType {
    AttributeType::custom(
        Some(provider_type("ec2", "NetworkAcl", "Id")),
        aws_resource_id(),
        Some("^acl-[0-9a-f]{8,}$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "acl")
                    .map_err(|reason| format!("Invalid Network ACL ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

/// Transit Gateway Attachment ID type (e.g., "tgw-attach-0123456789abcdef0")
pub fn transit_gateway_attachment_id() -> AttributeType {
    AttributeType::custom(
        Some(provider_type("ec2", "TransitGatewayAttachment", "Id")),
        aws_resource_id(),
        Some("^tgw-attach-[0-9a-f]{8,}$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "tgw-attach").map_err(|reason| {
                    format!("Invalid Transit Gateway Attachment ID '{}': {}", s, reason)
                })
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

/// Flow Log ID type (e.g., "fl-0123456789abcdef0")
pub fn flow_log_id() -> AttributeType {
    AttributeType::custom(
        Some(provider_type("ec2", "FlowLog", "Id")),
        aws_resource_id(),
        Some("^fl-[0-9a-f]{8,}$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "fl")
                    .map_err(|reason| format!("Invalid Flow Log ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

/// IPAM ID type (e.g., "ipam-0123456789abcdef0")
pub fn ipam_id() -> AttributeType {
    AttributeType::custom(
        Some(provider_type("ec2", "Ipam", "Id")),
        aws_resource_id(),
        Some("^ipam-[0-9a-f]{8,}$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "ipam")
                    .map_err(|reason| format!("Invalid IPAM ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

/// Subnet Route Table Association ID type (e.g., "rtbassoc-0123456789abcdef0")
pub fn subnet_route_table_association_id() -> AttributeType {
    AttributeType::custom(
        Some(provider_type("ec2", "SubnetRouteTableAssociation", "Id")),
        aws_resource_id(),
        Some("^rtbassoc-[0-9a-f]{8,}$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "rtbassoc").map_err(|reason| {
                    format!(
                        "Invalid Subnet Route Table Association ID '{}': {}",
                        s, reason
                    )
                })
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}

/// Security Group Rule ID type (e.g., "sgr-0123456789abcdef0")
pub fn security_group_rule_id() -> AttributeType {
    AttributeType::custom(
        Some(provider_type("ec2", "SecurityGroupRule", "Id")),
        aws_resource_id(),
        Some("^sgr-[0-9a-f]{8,}$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "sgr")
                    .map_err(|reason| format!("Invalid Security Group Rule ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}
