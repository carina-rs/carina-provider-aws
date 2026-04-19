//! Shared AWS type definitions and validators
//!
//! This module contains type validators shared between `carina-provider-aws`
//! and `carina-provider-awscc`. Provider-specific types (region with namespace,
//! schema config structs) remain in their respective crates.

use carina_core::resource::Value;
use carina_core::schema::{AttributeType, CompletionValue, StructField};

// ========== Enum helpers ==========

/// Check if `input` matches any of `valid_values` using enum matching rules:
/// exact match, case-insensitive, or underscore-to-hyphen (case-insensitive).
/// Returns the matched valid value if found.
pub fn find_matching_enum_value<'a>(input: &str, valid_values: &[&'a str]) -> Option<&'a str> {
    // Exact match
    if let Some(&v) = valid_values.iter().find(|&&v| v == input) {
        return Some(v);
    }
    // Case-insensitive match
    if let Some(&v) = valid_values
        .iter()
        .find(|&&v| v.eq_ignore_ascii_case(input))
    {
        return Some(v);
    }
    // Underscore-to-hyphen match (case-insensitive)
    let hyphenated = input.replace('_', "-");
    if let Some(&v) = valid_values
        .iter()
        .find(|&&v| v.eq_ignore_ascii_case(&hyphenated))
    {
        return Some(v);
    }
    None
}

/// Canonicalize an enum value by matching against valid values.
/// Handles exact match, case-insensitive match, and underscore-to-hyphen conversion.
pub fn canonicalize_enum_value(raw: &str, valid_values: &[&str]) -> String {
    find_matching_enum_value(raw, valid_values)
        .unwrap_or(raw)
        .to_string()
}

// ========== Region constants ==========

/// AWS regions with display names. Single source of truth for validation and completions.
pub const REGIONS: &[(&str, &str)] = &[
    // Africa
    ("af-south-1", "Africa (Cape Town)"),
    // Asia Pacific
    ("ap-east-1", "Asia Pacific (Hong Kong)"),
    ("ap-east-2", "Asia Pacific (Malaysia)"),
    ("ap-northeast-1", "Asia Pacific (Tokyo)"),
    ("ap-northeast-2", "Asia Pacific (Seoul)"),
    ("ap-northeast-3", "Asia Pacific (Osaka)"),
    ("ap-south-1", "Asia Pacific (Mumbai)"),
    ("ap-south-2", "Asia Pacific (Hyderabad)"),
    ("ap-southeast-1", "Asia Pacific (Singapore)"),
    ("ap-southeast-2", "Asia Pacific (Sydney)"),
    ("ap-southeast-3", "Asia Pacific (Jakarta)"),
    ("ap-southeast-4", "Asia Pacific (Melbourne)"),
    ("ap-southeast-5", "Asia Pacific (Auckland)"),
    ("ap-southeast-6", "Asia Pacific (Thailand)"),
    ("ap-southeast-7", "Asia Pacific (Taiwan)"),
    // Canada
    ("ca-central-1", "Canada (Central)"),
    ("ca-west-1", "Canada West (Calgary)"),
    // China
    ("cn-north-1", "China (Beijing)"),
    ("cn-northwest-1", "China (Ningxia)"),
    // Europe
    ("eu-central-1", "Europe (Frankfurt)"),
    ("eu-central-2", "Europe (Zurich)"),
    ("eu-north-1", "Europe (Stockholm)"),
    ("eu-south-1", "Europe (Milan)"),
    ("eu-south-2", "Europe (Spain)"),
    ("eu-west-1", "Europe (Ireland)"),
    ("eu-west-2", "Europe (London)"),
    ("eu-west-3", "Europe (Paris)"),
    // Israel
    ("il-central-1", "Israel (Tel Aviv)"),
    // Middle East
    ("me-central-1", "Middle East (UAE)"),
    ("me-south-1", "Middle East (Bahrain)"),
    // Mexico
    ("mx-central-1", "Mexico (Central)"),
    // South America
    ("sa-east-1", "South America (Sao Paulo)"),
    // US
    ("us-east-1", "US East (N. Virginia)"),
    ("us-east-2", "US East (Ohio)"),
    ("us-gov-east-1", "AWS GovCloud (US-East)"),
    ("us-gov-west-1", "AWS GovCloud (US-West)"),
    ("us-west-1", "US West (N. California)"),
    ("us-west-2", "US West (Oregon)"),
];

/// Check if a region code is valid.
pub fn is_valid_region(region: &str) -> bool {
    REGIONS.iter().any(|(code, _)| *code == region)
}

/// Format valid region codes as a comma-separated string for error messages.
pub fn valid_regions_display() -> String {
    REGIONS
        .iter()
        .map(|(code, _)| *code)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Generate region completion values for a given provider prefix (e.g., "aws" or "awscc").
///
/// Converts AWS region format (`ap-northeast-1`) to DSL format (`ap_northeast_1`)
/// and prefixes with `{prefix}.Region.`.
pub fn region_completions(prefix: &str) -> Vec<CompletionValue> {
    REGIONS
        .iter()
        .map(|(code, name)| {
            let dsl_code = code.replace('-', "_");
            CompletionValue::new(format!("{}.Region.{}", prefix, dsl_code), *name)
        })
        .collect()
}

// ========== Tags ==========

/// Tags type for AWS resources (map of string values)
pub fn tags_type() -> AttributeType {
    AttributeType::map(AttributeType::String)
}

/// Validate that a tags map does not use Key/Value pair list structure.
///
/// Detects when a tags map contains both `key` and `value` as keys (case-insensitive),
/// which indicates the user wrote a Key/Value pair list instead of a flat map:
///   Wrong: `tags = { key = 'Name', value = '...' }`
///   Right: `tags = { Name = '...' }`
pub fn validate_tags_map(
    attributes: &std::collections::HashMap<String, Value>,
) -> Result<(), Vec<carina_core::schema::TypeError>> {
    if let Some(Value::Map(map)) = attributes.get("tags") {
        let has_key = map.keys().any(|k| k.eq_ignore_ascii_case("key"));
        let has_value = map.keys().any(|k| k.eq_ignore_ascii_case("value"));
        if has_key && has_value {
            return Err(vec![carina_core::schema::TypeError::ResourceValidationFailed {
                message: "tags map contains both 'key' and 'value' as keys, which looks like a Key/Value pair list. Use flat map syntax instead: tags = { Name = '...' }".to_string(),
                attribute: Some("tags".to_string()),
            }]);
        }
    }
    Ok(())
}

// ========== Resource ID validators ==========

/// Validate a generic AWS resource ID format: `{prefix}-{hex}` where hex is 8+ hex digits.
pub fn validate_aws_resource_id(id: &str) -> Result<(), String> {
    let Some(dash_pos) = id.find('-') else {
        return Err("expected format 'prefix-hexdigits'".to_string());
    };

    let prefix = &id[..dash_pos];
    let hex_part = &id[dash_pos + 1..];

    if prefix.is_empty()
        || !prefix
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    {
        return Err("prefix must be lowercase alphanumeric".to_string());
    }

    if hex_part.len() < 8 {
        return Err("ID part must be at least 8 characters after prefix".to_string());
    }

    if !hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("ID part must contain only hex digits".to_string());
    }

    Ok(())
}

/// Validate a resource ID with a specific prefix (e.g., "vpc", "subnet", "tgw-attach").
pub fn validate_prefixed_resource_id(id: &str, expected_prefix: &str) -> Result<(), String> {
    let expected_format = format!("{}-xxxxxxxx", expected_prefix);
    let Some(hex_part) = id.strip_prefix(&format!("{}-", expected_prefix)) else {
        return Err(format!("expected format '{}'", expected_format));
    };
    if hex_part.len() < 8 {
        return Err("ID part must be at least 8 characters after prefix".to_string());
    }
    if !hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("ID part must contain only hex digits".to_string());
    }
    Ok(())
}

/// AWS resource ID type (e.g., "vpc-1a2b3c4d", "subnet-0123456789abcdef0")
#[allow(dead_code)]
pub fn aws_resource_id() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("AwsResourceId".to_string()),
        pattern: Some("^[a-z-]+-[0-9a-f]{8,}$".to_string()),
        length: None,
        base: Box::new(AttributeType::String),
        validate: |value| {
            if let Value::String(s) = value {
                validate_aws_resource_id(s)
                    .map_err(|reason| format!("Invalid resource ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

/// VPC ID type (e.g., "vpc-1a2b3c4d")
pub fn vpc_id() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("VpcId".to_string()),
        pattern: Some("^vpc-[0-9a-f]{8,}$".to_string()),
        length: None,
        base: Box::new(aws_resource_id()),
        validate: |value| {
            if let Value::String(s) = value {
                validate_prefixed_resource_id(s, "vpc")
                    .map_err(|reason| format!("Invalid VPC ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

/// Subnet ID type (e.g., "subnet-0123456789abcdef0")
pub fn subnet_id() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("SubnetId".to_string()),
        pattern: Some("^subnet-[0-9a-f]{8,}$".to_string()),
        length: None,
        base: Box::new(aws_resource_id()),
        validate: |value| {
            if let Value::String(s) = value {
                validate_prefixed_resource_id(s, "subnet")
                    .map_err(|reason| format!("Invalid Subnet ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

/// Security Group ID type (e.g., "sg-12345678")
pub fn security_group_id() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("SecurityGroupId".to_string()),
        pattern: Some("^sg-[0-9a-f]{8,}$".to_string()),
        length: None,
        base: Box::new(aws_resource_id()),
        validate: |value| {
            if let Value::String(s) = value {
                validate_prefixed_resource_id(s, "sg")
                    .map_err(|reason| format!("Invalid Security Group ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

/// Internet Gateway ID type (e.g., "igw-12345678")
pub fn internet_gateway_id() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("InternetGatewayId".to_string()),
        pattern: Some("^igw-[0-9a-f]{8,}$".to_string()),
        length: None,
        base: Box::new(aws_resource_id()),
        validate: |value| {
            if let Value::String(s) = value {
                validate_prefixed_resource_id(s, "igw")
                    .map_err(|reason| format!("Invalid Internet Gateway ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

/// Route Table ID type (e.g., "rtb-abcdef12")
pub fn route_table_id() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("RouteTableId".to_string()),
        pattern: Some("^rtb-[0-9a-f]{8,}$".to_string()),
        length: None,
        base: Box::new(aws_resource_id()),
        validate: |value| {
            if let Value::String(s) = value {
                validate_prefixed_resource_id(s, "rtb")
                    .map_err(|reason| format!("Invalid Route Table ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

/// NAT Gateway ID type (e.g., "nat-12345678")
pub fn nat_gateway_id() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("NatGatewayId".to_string()),
        pattern: Some("^nat-[0-9a-f]{8,}$".to_string()),
        length: None,
        base: Box::new(aws_resource_id()),
        validate: |value| {
            if let Value::String(s) = value {
                validate_prefixed_resource_id(s, "nat")
                    .map_err(|reason| format!("Invalid NAT Gateway ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

/// VPC Peering Connection ID type (e.g., "pcx-12345678")
pub fn vpc_peering_connection_id() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("VpcPeeringConnectionId".to_string()),
        pattern: Some("^pcx-[0-9a-f]{8,}$".to_string()),
        length: None,
        base: Box::new(aws_resource_id()),
        validate: |value| {
            if let Value::String(s) = value {
                validate_prefixed_resource_id(s, "pcx").map_err(|reason| {
                    format!("Invalid VPC Peering Connection ID '{}': {}", s, reason)
                })
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

/// Transit Gateway ID type (e.g., "tgw-12345678")
pub fn transit_gateway_id() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("TransitGatewayId".to_string()),
        pattern: Some("^tgw-[0-9a-f]{8,}$".to_string()),
        length: None,
        base: Box::new(aws_resource_id()),
        validate: |value| {
            if let Value::String(s) = value {
                validate_prefixed_resource_id(s, "tgw")
                    .map_err(|reason| format!("Invalid Transit Gateway ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

/// VPC CIDR Block Association ID type (e.g., "vpc-cidr-assoc-12345678")
pub fn vpc_cidr_block_association_id() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("VpcCidrBlockAssociationId".to_string()),
        pattern: Some("^vpc-cidr-assoc-[0-9a-f]{8,}$".to_string()),
        length: None,
        base: Box::new(aws_resource_id()),
        validate: |value| {
            if let Value::String(s) = value {
                validate_prefixed_resource_id(s, "vpc-cidr-assoc").map_err(|reason| {
                    format!("Invalid VPC CIDR Block Association ID '{}': {}", s, reason)
                })
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

/// Transit Gateway Route Table ID type (e.g., "tgw-rtb-12345678")
pub fn tgw_route_table_id() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("TgwRouteTableId".to_string()),
        pattern: Some("^tgw-rtb-[0-9a-f]{8,}$".to_string()),
        length: None,
        base: Box::new(aws_resource_id()),
        validate: |value| {
            if let Value::String(s) = value {
                validate_prefixed_resource_id(s, "tgw-rtb")
                    .map_err(|reason| format!("Invalid TGW Route Table ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

/// VPN Gateway ID type (e.g., "vgw-12345678")
pub fn vpn_gateway_id() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("VpnGatewayId".to_string()),
        pattern: Some("^vgw-[0-9a-f]{8,}$".to_string()),
        length: None,
        base: Box::new(aws_resource_id()),
        validate: |value| {
            if let Value::String(s) = value {
                validate_prefixed_resource_id(s, "vgw")
                    .map_err(|reason| format!("Invalid VPN Gateway ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

/// Gateway ID type — union of InternetGatewayId and VpnGatewayId.
pub fn gateway_id() -> AttributeType {
    AttributeType::Union(vec![internet_gateway_id(), vpn_gateway_id()])
}

/// Egress Only Internet Gateway ID type (e.g., "eigw-12345678")
pub fn egress_only_internet_gateway_id() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("EgressOnlyInternetGatewayId".to_string()),
        pattern: Some("^eigw-[0-9a-f]{8,}$".to_string()),
        length: None,
        base: Box::new(aws_resource_id()),
        validate: |value| {
            if let Value::String(s) = value {
                validate_prefixed_resource_id(s, "eigw").map_err(|reason| {
                    format!(
                        "Invalid Egress Only Internet Gateway ID '{}': {}",
                        s, reason
                    )
                })
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

/// VPC Endpoint ID type (e.g., "vpce-12345678")
pub fn vpc_endpoint_id() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("VpcEndpointId".to_string()),
        pattern: Some("^vpce-[0-9a-f]{8,}$".to_string()),
        length: None,
        base: Box::new(aws_resource_id()),
        validate: |value| {
            if let Value::String(s) = value {
                validate_prefixed_resource_id(s, "vpce")
                    .map_err(|reason| format!("Invalid VPC Endpoint ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

/// Instance ID type (e.g., "i-0123456789abcdef0")
pub fn instance_id() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("InstanceId".to_string()),
        pattern: Some("^i-[0-9a-f]{8,}$".to_string()),
        length: None,
        base: Box::new(aws_resource_id()),
        validate: |value| {
            if let Value::String(s) = value {
                validate_prefixed_resource_id(s, "i")
                    .map_err(|reason| format!("Invalid Instance ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

/// Network Interface ID type (e.g., "eni-0123456789abcdef0")
pub fn network_interface_id() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("NetworkInterfaceId".to_string()),
        pattern: Some("^eni-[0-9a-f]{8,}$".to_string()),
        length: None,
        base: Box::new(aws_resource_id()),
        validate: |value| {
            if let Value::String(s) = value {
                validate_prefixed_resource_id(s, "eni")
                    .map_err(|reason| format!("Invalid Network Interface ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

/// EIP Allocation ID type (e.g., "eipalloc-0123456789abcdef0")
#[allow(dead_code)]
pub fn allocation_id() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("AllocationId".to_string()),
        pattern: Some("^eipalloc-[0-9a-f]{8,}$".to_string()),
        length: None,
        base: Box::new(aws_resource_id()),
        validate: |value| {
            if let Value::String(s) = value {
                validate_prefixed_resource_id(s, "eipalloc")
                    .map_err(|reason| format!("Invalid Allocation ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

/// Prefix List ID type (e.g., "pl-0123456789abcdef0")
pub fn prefix_list_id() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("PrefixListId".to_string()),
        pattern: Some("^pl-[0-9a-f]{8,}$".to_string()),
        length: None,
        base: Box::new(aws_resource_id()),
        validate: |value| {
            if let Value::String(s) = value {
                validate_prefixed_resource_id(s, "pl")
                    .map_err(|reason| format!("Invalid Prefix List ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

/// Carrier Gateway ID type (e.g., "cagw-0123456789abcdef0")
pub fn carrier_gateway_id() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("CarrierGatewayId".to_string()),
        pattern: Some("^cagw-[0-9a-f]{8,}$".to_string()),
        length: None,
        base: Box::new(aws_resource_id()),
        validate: |value| {
            if let Value::String(s) = value {
                validate_prefixed_resource_id(s, "cagw")
                    .map_err(|reason| format!("Invalid Carrier Gateway ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

/// Local Gateway ID type (e.g., "lgw-0123456789abcdef0")
pub fn local_gateway_id() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("LocalGatewayId".to_string()),
        pattern: Some("^lgw-[0-9a-f]{8,}$".to_string()),
        length: None,
        base: Box::new(aws_resource_id()),
        validate: |value| {
            if let Value::String(s) = value {
                validate_prefixed_resource_id(s, "lgw")
                    .map_err(|reason| format!("Invalid Local Gateway ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

/// Network ACL ID type (e.g., "acl-0123456789abcdef0")
#[allow(dead_code)]
pub fn network_acl_id() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("NetworkAclId".to_string()),
        pattern: Some("^acl-[0-9a-f]{8,}$".to_string()),
        length: None,
        base: Box::new(aws_resource_id()),
        validate: |value| {
            if let Value::String(s) = value {
                validate_prefixed_resource_id(s, "acl")
                    .map_err(|reason| format!("Invalid Network ACL ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

/// Transit Gateway Attachment ID type (e.g., "tgw-attach-0123456789abcdef0")
pub fn transit_gateway_attachment_id() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("TransitGatewayAttachmentId".to_string()),
        pattern: Some("^tgw-attach-[0-9a-f]{8,}$".to_string()),
        length: None,
        base: Box::new(aws_resource_id()),
        validate: |value| {
            if let Value::String(s) = value {
                validate_prefixed_resource_id(s, "tgw-attach").map_err(|reason| {
                    format!("Invalid Transit Gateway Attachment ID '{}': {}", s, reason)
                })
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

/// Flow Log ID type (e.g., "fl-0123456789abcdef0")
pub fn flow_log_id() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("FlowLogId".to_string()),
        pattern: Some("^fl-[0-9a-f]{8,}$".to_string()),
        length: None,
        base: Box::new(aws_resource_id()),
        validate: |value| {
            if let Value::String(s) = value {
                validate_prefixed_resource_id(s, "fl")
                    .map_err(|reason| format!("Invalid Flow Log ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

/// IPAM ID type (e.g., "ipam-0123456789abcdef0")
pub fn ipam_id() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("IpamId".to_string()),
        pattern: Some("^ipam-[0-9a-f]{8,}$".to_string()),
        length: None,
        base: Box::new(aws_resource_id()),
        validate: |value| {
            if let Value::String(s) = value {
                validate_prefixed_resource_id(s, "ipam")
                    .map_err(|reason| format!("Invalid IPAM ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

/// Subnet Route Table Association ID type (e.g., "rtbassoc-0123456789abcdef0")
pub fn subnet_route_table_association_id() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("SubnetRouteTableAssociationId".to_string()),
        pattern: Some("^rtbassoc-[0-9a-f]{8,}$".to_string()),
        length: None,
        base: Box::new(aws_resource_id()),
        validate: |value| {
            if let Value::String(s) = value {
                validate_prefixed_resource_id(s, "rtbassoc").map_err(|reason| {
                    format!(
                        "Invalid Subnet Route Table Association ID '{}': {}",
                        s, reason
                    )
                })
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

/// Security Group Rule ID type (e.g., "sgr-0123456789abcdef0")
pub fn security_group_rule_id() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("SecurityGroupRuleId".to_string()),
        pattern: Some("^sgr-[0-9a-f]{8,}$".to_string()),
        length: None,
        base: Box::new(aws_resource_id()),
        validate: |value| {
            if let Value::String(s) = value {
                validate_prefixed_resource_id(s, "sgr")
                    .map_err(|reason| format!("Invalid Security Group Rule ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

/// Validate IAM Role ID format: starts with "AROA" followed by alphanumeric characters.
pub fn validate_iam_role_id(id: &str) -> Result<(), String> {
    let Some(rest) = id.strip_prefix("AROA") else {
        return Err("must start with 'AROA'".to_string());
    };
    if rest.is_empty() {
        return Err("must have characters after 'AROA' prefix".to_string());
    }
    if !rest.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("characters after prefix must be alphanumeric".to_string());
    }
    Ok(())
}

/// IAM Role ID type (e.g., "AROAEXAMPLEID")
pub fn iam_role_id() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("IamRoleId".to_string()),
        pattern: Some("^AROA[A-Z0-9]+$".to_string()),
        length: None,
        base: Box::new(aws_resource_id()),
        validate: |value| {
            if let Value::String(s) = value {
                validate_iam_role_id(s)
                    .map_err(|reason| format!("Invalid IAM Role ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

// ========== AWS Account ID ==========

/// Validate a 12-digit AWS Account ID.
pub fn validate_aws_account_id(id: &str) -> Result<(), String> {
    if id.len() != 12 {
        return Err(format!(
            "must be exactly 12 digits, got {} characters",
            id.len()
        ));
    }
    if !id.chars().all(|c| c.is_ascii_digit()) {
        return Err("must contain only digits".to_string());
    }
    Ok(())
}

/// AWS Account ID type (12-digit numeric string, e.g., "123456789012")
pub fn aws_account_id() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("AwsAccountId".to_string()),
        pattern: Some("^\\d{12}$".to_string()),
        length: Some((Some(12), Some(12))),
        base: Box::new(AttributeType::String),
        validate: |value| {
            if let Value::String(s) = value {
                validate_aws_account_id(s)
                    .map_err(|reason| format!("Invalid AWS Account ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

// ========== ARN validators ==========

/// Valid AWS partition values.
const VALID_PARTITIONS: &[&str] = &["aws", "aws-cn", "aws-us-gov"];

/// Validate basic ARN format (starts with "arn:", has 6+ colon-separated parts).
/// Enforces valid partition and non-empty service.
pub fn validate_arn(arn: &str) -> Result<(), String> {
    if !arn.starts_with("arn:") {
        return Err("must start with 'arn:'".to_string());
    }
    let parts: Vec<&str> = arn.splitn(6, ':').collect();
    if parts.len() < 6 {
        return Err(
            "must have at least 6 colon-separated parts (arn:partition:service:region:account:resource)".to_string()
        );
    }
    if !VALID_PARTITIONS.contains(&parts[1]) {
        return Err(format!(
            "invalid partition '{}', must be one of: {}",
            parts[1],
            VALID_PARTITIONS.join(", ")
        ));
    }
    // Service must be non-empty (e.g., "s3", "iam", "ec2")
    if parts[2].is_empty() {
        return Err("service must not be empty (e.g., 's3', 'iam', 'ec2')".to_string());
    }
    Ok(())
}

/// Validate an ARN for a specific AWS service and optional resource prefix.
pub fn validate_service_arn(
    arn: &str,
    expected_service: &str,
    resource_prefix: Option<&str>,
) -> Result<(), String> {
    validate_arn(arn)?;
    let parts: Vec<&str> = arn.splitn(6, ':').collect();
    if parts[2] != expected_service {
        return Err(format!(
            "expected {} service, got '{}'",
            expected_service, parts[2]
        ));
    }
    if let Some(prefix) = resource_prefix
        && !parts[5].starts_with(prefix)
    {
        return Err(format!(
            "expected resource starting with '{}', got '{}'",
            prefix, parts[5]
        ));
    }
    Ok(())
}

/// Validate an IAM ARN with strict checks on region, account, and resource name.
///
/// IAM ARNs have the form `arn:{partition}:iam::{account}:{resource_prefix}{name}`.
/// - Region (parts[3]) must be empty
/// - Account (parts[4]) must be `aws` (managed policy) or a 12-digit account ID
/// - Resource name after `resource_prefix` must be non-empty and contain only
///   valid IAM path/name characters
pub fn validate_iam_arn(arn: &str, resource_prefix: &str) -> Result<(), String> {
    // Derive type label from prefix: "policy/" -> "IAM Policy ARN", "role/" -> "IAM Role ARN"
    let resource_type = resource_prefix.trim_end_matches('/');
    let label = format!(
        "IAM {} ARN",
        resource_type
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string() + &resource_type[1..])
            .unwrap_or_default()
    );

    validate_arn(arn)?;
    let parts: Vec<&str> = arn.splitn(6, ':').collect();
    if parts[2] != "iam" {
        return Err(format!(
            "expected {label} but service is '{}' in '{arn}'",
            parts[2]
        ));
    }
    if !parts[3].is_empty() {
        return Err(format!(
            "{label} region must be empty, got '{}' in '{arn}'",
            parts[3]
        ));
    }
    let account = parts[4];
    if account != "aws" && (account.len() != 12 || !account.chars().all(|c| c.is_ascii_digit())) {
        return Err(format!(
            "{label} account must be 'aws' or a 12-digit ID, got '{account}' in '{arn}'"
        ));
    }
    if !parts[5].starts_with(resource_prefix) {
        return Err(format!(
            "{label} resource must begin with '{resource_prefix}', but got '{}' in '{arn}'",
            parts[5]
        ));
    }
    let resource_name = &parts[5][resource_prefix.len()..];
    if resource_name.is_empty() {
        return Err(format!(
            "{label} name after '{resource_prefix}' must not be empty in '{arn}'"
        ));
    }
    // IAM names/paths allow: alphanumeric, plus +, =, ,, ., @, -, _, /
    if !resource_name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "+=,.@-_/".contains(c))
    {
        return Err(format!(
            "{label} name contains invalid characters: '{resource_name}' in '{arn}'"
        ));
    }
    Ok(())
}

/// ARN type (e.g., "arn:aws:s3:::my-bucket")
pub fn arn() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("Arn".to_string()),
        pattern: Some("^arn:(aws|aws-cn|aws-us-gov):[^:]+:.*$".to_string()),
        length: None,
        base: Box::new(AttributeType::String),
        validate: |value| {
            if let Value::String(s) = value {
                validate_arn(s).map_err(|reason| format!("Invalid ARN '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

/// IAM Role ARN type (e.g., "arn:aws:iam::123456789012:role/MyRole")
#[allow(dead_code)]
pub fn iam_role_arn() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("IamRoleArn".to_string()),
        pattern: Some("^arn:(aws|aws-cn|aws-us-gov):iam::[^:]*:role/.+$".to_string()),
        length: None,
        base: Box::new(arn()),
        validate: |value| {
            if let Value::String(s) = value {
                validate_iam_arn(s, "role/")
                    .map_err(|reason| format!("Invalid IAM Role ARN '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

/// IAM Policy ARN type (e.g., "arn:aws:iam::123456789012:policy/MyPolicy")
#[allow(dead_code)]
pub fn iam_policy_arn() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("IamPolicyArn".to_string()),
        pattern: Some("^arn:(aws|aws-cn|aws-us-gov):iam::[^:]*:policy/.+$".to_string()),
        length: None,
        base: Box::new(arn()),
        validate: |value| {
            if let Value::String(s) = value {
                validate_iam_arn(s, "policy/")
                    .map_err(|reason| format!("Invalid IAM Policy ARN '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

/// KMS Key ARN type (e.g., "arn:aws:kms:us-east-1:123456789012:key/...")
#[allow(dead_code)]
pub fn kms_key_arn() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("KmsKeyArn".to_string()),
        pattern: Some("^arn:(aws|aws-cn|aws-us-gov):kms:[^:]*:[^:]*:key/.+$".to_string()),
        length: None,
        base: Box::new(arn()),
        validate: |value| {
            if let Value::String(s) = value {
                validate_service_arn(s, "kms", Some("key/"))
                    .map_err(|reason| format!("Invalid KMS Key ARN '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

// ========== KMS Key ID ==========

/// Check if a string is a valid UUID (8-4-4-4-12 hex digits).
pub fn is_uuid(s: &str) -> bool {
    let expected_lens = [8, 4, 4, 4, 12];
    let parts: Vec<&str> = s.split('-').collect();
    parts.len() == 5
        && parts
            .iter()
            .zip(expected_lens.iter())
            .all(|(part, &len)| part.len() == len && part.chars().all(|c| c.is_ascii_hexdigit()))
}

/// Validate a KMS Key ID (ARN, alias, or UUID format).
pub fn validate_kms_key_id(value: &str) -> Result<(), String> {
    // Accept KMS ARNs with key/ or alias/ resource prefix
    if value.starts_with("arn:") {
        validate_service_arn(value, "kms", None)?;
        let parts: Vec<&str> = value.splitn(6, ':').collect();
        let resource = parts[5];
        if !resource.starts_with("key/") && !resource.starts_with("alias/") {
            return Err(format!(
                "KMS ARN resource '{}' must start with 'key/' or 'alias/'",
                resource
            ));
        }
        return Ok(());
    }
    // Accept alias format: alias/<name>
    if value.starts_with("alias/") {
        if value.len() <= "alias/".len() {
            return Err("missing alias name after 'alias/'".to_string());
        }
        return Ok(());
    }
    // Accept bare key ID (UUID format: 8-4-4-4-12 hex digits)
    if is_uuid(value) {
        return Ok(());
    }
    Err(
        "expected a key ARN, alias ARN, alias name (alias/...), or key ID (UUID format)"
            .to_string(),
    )
}

/// KMS Key ID type - accepts multiple formats:
/// - Key ARN: "arn:aws:kms:us-east-1:123456789012:key/..."
/// - Key alias ARN: "arn:aws:kms:us-east-1:123456789012:alias/my-key"
/// - Key alias: "alias/my-key"
/// - Key ID: "1234abcd-12ab-34cd-56ef-1234567890ab"
#[allow(dead_code)]
pub fn kms_key_id() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("KmsKeyId".to_string()),
        pattern: None,
        length: None,
        base: Box::new(aws_resource_id()),
        validate: |value| {
            if let Value::String(s) = value {
                validate_kms_key_id(s)
                    .map_err(|reason| format!("Invalid KMS key identifier '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

// ========== IPAM types ==========

/// Validate IPAM Pool ID format: `ipam-pool-{hex}` where hex is 8+ hex digits.
pub fn validate_ipam_pool_id(id: &str) -> Result<(), String> {
    let Some(hex_part) = id.strip_prefix("ipam-pool-") else {
        return Err("expected format 'ipam-pool-{hex}'".to_string());
    };
    if hex_part.len() < 8 {
        return Err("hex part must be at least 8 characters".to_string());
    }
    if !hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("hex part must contain only hex digits".to_string());
    }
    Ok(())
}

/// IPAM Pool ID type (e.g., "ipam-pool-0123456789abcdef0")
pub fn ipam_pool_id() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("IpamPoolId".to_string()),
        pattern: Some("^ipam-pool-[0-9a-f]{8,}$".to_string()),
        length: None,
        base: Box::new(aws_resource_id()),
        validate: |value| {
            if let Value::String(s) = value {
                validate_ipam_pool_id(s)
                    .map_err(|reason| format!("Invalid IPAM Pool ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

// ========== Availability Zone ==========

/// Validate availability zone format.
/// Accepts standard AZs (e.g., "us-east-1a"), Local Zones (e.g., "us-east-1-bos-1a"),
/// and Wavelength Zones (e.g., "us-east-1-wl1-bos-wlz-1").
pub fn validate_availability_zone(az: &str) -> Result<(), String> {
    // Must end with a lowercase letter or digit
    let last_char = az.chars().last();
    if !last_char.is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit()) {
        return Err("must end with a zone letter (a-z) or digit".to_string());
    }

    // Split into parts by hyphen
    let parts: Vec<&str> = az.split('-').collect();
    if parts.len() < 3 {
        return Err("expected format like 'us-east-1a'".to_string());
    }

    // All parts must be non-empty and contain only lowercase letters and digits
    for part in &parts {
        if part.is_empty()
            || !part
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        {
            return Err("expected format like 'us-east-1a'".to_string());
        }
    }

    // Must contain at least one part that starts with a digit (region number, possibly
    // with a trailing zone letter like "1a")
    let has_numeric = parts
        .iter()
        .any(|p| p.starts_with(|c: char| c.is_ascii_digit()));
    if !has_numeric {
        return Err("must contain a region number".to_string());
    }

    // A bare region like "us-east-1" (all parts are purely alphabetic or numeric,
    // no part mixes digits and letters) must be rejected. An AZ must either have
    // more parts than a basic region (Local/Wavelength zones) or have a zone letter
    // appended to the numeric part (standard AZ like "1a").
    let has_mixed_part = parts.iter().any(|p| {
        p.chars().any(|c| c.is_ascii_digit()) && p.chars().any(|c| c.is_ascii_lowercase())
    });
    if !has_mixed_part && parts.len() <= 3 {
        return Err("expected availability zone, not region (missing zone suffix)".to_string());
    }

    Ok(())
}

// ========== Availability Zone ID ==========

/// Validate availability zone ID format.
/// AZ IDs use a compact format like "use1-az1", "usw2-az2", "apne1-az4", "euc1-az1".
/// Format: region-abbreviation + number + "-az" + digit(s)
pub fn validate_availability_zone_id(az_id: &str) -> Result<(), String> {
    // Must contain "-az" separator
    let Some(az_pos) = az_id.find("-az") else {
        return Err("must contain '-az' (e.g., 'use1-az1')".to_string());
    };

    let prefix = &az_id[..az_pos];
    let suffix = &az_id[az_pos + 3..]; // after "-az"

    // Prefix must be non-empty and contain only lowercase letters and digits,
    // ending with a digit (the region number)
    if prefix.is_empty() {
        return Err("region prefix must not be empty".to_string());
    }
    if !prefix
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    {
        return Err("region prefix must contain only lowercase letters and digits".to_string());
    }
    if !prefix.chars().last().is_some_and(|c| c.is_ascii_digit()) {
        return Err("region prefix must end with a digit (e.g., 'use1', 'apne1')".to_string());
    }

    // Suffix (after "-az") must be one or more digits
    if suffix.is_empty() || !suffix.chars().all(|c| c.is_ascii_digit()) {
        return Err("AZ number after '-az' must be one or more digits".to_string());
    }

    Ok(())
}

/// Availability Zone ID type (e.g., "use1-az1", "usw2-az2", "apne1-az4")
pub fn availability_zone_id() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("AvailabilityZoneId".to_string()),
        pattern: Some("^[a-z]+[0-9]+-az[0-9]+$".to_string()),
        length: None,
        base: Box::new(AttributeType::String),
        validate: |value| {
            if let Value::String(s) = value {
                validate_availability_zone_id(s)
                    .map_err(|reason| format!("Invalid availability zone ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

// ========== IAM Policy Document ==========

/// String or list of strings type — for IAM policy fields like action, resource
fn string_or_list_of_strings() -> AttributeType {
    AttributeType::Union(vec![
        AttributeType::String,
        AttributeType::list(AttributeType::String),
    ])
}

/// String or principal struct type — for IAM policy principal fields
/// Principal can be either a string (e.g., "*") or a struct with known fields
/// (Service, AWS, Federated) whose values are string or list of strings.
fn string_or_principal_struct() -> AttributeType {
    // Struct must come before String because Union tries members in order,
    // and dsl_value_to_aws's fallthrough to value_to_json would match
    // Value::Map against String incorrectly.
    AttributeType::Union(vec![
        AttributeType::Struct {
            name: "IamPolicyPrincipal".to_string(),
            fields: vec![
                StructField::new("service", string_or_list_of_strings())
                    .with_provider_name("Service"),
                StructField::new("aws", string_or_list_of_strings()).with_provider_name("AWS"),
                StructField::new("federated", string_or_list_of_strings())
                    .with_provider_name("Federated"),
            ],
        },
        AttributeType::String,
    ])
}

/// IAM Policy Effect enum type
/// Only allows "Allow" or "Deny"
fn iam_policy_effect() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("IamPolicyEffect".to_string()),
        pattern: Some("^(Allow|Deny)$".to_string()),
        length: None,
        base: Box::new(AttributeType::String),
        validate: |value| {
            if let Value::String(s) = value {
                match s.as_str() {
                    "Allow" | "Deny" => Ok(()),
                    _ => Err(format!(
                        "Invalid IAM policy effect: \"{}\". Must be \"Allow\" or \"Deny\"",
                        s
                    )),
                }
            } else {
                Err(format!("Expected string, got {:?}", value))
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

/// IAM Policy Document Version enum type
/// Only allows "2012-10-17" or "2008-10-17"
fn iam_policy_version() -> AttributeType {
    AttributeType::Custom {
        semantic_name: Some("IamPolicyVersion".to_string()),
        pattern: Some("^(2012-10-17|2008-10-17)$".to_string()),
        length: None,
        base: Box::new(AttributeType::String),
        validate: |value| {
            if let Value::String(s) = value {
                match s.as_str() {
                    "2012-10-17" | "2008-10-17" => Ok(()),
                    _ => Err(format!(
                        "Invalid IAM policy version: \"{}\". Must be \"2012-10-17\" or \"2008-10-17\"",
                        s
                    )),
                }
            } else {
                Err(format!("Expected string, got {:?}", value))
            }
        },
        namespace: None,
        to_dsl: None,
    }
}

/// IAM condition map type: Map<ConditionOperator, Map<ConditionKey, StringOrList>>
fn condition_type() -> AttributeType {
    let operator_values: Vec<String> = CONDITION_OPERATORS
        .iter()
        .map(|(s, _)| s.to_string())
        .collect();
    AttributeType::map_with_key(
        AttributeType::StringEnum {
            name: "ConditionOperator".to_string(),
            values: operator_values,
            namespace: None,
            to_dsl: None,
        },
        AttributeType::map(string_or_list_of_strings()),
    )
}

/// IAM Policy Statement struct type
fn iam_policy_statement() -> AttributeType {
    AttributeType::Struct {
        name: "IamPolicyStatement".to_string(),
        fields: vec![
            StructField::new("sid", AttributeType::String).with_provider_name("Sid"),
            StructField::new("effect", iam_policy_effect()).with_provider_name("Effect"),
            StructField::new("action", string_or_list_of_strings()).with_provider_name("Action"),
            StructField::new("not_action", string_or_list_of_strings())
                .with_provider_name("NotAction"),
            StructField::new("resource", string_or_list_of_strings())
                .with_provider_name("Resource"),
            StructField::new("not_resource", string_or_list_of_strings())
                .with_provider_name("NotResource"),
            StructField::new("principal", string_or_principal_struct())
                .with_provider_name("Principal"),
            StructField::new("not_principal", string_or_principal_struct())
                .with_provider_name("NotPrincipal"),
            StructField::new("condition", condition_type()).with_provider_name("Condition"),
        ],
    }
}

/// IAM Policy Document type
/// Validates the structure of IAM policy documents (trust policies, inline policies, etc.)
///
/// Uses `Struct` type so that `dsl_value_to_aws` and `aws_value_to_dsl` properly
/// convert between snake_case DSL field names and PascalCase IAM field names
/// (e.g., `version` <-> `Version`, `statement` <-> `Statement`).
pub fn iam_policy_document() -> AttributeType {
    AttributeType::Struct {
        name: "IamPolicyDocument".to_string(),
        fields: vec![
            StructField::new("version", iam_policy_version()).with_provider_name("Version"),
            StructField::new("id", AttributeType::String).with_provider_name("Id"),
            StructField::new("statement", AttributeType::list(iam_policy_statement()))
                .with_provider_name("Statement")
                .with_block_name("statement"),
        ],
    }
}

/// IAM condition operator mappings: (snake_case, PascalCase).
///
/// See <https://docs.aws.amazon.com/IAM/latest/UserGuide/reference_policies_elements_condition_operators.html>
const CONDITION_OPERATORS: &[(&str, &str)] = &[
    // String
    ("string_equals", "StringEquals"),
    ("string_not_equals", "StringNotEquals"),
    ("string_equals_ignore_case", "StringEqualsIgnoreCase"),
    ("string_not_equals_ignore_case", "StringNotEqualsIgnoreCase"),
    ("string_like", "StringLike"),
    ("string_not_like", "StringNotLike"),
    // Numeric
    ("numeric_equals", "NumericEquals"),
    ("numeric_not_equals", "NumericNotEquals"),
    ("numeric_less_than", "NumericLessThan"),
    ("numeric_less_than_equals", "NumericLessThanEquals"),
    ("numeric_greater_than", "NumericGreaterThan"),
    ("numeric_greater_than_equals", "NumericGreaterThanEquals"),
    // Date
    ("date_equals", "DateEquals"),
    ("date_not_equals", "DateNotEquals"),
    ("date_less_than", "DateLessThan"),
    ("date_less_than_equals", "DateLessThanEquals"),
    ("date_greater_than", "DateGreaterThan"),
    ("date_greater_than_equals", "DateGreaterThanEquals"),
    // Boolean
    ("bool", "Bool"),
    // Binary
    ("binary_equals", "BinaryEquals"),
    // IP
    ("ip_address", "IpAddress"),
    ("not_ip_address", "NotIpAddress"),
    // ARN
    ("arn_equals", "ArnEquals"),
    ("arn_not_equals", "ArnNotEquals"),
    ("arn_like", "ArnLike"),
    ("arn_not_like", "ArnNotLike"),
    // Null check
    ("null", "Null"),
];

/// Snake_case prefixes for set operators and their PascalCase AWS form.
const CONDITION_QUALIFIER_PREFIXES: &[(&str, &str)] = &[
    ("for_all_values_", "ForAllValues:"),
    ("for_any_value_", "ForAnyValue:"),
];

/// Convert a snake_case condition operator to its PascalCase AWS form.
/// Returns `None` if the operator is unknown.
///
/// Handles modifiers generically:
/// - `_if_exists` suffix: `string_equals_if_exists` → `StringEqualsIfExists`
/// - `for_all_values_` / `for_any_value_` prefix: `for_all_values_string_like` → `ForAllValues:StringLike`
/// - Combined: `for_all_values_string_like_if_exists` → `ForAllValues:StringLikeIfExists`
pub fn condition_operator_to_aws(snake: &str) -> Option<String> {
    let (rest, if_exists) = match snake.strip_suffix("_if_exists") {
        Some(base) => (base, true),
        None => (snake, false),
    };
    // Check for qualifier prefix (for_all_values_, for_any_value_)
    for (snake_prefix, pascal_prefix) in CONDITION_QUALIFIER_PREFIXES {
        if let Some(base) = rest.strip_prefix(snake_prefix) {
            return CONDITION_OPERATORS
                .iter()
                .find(|(s, _)| *s == base)
                .map(|(_, pascal)| {
                    let suffix = if if_exists { "IfExists" } else { "" };
                    format!("{pascal_prefix}{pascal}{suffix}")
                });
        }
    }
    // Direct operator lookup
    CONDITION_OPERATORS
        .iter()
        .find(|(s, _)| *s == rest)
        .map(|(_, pascal)| {
            if if_exists {
                format!("{pascal}IfExists")
            } else {
                pascal.to_string()
            }
        })
}

/// Convert a PascalCase AWS condition operator to snake_case DSL form.
/// Returns `None` if the operator is unknown.
///
/// Handles modifiers generically:
/// - `IfExists` suffix: `StringEqualsIfExists` → `string_equals_if_exists`
/// - `ForAllValues:` / `ForAnyValue:` prefix: `ForAllValues:StringLike` → `for_all_values_string_like`
/// - Combined: `ForAllValues:StringLikeIfExists` → `for_all_values_string_like_if_exists`
pub fn condition_operator_to_snake(pascal: &str) -> Option<String> {
    let (rest, if_exists) = match pascal.strip_suffix("IfExists") {
        Some(base) => (base, true),
        None => (pascal, false),
    };
    // Check for qualifier prefix (ForAllValues:, ForAnyValue:)
    for (snake_prefix, pascal_prefix) in CONDITION_QUALIFIER_PREFIXES {
        if let Some(base) = rest.strip_prefix(pascal_prefix) {
            return CONDITION_OPERATORS
                .iter()
                .find(|(_, p)| *p == base)
                .map(|(snake, _)| {
                    let suffix = if if_exists { "_if_exists" } else { "" };
                    format!("{snake_prefix}{snake}{suffix}")
                });
        }
    }
    // Direct operator lookup
    CONDITION_OPERATORS
        .iter()
        .find(|(_, p)| *p == rest)
        .map(|(snake, _)| {
            if if_exists {
                format!("{snake}_if_exists")
            } else {
                snake.to_string()
            }
        })
}

/// Check if a string is a valid snake_case condition operator.
pub fn is_valid_condition_operator(key: &str) -> bool {
    condition_operator_to_aws(key).is_some()
}

/// Validate condition operators in a parsed IAM policy document.
///
/// Walks the document looking for `condition` maps and validates that
/// all operator keys are valid snake_case condition operators.
pub fn validate_condition_operators(value: &Value) -> Result<(), String> {
    let Value::Map(doc) = value else {
        return Ok(());
    };
    // Look for "statement" list
    let Some(Value::List(statements)) = doc.get("statement") else {
        return Ok(());
    };
    for (i, stmt) in statements.iter().enumerate() {
        let Value::Map(stmt_map) = stmt else {
            continue;
        };
        let Some(Value::Map(condition)) = stmt_map.get("condition") else {
            continue;
        };
        for key in condition.keys() {
            if !is_valid_condition_operator(key) {
                let valid_operators: Vec<&str> =
                    CONDITION_OPERATORS.iter().map(|(s, _)| *s).collect();
                return Err(format!(
                    "statement[{}]: unknown condition operator '{}'. \
                     Valid operators: {} \
                     (prefix with for_all_values_ or for_any_value_ for set operators, \
                     append _if_exists for conditional variants)",
                    i,
                    key,
                    valid_operators.join(", ")
                ));
            }
        }
    }
    Ok(())
}

/// Validate IAM policy document structure and condition operators.
pub fn validate_iam_policy_document(value: &Value) -> Result<(), String> {
    iam_policy_document()
        .validate(value)
        .map_err(|e| e.to_string())?;
    validate_condition_operators(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Custom type shape tests

    #[test]
    fn aws_account_id_carries_pattern_and_length() {
        let t = aws_account_id();
        if let AttributeType::Custom {
            semantic_name,
            pattern,
            length,
            ..
        } = t
        {
            assert_eq!(semantic_name.as_deref(), Some("AwsAccountId"));
            assert_eq!(pattern.as_deref(), Some(r"^\d{12}$"));
            assert_eq!(length, Some((Some(12), Some(12))));
        } else {
            panic!("aws_account_id() should be AttributeType::Custom");
        }
    }

    #[test]
    fn vpc_id_carries_semantic_name_and_pattern() {
        let t = vpc_id();
        if let AttributeType::Custom {
            semantic_name,
            pattern,
            ..
        } = t
        {
            assert_eq!(semantic_name.as_deref(), Some("VpcId"));
            assert!(pattern.is_some(), "VpcId should carry a pattern");
        } else {
            panic!("vpc_id() should be AttributeType::Custom");
        }
    }

    // ARN tests

    #[test]
    fn validate_arn_valid() {
        assert!(validate_arn("arn:aws:s3:::my-bucket").is_ok());
        assert!(validate_arn("arn:aws:iam::123456789012:role/MyRole").is_ok());
        assert!(validate_arn("arn:aws-cn:s3:::my-bucket").is_ok());
        assert!(validate_arn("arn:aws:ec2:us-east-1:123456789012:vpc/vpc-1234").is_ok());
    }

    #[test]
    fn validate_arn_invalid() {
        assert!(validate_arn("not-an-arn").is_err());
        assert!(validate_arn("arn:aws:s3").is_err());
        assert!(validate_arn("arn:aws").is_err());
        assert!(validate_arn("").is_err());
    }

    #[test]
    fn validate_arn_rejects_empty_partition() {
        // "arn::::::" has empty partition and service — should be rejected
        assert!(validate_arn("arn::s3:::my-bucket").is_err());
        assert!(validate_arn("arn:::::").is_err());
    }

    #[test]
    fn validate_arn_rejects_empty_service() {
        assert!(validate_arn("arn:aws::::").is_err());
        assert!(validate_arn("arn:aws:::123456789012:resource").is_err());
    }

    #[test]
    fn validate_arn_type_with_value() {
        let t = arn();
        assert!(
            t.validate(&Value::String("arn:aws:s3:::my-bucket".to_string()))
                .is_ok()
        );
        assert!(
            t.validate(&Value::String("not-an-arn".to_string()))
                .is_err()
        );
        assert!(t.validate(&Value::Int(42)).is_err());
        assert!(
            t.validate(&Value::resource_ref(
                "role".to_string(),
                "arn".to_string(),
                vec![]
            ))
            .is_ok()
        );
    }

    // Resource ID tests

    #[test]
    fn validate_aws_resource_id_valid() {
        assert!(validate_aws_resource_id("vpc-1a2b3c4d").is_ok());
        assert!(validate_aws_resource_id("subnet-0123456789abcdef0").is_ok());
        assert!(validate_aws_resource_id("sg-12345678").is_ok());
        assert!(validate_aws_resource_id("rtb-abcdef12").is_ok());
        assert!(validate_aws_resource_id("eipalloc-0123456789abcdef0").is_ok());
        assert!(validate_aws_resource_id("igw-12345678").is_ok());
    }

    #[test]
    fn validate_aws_resource_id_invalid() {
        assert!(validate_aws_resource_id("not-a-valid-id").is_err());
        assert!(validate_aws_resource_id("vpc").is_err());
        assert!(validate_aws_resource_id("vpc-short").is_err());
        assert!(validate_aws_resource_id("vpc-1234567").is_err());
        assert!(validate_aws_resource_id("VPC-12345678").is_err());
    }

    #[test]
    fn validate_aws_resource_id_type_with_value() {
        let t = aws_resource_id();
        assert!(
            t.validate(&Value::String("vpc-1a2b3c4d".to_string()))
                .is_ok()
        );
        assert!(t.validate(&Value::String("vpc".to_string())).is_err());
        assert!(t.validate(&Value::Int(42)).is_err());
        assert!(
            t.validate(&Value::resource_ref(
                "my_vpc".to_string(),
                "vpc_id".to_string(),
                vec![]
            ))
            .is_ok()
        );
    }

    #[test]
    fn validate_vpc_cidr_block_association_id_valid() {
        let t = vpc_cidr_block_association_id();
        assert!(
            t.validate(&Value::String("vpc-cidr-assoc-12345678".to_string()))
                .is_ok()
        );
        assert!(
            t.validate(&Value::String(
                "vpc-cidr-assoc-0123456789abcdef0".to_string()
            ))
            .is_ok()
        );
    }

    #[test]
    fn validate_vpc_cidr_block_association_id_invalid() {
        let t = vpc_cidr_block_association_id();
        assert!(
            t.validate(&Value::String("vpc-12345678".to_string()))
                .is_err()
        );
        assert!(t.validate(&Value::String("invalid".to_string())).is_err());
    }

    #[test]
    fn validate_tgw_route_table_id_valid() {
        let t = tgw_route_table_id();
        assert!(
            t.validate(&Value::String("tgw-rtb-12345678".to_string()))
                .is_ok()
        );
        assert!(
            t.validate(&Value::String("tgw-rtb-0123456789abcdef0".to_string()))
                .is_ok()
        );
    }

    #[test]
    fn validate_tgw_route_table_id_invalid() {
        let t = tgw_route_table_id();
        // Regular route table ID should fail
        assert!(
            t.validate(&Value::String("rtb-12345678".to_string()))
                .is_err()
        );
        // Transit gateway ID should fail
        assert!(
            t.validate(&Value::String("tgw-12345678".to_string()))
                .is_err()
        );
        assert!(t.validate(&Value::String("invalid".to_string())).is_err());
    }

    // Availability zone tests

    #[test]
    fn validate_availability_zone_valid() {
        assert!(validate_availability_zone("us-east-1a").is_ok());
        assert!(validate_availability_zone("ap-northeast-1c").is_ok());
        assert!(validate_availability_zone("eu-central-1b").is_ok());
        assert!(validate_availability_zone("me-south-1a").is_ok());
        assert!(validate_availability_zone("us-west-2d").is_ok());
    }

    #[test]
    fn validate_availability_zone_local_zone() {
        // Local Zones: us-east-1-bos-1a, us-west-2-lax-1a
        assert!(validate_availability_zone("us-east-1-bos-1a").is_ok());
        assert!(validate_availability_zone("us-west-2-lax-1a").is_ok());
        assert!(validate_availability_zone("ap-northeast-1-tpe-1a").is_ok());
    }

    #[test]
    fn validate_availability_zone_wavelength_zone() {
        // Wavelength Zones: us-east-1-wl1-bos-wlz-1
        assert!(validate_availability_zone("us-east-1-wl1-bos-wlz-1").is_ok());
        assert!(validate_availability_zone("us-west-2-wl1-las-wlz-1").is_ok());
    }

    #[test]
    fn validate_availability_zone_invalid() {
        assert!(validate_availability_zone("us-east-1").is_err()); // region, not AZ
        assert!(validate_availability_zone("a").is_err()); // too short
        assert!(validate_availability_zone("invalid").is_err()); // no numeric part
        assert!(validate_availability_zone("us-east").is_err()); // no numeric part
    }

    // Availability zone ID tests

    #[test]
    fn validate_availability_zone_id_valid() {
        assert!(validate_availability_zone_id("use1-az1").is_ok());
        assert!(validate_availability_zone_id("use1-az2").is_ok());
        assert!(validate_availability_zone_id("usw2-az1").is_ok());
        assert!(validate_availability_zone_id("usw2-az4").is_ok());
        assert!(validate_availability_zone_id("apne1-az1").is_ok());
        assert!(validate_availability_zone_id("apne1-az4").is_ok());
        assert!(validate_availability_zone_id("euc1-az1").is_ok());
        assert!(validate_availability_zone_id("aps1-az1").is_ok());
        assert!(validate_availability_zone_id("mes1-az1").is_ok());
        assert!(validate_availability_zone_id("afs1-az1").is_ok());
    }

    #[test]
    fn validate_availability_zone_id_invalid() {
        assert!(validate_availability_zone_id("us-east-1a").is_err()); // AZ name, not ID
        assert!(validate_availability_zone_id("use1").is_err()); // missing -az suffix
        assert!(validate_availability_zone_id("az1").is_err()); // missing region prefix
        assert!(validate_availability_zone_id("").is_err()); // empty
        assert!(validate_availability_zone_id("USE1-AZ1").is_err()); // uppercase
        assert!(validate_availability_zone_id("use-az1").is_err()); // prefix doesn't end with digit
        assert!(validate_availability_zone_id("use1-az").is_err()); // missing AZ number
        assert!(validate_availability_zone_id("use1-azX").is_err()); // non-digit after -az
    }

    #[test]
    fn validate_availability_zone_id_type_with_value() {
        let t = availability_zone_id();
        assert!(t.validate(&Value::String("use1-az1".to_string())).is_ok());
        assert!(
            t.validate(&Value::String("us-east-1a".to_string()))
                .is_err()
        );
        assert!(t.validate(&Value::Int(42)).is_err());
        assert!(
            t.validate(&Value::resource_ref(
                "subnet".to_string(),
                "availability_zone_id".to_string(),
                vec![]
            ))
            .is_ok()
        );
    }

    // Enum helpers

    #[test]
    fn find_matching_enum_value_exact() {
        let values = &["Enabled", "Suspended"];
        assert_eq!(find_matching_enum_value("Enabled", values), Some("Enabled"));
        assert_eq!(find_matching_enum_value("Missing", values), None);
    }

    #[test]
    fn find_matching_enum_value_case_insensitive() {
        let values = &["Enabled", "Suspended"];
        assert_eq!(find_matching_enum_value("enabled", values), Some("Enabled"));
    }

    #[test]
    fn find_matching_enum_value_underscore_to_hyphen() {
        let values = &["us-east-1", "eu-west-1"];
        assert_eq!(
            find_matching_enum_value("us_east_1", values),
            Some("us-east-1")
        );
    }

    #[test]
    fn canonicalize_enum_value_matches() {
        assert_eq!(
            canonicalize_enum_value("enabled", &["Enabled", "Suspended"]),
            "Enabled"
        );
    }

    #[test]
    fn canonicalize_enum_value_no_match() {
        assert_eq!(
            canonicalize_enum_value("unknown", &["Enabled", "Suspended"]),
            "unknown"
        );
    }

    // IPAM Pool ID tests

    #[test]
    fn validate_ipam_pool_id_valid() {
        assert!(validate_ipam_pool_id("ipam-pool-0123456789abcdef0").is_ok());
        assert!(validate_ipam_pool_id("ipam-pool-12345678").is_ok());
    }

    #[test]
    fn validate_ipam_pool_id_invalid() {
        assert!(validate_ipam_pool_id("ipam-pool-short").is_err());
        assert!(validate_ipam_pool_id("not-ipam-pool").is_err());
        assert!(validate_ipam_pool_id("ipam-pool-").is_err());
    }

    // AWS Account ID tests

    #[test]
    fn validate_aws_account_id_valid() {
        assert!(validate_aws_account_id("123456789012").is_ok());
    }

    #[test]
    fn validate_aws_account_id_invalid() {
        assert!(validate_aws_account_id("1234").is_err());
        assert!(validate_aws_account_id("12345678901a").is_err());
        assert!(validate_aws_account_id("").is_err());
    }

    // KMS Key ID tests

    #[test]
    fn validate_kms_key_id_arn() {
        assert!(
            validate_kms_key_id(
                "arn:aws:kms:us-east-1:123456789012:key/1234abcd-12ab-34cd-56ef-1234567890ab"
            )
            .is_ok()
        );
        assert!(validate_kms_key_id("arn:aws:kms:us-east-1:123456789012:alias/my-key").is_ok());
    }

    #[test]
    fn validate_kms_key_id_alias() {
        assert!(validate_kms_key_id("alias/my-key").is_ok());
        assert!(validate_kms_key_id("alias/").is_err());
    }

    #[test]
    fn validate_kms_key_id_uuid() {
        assert!(validate_kms_key_id("1234abcd-12ab-34cd-56ef-1234567890ab").is_ok());
        assert!(validate_kms_key_id("not-a-uuid").is_err());
    }

    // Service ARN tests

    #[test]
    fn validate_service_arn_valid() {
        assert!(
            validate_service_arn(
                "arn:aws:iam::123456789012:role/MyRole",
                "iam",
                Some("role/")
            )
            .is_ok()
        );
    }

    #[test]
    fn validate_service_arn_wrong_service() {
        assert!(validate_service_arn("arn:aws:s3:::bucket", "iam", None).is_err());
    }

    #[test]
    fn validate_service_arn_wrong_prefix() {
        assert!(
            validate_service_arn(
                "arn:aws:iam::123456789012:user/MyUser",
                "iam",
                Some("role/")
            )
            .is_err()
        );
    }

    // --- validate_arn partition tests ---

    #[test]
    fn validate_arn_rejects_invalid_partition() {
        assert!(validate_arn("arn:xxx:iam::aws:policy/Foo").is_err());
        assert!(validate_arn("arn:invalid:s3:::bucket").is_err());
    }

    #[test]
    fn validate_arn_accepts_valid_partitions() {
        assert!(validate_arn("arn:aws:s3:::bucket").is_ok());
        assert!(validate_arn("arn:aws-cn:s3:::bucket").is_ok());
        assert!(validate_arn("arn:aws-us-gov:s3:::bucket").is_ok());
    }

    // --- IAM ARN validation tests ---

    #[test]
    fn validate_iam_arn_rejects_non_empty_region() {
        assert!(validate_iam_arn("arn:aws:iam:us-east-1:aws:policy/Foo", "policy/").is_err());
    }

    #[test]
    fn validate_iam_arn_rejects_short_account_id() {
        assert!(validate_iam_arn("arn:aws:iam::1234:policy/Foo", "policy/").is_err());
    }

    #[test]
    fn validate_iam_arn_rejects_non_digit_account() {
        assert!(validate_iam_arn("arn:aws:iam::aw:policy/Foo", "policy/").is_err());
    }

    #[test]
    fn validate_iam_arn_accepts_aws_managed() {
        assert!(validate_iam_arn("arn:aws:iam::aws:policy/AdministratorAccess", "policy/").is_ok());
    }

    #[test]
    fn validate_iam_arn_accepts_customer_managed() {
        assert!(validate_iam_arn("arn:aws:iam::123456789012:policy/MyPolicy", "policy/").is_ok());
    }

    #[test]
    fn validate_iam_arn_rejects_empty_resource_name() {
        assert!(validate_iam_arn("arn:aws:iam::aws:policy/", "policy/").is_err());
    }

    #[test]
    fn validate_iam_arn_rejects_invalid_resource_chars() {
        assert!(validate_iam_arn("arn:aws:iam::aws:policy/My Policy", "policy/").is_err());
        assert!(validate_iam_arn("arn:aws:iam::aws:policy/foo<bar>", "policy/").is_err());
    }

    #[test]
    fn validate_iam_arn_accepts_path_prefix() {
        assert!(
            validate_iam_arn(
                "arn:aws:iam::123456789012:role/service-role/MyRole",
                "role/"
            )
            .is_ok()
        );
    }

    #[test]
    fn validate_iam_arn_error_says_iam_policy_arn() {
        let err = validate_iam_arn("arn:aws:iam:us-east-1:aws:policy/Foo", "policy/").unwrap_err();
        assert!(
            err.contains("IAM Policy ARN"),
            "Error should say 'IAM Policy ARN': {err}"
        );
        assert!(
            err.contains("arn:aws:iam:us-east-1:aws:policy/Foo"),
            "Error should include full ARN: {err}"
        );
    }

    #[test]
    fn validate_iam_arn_error_says_iam_role_arn() {
        let err = validate_iam_arn("arn:aws:iam:us-east-1:aws:role/Foo", "role/").unwrap_err();
        assert!(
            err.contains("IAM Role ARN"),
            "Error should say 'IAM Role ARN': {err}"
        );
    }

    // UUID tests

    #[test]
    fn is_uuid_valid() {
        assert!(is_uuid("1234abcd-12ab-34cd-56ef-1234567890ab"));
    }

    #[test]
    fn is_uuid_invalid() {
        assert!(!is_uuid("not-a-uuid"));
        assert!(!is_uuid("1234abcd-12ab-34cd-56ef"));
        assert!(!is_uuid(""));
    }

    // IAM Policy Document tests

    #[test]
    fn validate_iam_policy_document_basic() {
        let doc = Value::Map(
            vec![
                (
                    "version".to_string(),
                    Value::String("2012-10-17".to_string()),
                ),
                (
                    "statement".to_string(),
                    Value::List(vec![Value::Map(
                        vec![
                            ("effect".to_string(), Value::String("Allow".to_string())),
                            (
                                "action".to_string(),
                                Value::String("sts:AssumeRole".to_string()),
                            ),
                            ("resource".to_string(), Value::String("*".to_string())),
                        ]
                        .into_iter()
                        .collect(),
                    )]),
                ),
            ]
            .into_iter()
            .collect(),
        );
        assert!(validate_iam_policy_document(&doc).is_ok());
    }

    #[test]
    fn validate_iam_policy_document_invalid_version() {
        let doc = Value::Map(
            vec![(
                "version".to_string(),
                Value::String("2020-01-01".to_string()),
            )]
            .into_iter()
            .collect(),
        );
        assert!(validate_iam_policy_document(&doc).is_err());
    }

    #[test]
    fn validate_iam_policy_document_invalid_effect() {
        let doc = Value::Map(
            vec![(
                "statement".to_string(),
                Value::List(vec![Value::Map(
                    vec![("effect".to_string(), Value::String("Grant".to_string()))]
                        .into_iter()
                        .collect(),
                )]),
            )]
            .into_iter()
            .collect(),
        );
        assert!(validate_iam_policy_document(&doc).is_err());
    }

    #[test]
    fn iam_policy_document_type_validates() {
        let t = iam_policy_document();
        let valid_doc = Value::Map(
            vec![
                (
                    "version".to_string(),
                    Value::String("2012-10-17".to_string()),
                ),
                (
                    "statement".to_string(),
                    Value::List(vec![Value::Map(
                        vec![
                            ("effect".to_string(), Value::String("Deny".to_string())),
                            ("action".to_string(), Value::String("s3:*".to_string())),
                            ("resource".to_string(), Value::String("*".to_string())),
                        ]
                        .into_iter()
                        .collect(),
                    )]),
                ),
            ]
            .into_iter()
            .collect(),
        );
        assert!(t.validate(&valid_doc).is_ok());
    }

    #[test]
    fn iam_policy_document_principal_map_validates() {
        let t = iam_policy_document();
        // principal as a map: { service = "ec2.amazonaws.com" }
        let doc_with_principal_map = Value::Map(
            vec![
                (
                    "version".to_string(),
                    Value::String("2012-10-17".to_string()),
                ),
                (
                    "statement".to_string(),
                    Value::List(vec![Value::Map(
                        vec![
                            ("effect".to_string(), Value::String("Allow".to_string())),
                            (
                                "principal".to_string(),
                                Value::Map(
                                    vec![(
                                        "service".to_string(),
                                        Value::String("ec2.amazonaws.com".to_string()),
                                    )]
                                    .into_iter()
                                    .collect(),
                                ),
                            ),
                            (
                                "action".to_string(),
                                Value::String("sts:AssumeRole".to_string()),
                            ),
                        ]
                        .into_iter()
                        .collect(),
                    )]),
                ),
            ]
            .into_iter()
            .collect(),
        );
        assert!(
            t.validate(&doc_with_principal_map).is_ok(),
            "principal as map (struct) should be valid: {:?}",
            t.validate(&doc_with_principal_map)
        );
    }

    #[test]
    fn iam_policy_document_principal_string_validates() {
        let t = iam_policy_document();
        // principal as a string: "*"
        let doc_with_principal_string = Value::Map(
            vec![
                (
                    "version".to_string(),
                    Value::String("2012-10-17".to_string()),
                ),
                (
                    "statement".to_string(),
                    Value::List(vec![Value::Map(
                        vec![
                            ("effect".to_string(), Value::String("Allow".to_string())),
                            ("principal".to_string(), Value::String("*".to_string())),
                            (
                                "action".to_string(),
                                Value::String("sts:AssumeRole".to_string()),
                            ),
                        ]
                        .into_iter()
                        .collect(),
                    )]),
                ),
            ]
            .into_iter()
            .collect(),
        );
        assert!(
            t.validate(&doc_with_principal_string).is_ok(),
            "principal as string should be valid: {:?}",
            t.validate(&doc_with_principal_string)
        );
    }

    #[test]
    fn transit_gateway_attachment_id_valid() {
        assert!(
            validate_prefixed_resource_id("tgw-attach-0123456789abcdef0", "tgw-attach").is_ok()
        );
    }

    #[test]
    fn transit_gateway_attachment_id_invalid() {
        assert!(validate_prefixed_resource_id("tgw-0123456789abcdef0", "tgw-attach").is_err());
    }

    #[test]
    fn flow_log_id_valid() {
        assert!(validate_prefixed_resource_id("fl-0123456789abcdef0", "fl").is_ok());
    }

    #[test]
    fn flow_log_id_invalid() {
        assert!(validate_prefixed_resource_id("fl-xyz", "fl").is_err());
    }

    #[test]
    fn ipam_id_valid() {
        assert!(validate_prefixed_resource_id("ipam-0123456789abcdef0", "ipam").is_ok());
    }

    #[test]
    fn ipam_id_invalid() {
        assert!(validate_prefixed_resource_id("ipam-pool-0123456789abcdef0", "ipam").is_err());
    }

    #[test]
    fn subnet_route_table_association_id_valid() {
        assert!(validate_prefixed_resource_id("rtbassoc-0123456789abcdef0", "rtbassoc").is_ok());
    }

    #[test]
    fn security_group_rule_id_valid() {
        assert!(validate_prefixed_resource_id("sgr-0123456789abcdef0", "sgr").is_ok());
    }

    #[test]
    fn security_group_rule_id_invalid() {
        assert!(validate_prefixed_resource_id("sg-0123456789abcdef0", "sgr").is_err());
    }

    #[test]
    fn iam_role_id_valid() {
        assert!(validate_iam_role_id("AROAEXAMPLEID123").is_ok());
        assert!(validate_iam_role_id("AROA1234567890ABCDEF").is_ok());
    }

    #[test]
    fn iam_role_id_invalid_prefix() {
        assert!(validate_iam_role_id("AIDA1234567890ABCDEF").is_err());
    }

    #[test]
    fn iam_role_id_invalid_empty_after_prefix() {
        assert!(validate_iam_role_id("AROA").is_err());
    }

    // Region completion tests

    #[test]
    fn region_completions_generates_dsl_format() {
        let completions = region_completions("aws");
        assert_eq!(completions.len(), REGIONS.len());
        // Spot-check a few entries
        assert_eq!(completions[0].value, "aws.Region.af_south_1");
        assert_eq!(completions[0].description, "Africa (Cape Town)");
        let tokyo = completions
            .iter()
            .find(|c| c.value.contains("ap_northeast_1"))
            .unwrap();
        assert_eq!(tokyo.description, "Asia Pacific (Tokyo)");
    }

    #[test]
    fn region_completions_uses_provider_prefix() {
        let aws = region_completions("aws");
        let awscc = region_completions("awscc");
        assert!(aws[0].value.starts_with("aws.Region."));
        assert!(awscc[0].value.starts_with("awscc.Region."));
    }

    #[test]
    fn validate_tags_map_detects_key_value_pattern() {
        let mut attrs = std::collections::HashMap::new();
        attrs.insert(
            "tags".to_string(),
            Value::Map(
                [
                    ("key".to_string(), Value::String("Project".to_string())),
                    ("value".to_string(), Value::String("carina".to_string())),
                ]
                .into_iter()
                .collect(),
            ),
        );
        assert!(validate_tags_map(&attrs).is_err());
    }

    #[test]
    fn validate_tags_map_case_insensitive() {
        let mut attrs = std::collections::HashMap::new();
        attrs.insert(
            "tags".to_string(),
            Value::Map(
                [
                    ("Key".to_string(), Value::String("Project".to_string())),
                    ("Value".to_string(), Value::String("carina".to_string())),
                ]
                .into_iter()
                .collect(),
            ),
        );
        assert!(validate_tags_map(&attrs).is_err());
    }

    #[test]
    fn validate_tags_map_normal_tags_ok() {
        let mut attrs = std::collections::HashMap::new();
        attrs.insert(
            "tags".to_string(),
            Value::Map(
                [
                    ("Project".to_string(), Value::String("carina".to_string())),
                    ("ManagedBy".to_string(), Value::String("carina".to_string())),
                ]
                .into_iter()
                .collect(),
            ),
        );
        assert!(validate_tags_map(&attrs).is_ok());
    }

    #[test]
    fn validate_tags_map_no_tags_ok() {
        let attrs = std::collections::HashMap::new();
        assert!(validate_tags_map(&attrs).is_ok());
    }

    // --- Condition operator tests ---

    #[test]
    fn condition_operator_to_aws_basic() {
        assert_eq!(
            condition_operator_to_aws("string_equals"),
            Some("StringEquals".to_string())
        );
        assert_eq!(
            condition_operator_to_aws("arn_like"),
            Some("ArnLike".to_string())
        );
        assert_eq!(condition_operator_to_aws("null"), Some("Null".to_string()));
    }

    #[test]
    fn condition_operator_to_aws_if_exists() {
        assert_eq!(
            condition_operator_to_aws("string_equals_if_exists"),
            Some("StringEqualsIfExists".to_string())
        );
        assert_eq!(
            condition_operator_to_aws("arn_like_if_exists"),
            Some("ArnLikeIfExists".to_string())
        );
    }

    #[test]
    fn condition_operator_to_aws_unknown() {
        assert_eq!(condition_operator_to_aws("unknown_op"), None);
        assert_eq!(condition_operator_to_aws("StringEquals"), None);
    }

    #[test]
    fn condition_operator_to_aws_for_all_values() {
        assert_eq!(
            condition_operator_to_aws("for_all_values_string_equals"),
            Some("ForAllValues:StringEquals".to_string())
        );
        assert_eq!(
            condition_operator_to_aws("for_any_value_string_like"),
            Some("ForAnyValue:StringLike".to_string())
        );
        // Any base operator should work with qualifiers
        assert_eq!(
            condition_operator_to_aws("for_all_values_numeric_equals"),
            Some("ForAllValues:NumericEquals".to_string())
        );
        // Combined qualifier + if_exists
        assert_eq!(
            condition_operator_to_aws("for_all_values_string_like_if_exists"),
            Some("ForAllValues:StringLikeIfExists".to_string())
        );
    }

    #[test]
    fn condition_operator_to_snake_roundtrip() {
        assert_eq!(
            condition_operator_to_snake("ForAllValues:NumericEquals"),
            Some("for_all_values_numeric_equals".to_string())
        );
        assert_eq!(
            condition_operator_to_snake("ForAnyValue:ArnLikeIfExists"),
            Some("for_any_value_arn_like_if_exists".to_string())
        );
    }

    #[test]
    fn validate_condition_operators_accepts_valid() {
        let doc = Value::Map(
            vec![(
                "statement".to_string(),
                Value::List(vec![Value::Map(
                    vec![(
                        "condition".to_string(),
                        Value::Map(
                            vec![(
                                "string_equals".to_string(),
                                Value::Map(
                                    vec![(
                                        "aws:RequestedRegion".to_string(),
                                        Value::String("us-east-1".to_string()),
                                    )]
                                    .into_iter()
                                    .collect(),
                                ),
                            )]
                            .into_iter()
                            .collect(),
                        ),
                    )]
                    .into_iter()
                    .collect(),
                )]),
            )]
            .into_iter()
            .collect(),
        );
        assert!(validate_condition_operators(&doc).is_ok());
    }

    #[test]
    fn validate_condition_operators_rejects_pascal_case() {
        let doc = Value::Map(
            vec![(
                "statement".to_string(),
                Value::List(vec![Value::Map(
                    vec![(
                        "condition".to_string(),
                        Value::Map(
                            vec![(
                                "StringEquals".to_string(),
                                Value::Map(std::collections::HashMap::new()),
                            )]
                            .into_iter()
                            .collect(),
                        ),
                    )]
                    .into_iter()
                    .collect(),
                )]),
            )]
            .into_iter()
            .collect(),
        );
        let err = validate_condition_operators(&doc).unwrap_err();
        assert!(
            err.contains("StringEquals"),
            "Error should mention the invalid key: {err}"
        );
    }

    #[test]
    fn validate_condition_operators_rejects_unknown() {
        let doc = Value::Map(
            vec![(
                "statement".to_string(),
                Value::List(vec![Value::Map(
                    vec![(
                        "condition".to_string(),
                        Value::Map(
                            vec![(
                                "foo_bar".to_string(),
                                Value::Map(std::collections::HashMap::new()),
                            )]
                            .into_iter()
                            .collect(),
                        ),
                    )]
                    .into_iter()
                    .collect(),
                )]),
            )]
            .into_iter()
            .collect(),
        );
        assert!(validate_condition_operators(&doc).is_err());
    }

    #[test]
    fn validate_condition_operators_accepts_if_exists() {
        let doc = Value::Map(
            vec![(
                "statement".to_string(),
                Value::List(vec![Value::Map(
                    vec![(
                        "condition".to_string(),
                        Value::Map(
                            vec![(
                                "string_equals_if_exists".to_string(),
                                Value::Map(std::collections::HashMap::new()),
                            )]
                            .into_iter()
                            .collect(),
                        ),
                    )]
                    .into_iter()
                    .collect(),
                )]),
            )]
            .into_iter()
            .collect(),
        );
        assert!(validate_condition_operators(&doc).is_ok());
    }
}
