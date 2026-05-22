//! Shared AWS type definitions and validators
//!
//! This module contains type validators shared between `carina-provider-aws`
//! and `carina-provider-awscc`. Provider-specific types (region with namespace,
//! schema config structs) remain in their respective crates.

use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{AttributeType, CompletionValue, StructField, legacy_validator};

// ========== Enum helpers ==========

/// Convert an AWS API enum value to its DSL (snake_case) spelling.
///
/// Behaviorally identical to `carina-codegen-aws::dsl::dsl_enum_value` and
/// `carina-provider-awscc::bin::codegen::dsl_enum_value`. The three
/// implementations must stay in sync — they collectively define what users
/// type in their `.crn` files. Per the naming-conventions design D7
/// (`carina-rs/carina/docs/specs/2026-04-22-naming-conventions-design.md`):
/// SHOUTY_SNAKE → lowercase; PascalCase → snake_case; kebab → snake;
/// numeric/dotted-numeric pass through unchanged.
fn dsl_enum_value(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    if value.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return value.to_string();
    }
    if value
        .chars()
        .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
        && value.chars().any(|c| c.is_ascii_uppercase())
    {
        return value.to_ascii_lowercase();
    }
    if !value.chars().any(|c| c.is_ascii_uppercase()) {
        // No uppercase letter: kebab and dotted-numeric (e.g.
        // `cloud-watch-logs`, `ipsec.1`) collapse to snake_case so DSL
        // identifiers stay parseable. Dotted-numeric values would
        // otherwise need to be quoted (`'ipsec.1'`), and the strict
        // validator (carina#2980) gates on the DSL spelling.
        return value.replace(['-', '.'], "_");
    }
    // PascalCase / mixed → snake_case (no heck dep, hand-rolled to keep
    // `carina-aws-types` heck-free).
    let mut out = String::with_capacity(value.len() + 4);
    for (i, c) in value.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Build a `dsl_aliases` pair list for a `StringEnum`'s `values`.
///
/// Returns `(api, dsl)` pairs for **every** value, including identity
/// rows where the DSL spelling equals the API spelling. The exhaustive
/// table is what makes the carina-core strict-DSL validator (see
/// `carina-rs/carina#2980`) treat the whole enum uniformly — every
/// enum that has at least one rewrite triggers strict mode for the
/// whole variant set, and identity rows ensure values like
/// `("Subnet", "subnet")` / `("STANDARD_IA", "standard_ia")` accept
/// the DSL spelling cleanly without falling back to the legacy lax
/// canonical-fallback.
fn dsl_aliases_for(values: &[&str]) -> Vec<(String, String)> {
    values
        .iter()
        .map(|v| (v.to_string(), dsl_enum_value(v)))
        .collect()
}

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
    if let Some(Value::Concrete(ConcreteValue::Map(map))) = attributes.get("tags") {
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
        validate: legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_aws_resource_id(s)
                    .map_err(|reason| format!("Invalid resource ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
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
        validate: legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "vpc")
                    .map_err(|reason| format!("Invalid VPC ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
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
        validate: legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "subnet")
                    .map_err(|reason| format!("Invalid Subnet ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
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
        validate: legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "sg")
                    .map_err(|reason| format!("Invalid Security Group ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
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
        validate: legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "igw")
                    .map_err(|reason| format!("Invalid Internet Gateway ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
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
        validate: legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "rtb")
                    .map_err(|reason| format!("Invalid Route Table ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
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
        validate: legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "nat")
                    .map_err(|reason| format!("Invalid NAT Gateway ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
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
        validate: legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "pcx").map_err(|reason| {
                    format!("Invalid VPC Peering Connection ID '{}': {}", s, reason)
                })
            } else {
                Err("Expected string".to_string())
            }
        }),
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
        validate: legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "tgw")
                    .map_err(|reason| format!("Invalid Transit Gateway ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
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
        validate: legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "vpc-cidr-assoc").map_err(|reason| {
                    format!("Invalid VPC CIDR Block Association ID '{}': {}", s, reason)
                })
            } else {
                Err("Expected string".to_string())
            }
        }),
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
        validate: legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "tgw-rtb")
                    .map_err(|reason| format!("Invalid TGW Route Table ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
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
        validate: legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "vgw")
                    .map_err(|reason| format!("Invalid VPN Gateway ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
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
        validate: legacy_validator(|value| {
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
        validate: legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "vpce")
                    .map_err(|reason| format!("Invalid VPC Endpoint ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
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
        validate: legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "i")
                    .map_err(|reason| format!("Invalid Instance ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
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
        validate: legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "eni")
                    .map_err(|reason| format!("Invalid Network Interface ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
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
        validate: legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "eipalloc")
                    .map_err(|reason| format!("Invalid Allocation ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
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
        validate: legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "pl")
                    .map_err(|reason| format!("Invalid Prefix List ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
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
        validate: legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "cagw")
                    .map_err(|reason| format!("Invalid Carrier Gateway ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
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
        validate: legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "lgw")
                    .map_err(|reason| format!("Invalid Local Gateway ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
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
        validate: legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "acl")
                    .map_err(|reason| format!("Invalid Network ACL ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
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
        validate: legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "tgw-attach").map_err(|reason| {
                    format!("Invalid Transit Gateway Attachment ID '{}': {}", s, reason)
                })
            } else {
                Err("Expected string".to_string())
            }
        }),
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
        validate: legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "fl")
                    .map_err(|reason| format!("Invalid Flow Log ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
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
        validate: legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "ipam")
                    .map_err(|reason| format!("Invalid IPAM ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
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
        validate: legacy_validator(|value| {
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
        validate: legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_prefixed_resource_id(s, "sgr")
                    .map_err(|reason| format!("Invalid Security Group Rule ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
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
        validate: legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_iam_role_id(s)
                    .map_err(|reason| format!("Invalid IAM Role ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
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
        validate: legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_aws_account_id(s)
                    .map_err(|reason| format!("Invalid AWS Account ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
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
        validate: legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_arn(s).map_err(|reason| format!("Invalid ARN '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
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
        validate: legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_iam_arn(s, "role/")
                    .map_err(|reason| format!("Invalid IAM Role ARN '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
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
        validate: legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_iam_arn(s, "policy/")
                    .map_err(|reason| format!("Invalid IAM Policy ARN '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
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
        validate: legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_service_arn(s, "kms", Some("key/"))
                    .map_err(|reason| format!("Invalid KMS Key ARN '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
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
        validate: legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_kms_key_id(s)
                    .map_err(|reason| format!("Invalid KMS key identifier '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
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
        validate: legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_ipam_pool_id(s)
                    .map_err(|reason| format!("Invalid IPAM Pool ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
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
        validate: legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_availability_zone_id(s)
                    .map_err(|reason| format!("Invalid availability zone ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
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
                StructField::new("canonical_user", string_or_list_of_strings())
                    .with_provider_name("CanonicalUser"),
            ],
        },
        AttributeType::String,
    ])
}

/// IAM Policy Effect enum type. Allows `Allow` / `Deny` (AWS canonical) and
/// their snake_case DSL aliases `allow` / `deny`, so users can write
/// `effect = allow` as a bare identifier — matching the bare-identifier
/// convention used by every other enum field in the same `.crn` file.
/// The namespace also makes the fully-qualified form
/// `aws.iam.PolicyDocument.Effect.allow` parse and resolve: the
/// resolver's canonical shape is `<namespace>.<type_name>.<value>`, so
/// `type_name` is the trailing `Effect` segment and `namespace` is
/// `aws.iam.PolicyDocument`.
fn iam_policy_effect() -> AttributeType {
    AttributeType::StringEnum {
        name: "Effect".to_string(),
        values: vec!["Allow".to_string(), "Deny".to_string()],
        namespace: Some("aws.iam.PolicyDocument".to_string()),
        dsl_aliases: dsl_aliases_for(&["Allow", "Deny"]),
    }
}

/// IAM Policy Document Version enum type. Allows `2012-10-17` / `2008-10-17`
/// (AWS canonical) with snake_case DSL aliases `2012_10_17` / `2008_10_17`,
/// so users can write `version = 2012_10_17` as a bare identifier —
/// matching the bare-identifier convention `effect = allow` uses in the
/// same `.crn` block. The fully-qualified form
/// `aws.iam.PolicyDocument.Version.2012_10_17` parses via `namespaced_id` +
/// the numeric-tail extension from `carina-rs/carina#3051` and resolves
/// through this namespace: the resolver's canonical shape is
/// `<namespace>.<type_name>.<value>`, so `type_name` is the trailing
/// `Version` segment and `namespace` is `aws.iam.PolicyDocument`.
fn iam_policy_version() -> AttributeType {
    AttributeType::StringEnum {
        name: "Version".to_string(),
        values: vec!["2012-10-17".to_string(), "2008-10-17".to_string()],
        namespace: Some("aws.iam.PolicyDocument".to_string()),
        dsl_aliases: dsl_aliases_for(&["2012-10-17", "2008-10-17"]),
    }
}

/// IAM condition map type: Map<ConditionOperator, Map<ConditionKey, StringOrList>>
///
/// The `ConditionOperator` key set is the full cross-product produced by
/// `all_condition_operator_snake_forms()` — base operators plus the
/// `for_all_values_` / `for_any_value_` qualifier prefixes and the
/// `_if_exists` suffix — so the schema accepts every spelling that
/// `condition_operator_to_aws` already converts. See issue #340.
fn condition_type() -> AttributeType {
    let operator_values: Vec<String> = all_condition_operator_snake_forms();
    let operator_aliases: Vec<(String, String)> = operator_values
        .iter()
        .filter_map(|v| {
            let dsl = dsl_enum_value(v);
            if dsl == *v {
                None
            } else {
                Some((v.clone(), dsl))
            }
        })
        .collect();
    AttributeType::map_with_key(
        AttributeType::StringEnum {
            name: "ConditionOperator".to_string(),
            values: operator_values,
            namespace: None,
            dsl_aliases: operator_aliases,
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

/// SQS dead-letter queue redrive policy. Returned by the SQS API as a
/// stringified JSON document under the `RedrivePolicy` queue attribute,
/// but it is a fixed, narrow shape (target ARN + retry count) rather
/// than an IAM-style policy — so it gets its own Struct rather than
/// reusing `iam_policy_document()`.
pub fn sqs_redrive_policy() -> AttributeType {
    AttributeType::Struct {
        name: "SqsRedrivePolicy".to_string(),
        fields: vec![
            StructField::new("dead_letter_target_arn", arn())
                .with_provider_name("deadLetterTargetArn")
                .required()
                .with_description("ARN of the dead-letter queue to which Amazon SQS moves messages after `max_receive_count` is exceeded."),
            StructField::new("max_receive_count", AttributeType::Int)
                .with_provider_name("maxReceiveCount")
                .required()
                .with_description("Number of times a consumer can receive a message before it is moved to the dead-letter queue. Range 1–1,000."),
        ],
    }
}

/// `redrive_permission` enum used inside `sqs_redrive_allow_policy`.
fn sqs_redrive_permission() -> AttributeType {
    AttributeType::StringEnum {
        name: "SqsRedrivePermission".to_string(),
        values: vec![
            "allowAll".to_string(),
            "denyAll".to_string(),
            "byQueue".to_string(),
        ],
        namespace: Some("aws.sqs.Queue".to_string()),
        dsl_aliases: dsl_aliases_for(&["allowAll", "denyAll", "byQueue"]),
    }
}

/// SQS redrive-allow policy. Controls which source queues may use this
/// queue as their dead-letter destination. Distinct from
/// `iam_policy_document()`: the shape is `{redrivePermission, sourceQueueArns?}`,
/// not a generic IAM statement list.
pub fn sqs_redrive_allow_policy() -> AttributeType {
    AttributeType::Struct {
        name: "SqsRedriveAllowPolicy".to_string(),
        fields: vec![
            StructField::new("redrive_permission", sqs_redrive_permission())
                .with_provider_name("redrivePermission")
                .required()
                .with_description("Which source queues may redrive into this queue. `allowAll`: any queue in the account; `denyAll`: none; `byQueue`: only the ARNs in `source_queue_arns`."),
            StructField::new("source_queue_arns", AttributeType::list(arn()))
                .with_provider_name("sourceQueueArns")
                .with_description("Up to 10 source queue ARNs permitted to redrive into this queue. Required when `redrive_permission` is `byQueue`."),
        ],
    }
}

/// SSE algorithm enum for `aws.s3.BucketServerSideEncryptionConfiguration`.
fn s3_sse_algorithm() -> AttributeType {
    AttributeType::StringEnum {
        name: "SseAlgorithm".to_string(),
        values: vec![
            "AES256".to_string(),
            "aws:kms".to_string(),
            "aws:kms:dsse".to_string(),
        ],
        namespace: Some("aws.s3.BucketServerSideEncryptionConfiguration".to_string()),
        // `aws:kms`/`aws:kms:dsse` carry colons that survive verbatim;
        // only `AES256` (SHOUTY) needs a DSL spelling distinct from the API form.
        dsl_aliases: dsl_aliases_for(&["AES256", "aws:kms", "aws:kms:dsse"]),
    }
}

/// `ApplyServerSideEncryptionByDefault` struct: SSE algorithm + optional KMS key.
fn s3_sse_by_default() -> AttributeType {
    AttributeType::Struct {
        name: "SseByDefault".to_string(),
        fields: vec![
            StructField::new("sse_algorithm", s3_sse_algorithm())
                .with_provider_name("SSEAlgorithm")
                .required(),
            StructField::new("kms_master_key_id", AttributeType::String)
                .with_provider_name("KMSMasterKeyID"),
        ],
    }
}

/// A single SSE rule: per-bucket default + optional bucket-key flag.
fn s3_sse_rule() -> AttributeType {
    AttributeType::Struct {
        name: "SseRule".to_string(),
        fields: vec![
            StructField::new(
                "apply_server_side_encryption_by_default",
                s3_sse_by_default(),
            )
            .with_provider_name("ApplyServerSideEncryptionByDefault")
            .with_block_name("apply_server_side_encryption_by_default"),
            StructField::new("bucket_key_enabled", AttributeType::Bool)
                .with_provider_name("BucketKeyEnabled"),
        ],
    }
}

/// List of SSE rules — the `rules` attribute on
/// `aws.s3.BucketServerSideEncryptionConfiguration`.
pub fn bucket_encryption_rules() -> AttributeType {
    AttributeType::list(s3_sse_rule())
}

/// `Destination` for a replication rule. Minimal fields: target bucket
/// ARN and optional storage class / account.
fn s3_replication_destination() -> AttributeType {
    AttributeType::Struct {
        name: "ReplicationDestination".to_string(),
        fields: vec![
            StructField::new("bucket", AttributeType::String)
                .with_provider_name("Bucket")
                .required(),
            StructField::new("account", AttributeType::String).with_provider_name("Account"),
            StructField::new(
                "storage_class",
                AttributeType::StringEnum {
                    name: "ReplicationStorageClass".to_string(),
                    values: vec![
                        "STANDARD".to_string(),
                        "REDUCED_REDUNDANCY".to_string(),
                        "STANDARD_IA".to_string(),
                        "ONEZONE_IA".to_string(),
                        "INTELLIGENT_TIERING".to_string(),
                        "GLACIER".to_string(),
                        "DEEP_ARCHIVE".to_string(),
                        "GLACIER_IR".to_string(),
                    ],
                    namespace: Some("aws.s3.BucketReplicationConfiguration".to_string()),
                    dsl_aliases: dsl_aliases_for(&[
                        "STANDARD",
                        "REDUCED_REDUNDANCY",
                        "STANDARD_IA",
                        "ONEZONE_IA",
                        "INTELLIGENT_TIERING",
                        "GLACIER",
                        "DEEP_ARCHIVE",
                        "GLACIER_IR",
                    ]),
                },
            )
            .with_provider_name("StorageClass"),
        ],
    }
}

fn s3_replication_status() -> AttributeType {
    AttributeType::StringEnum {
        name: "ReplicationRuleStatus".to_string(),
        values: vec!["Enabled".to_string(), "Disabled".to_string()],
        namespace: Some("aws.s3.BucketReplicationConfiguration".to_string()),
        dsl_aliases: dsl_aliases_for(&["Enabled", "Disabled"]),
    }
}

/// The `And` operator for a replication rule filter — combines a prefix
/// with multiple tags. Required by S3 whenever a filter needs more than
/// one condition.
fn s3_replication_filter_and() -> AttributeType {
    AttributeType::Struct {
        name: "ReplicationRuleAndOperator".to_string(),
        fields: vec![
            StructField::new("prefix", AttributeType::String).with_provider_name("Prefix"),
            StructField::new("tags", AttributeType::list(s3_filter_tag()))
                .with_provider_name("Tags")
                .with_block_name("tag"),
        ],
    }
}

/// Filter selecting which objects a replication rule applies to. A V2
/// replication rule requires a `Filter` element; the provider emits an
/// empty one automatically if this attribute is omitted.
fn s3_replication_rule_filter() -> AttributeType {
    AttributeType::Struct {
        name: "ReplicationRuleFilter".to_string(),
        fields: vec![
            StructField::new("prefix", AttributeType::String).with_provider_name("Prefix"),
            StructField::new("tag", s3_filter_tag())
                .with_provider_name("Tag")
                .with_block_name("tag"),
            StructField::new("and", s3_replication_filter_and())
                .with_provider_name("And")
                .with_block_name("and"),
        ],
    }
}

/// Whether delete markers are replicated. A V2 replication rule that
/// carries a `Filter` must also declare `DeleteMarkerReplication`; the
/// provider defaults it to `Disabled` when omitted.
fn s3_delete_marker_replication() -> AttributeType {
    AttributeType::Struct {
        name: "DeleteMarkerReplication".to_string(),
        fields: vec![
            StructField::new(
                "status",
                AttributeType::StringEnum {
                    name: "DeleteMarkerReplicationStatus".to_string(),
                    values: vec!["Enabled".to_string(), "Disabled".to_string()],
                    namespace: Some("aws.s3.BucketReplicationConfiguration".to_string()),
                    dsl_aliases: dsl_aliases_for(&["Enabled", "Disabled"]),
                },
            )
            .with_provider_name("Status")
            .required(),
        ],
    }
}

fn s3_replication_rule() -> AttributeType {
    AttributeType::Struct {
        name: "ReplicationRule".to_string(),
        fields: vec![
            StructField::new("id", AttributeType::String).with_provider_name("ID"),
            StructField::new("priority", AttributeType::Int).with_provider_name("Priority"),
            StructField::new("filter", s3_replication_rule_filter())
                .with_provider_name("Filter")
                .with_block_name("filter"),
            StructField::new("status", s3_replication_status())
                .with_provider_name("Status")
                .required(),
            StructField::new("delete_marker_replication", s3_delete_marker_replication())
                .with_provider_name("DeleteMarkerReplication")
                .with_block_name("delete_marker_replication"),
            StructField::new("destination", s3_replication_destination())
                .with_provider_name("Destination")
                .with_block_name("destination")
                .required(),
        ],
    }
}

pub fn bucket_replication_rules() -> AttributeType {
    AttributeType::list(s3_replication_rule())
}

/// Lifecycle rule status (Enabled / Disabled).
fn s3_lifecycle_status() -> AttributeType {
    AttributeType::StringEnum {
        name: "LifecycleRuleStatus".to_string(),
        values: vec!["Enabled".to_string(), "Disabled".to_string()],
        namespace: Some("aws.s3.BucketLifecycleConfiguration".to_string()),
        dsl_aliases: dsl_aliases_for(&["Enabled", "Disabled"]),
    }
}

/// Storage class for lifecycle transitions (Glacier / IA / etc.).
fn s3_transition_storage_class() -> AttributeType {
    AttributeType::StringEnum {
        name: "TransitionStorageClass".to_string(),
        values: vec![
            "GLACIER".to_string(),
            "STANDARD_IA".to_string(),
            "ONEZONE_IA".to_string(),
            "INTELLIGENT_TIERING".to_string(),
            "DEEP_ARCHIVE".to_string(),
            "GLACIER_IR".to_string(),
        ],
        namespace: Some("aws.s3.BucketLifecycleConfiguration".to_string()),
        dsl_aliases: dsl_aliases_for(&[
            "GLACIER",
            "STANDARD_IA",
            "ONEZONE_IA",
            "INTELLIGENT_TIERING",
            "DEEP_ARCHIVE",
            "GLACIER_IR",
        ]),
    }
}

fn s3_lifecycle_expiration() -> AttributeType {
    AttributeType::Struct {
        name: "LifecycleExpiration".to_string(),
        fields: vec![
            StructField::new("days", AttributeType::Int).with_provider_name("Days"),
            StructField::new("date", AttributeType::String).with_provider_name("Date"),
            StructField::new("expired_object_delete_marker", AttributeType::Bool)
                .with_provider_name("ExpiredObjectDeleteMarker"),
        ],
    }
}

fn s3_lifecycle_transition() -> AttributeType {
    AttributeType::Struct {
        name: "LifecycleTransition".to_string(),
        fields: vec![
            StructField::new("days", AttributeType::Int).with_provider_name("Days"),
            StructField::new("date", AttributeType::String).with_provider_name("Date"),
            StructField::new("storage_class", s3_transition_storage_class())
                .with_provider_name("StorageClass")
                .required(),
        ],
    }
}

fn s3_noncurrent_version_expiration() -> AttributeType {
    AttributeType::Struct {
        name: "NoncurrentVersionExpiration".to_string(),
        fields: vec![
            StructField::new("noncurrent_days", AttributeType::Int)
                .with_provider_name("NoncurrentDays"),
            StructField::new("newer_noncurrent_versions", AttributeType::Int)
                .with_provider_name("NewerNoncurrentVersions"),
        ],
    }
}

fn s3_noncurrent_version_transition() -> AttributeType {
    AttributeType::Struct {
        name: "NoncurrentVersionTransition".to_string(),
        fields: vec![
            StructField::new("noncurrent_days", AttributeType::Int)
                .with_provider_name("NoncurrentDays"),
            StructField::new("storage_class", s3_transition_storage_class())
                .with_provider_name("StorageClass")
                .required(),
            StructField::new("newer_noncurrent_versions", AttributeType::Int)
                .with_provider_name("NewerNoncurrentVersions"),
        ],
    }
}

fn s3_abort_multipart() -> AttributeType {
    AttributeType::Struct {
        name: "AbortIncompleteMultipartUpload".to_string(),
        fields: vec![
            StructField::new("days_after_initiation", AttributeType::Int)
                .with_provider_name("DaysAfterInitiation"),
        ],
    }
}

/// A single object tag (`key` / `value`) used inside S3 lifecycle and
/// replication rule filters.
fn s3_filter_tag() -> AttributeType {
    AttributeType::Struct {
        name: "FilterTag".to_string(),
        fields: vec![
            StructField::new("key", AttributeType::String)
                .with_provider_name("Key")
                .required(),
            StructField::new("value", AttributeType::String)
                .with_provider_name("Value")
                .required(),
        ],
    }
}

/// The `And` operator for a lifecycle rule filter — combines a prefix,
/// multiple tags, and object-size bounds. Required by S3 whenever a
/// filter needs more than one condition.
fn s3_lifecycle_filter_and() -> AttributeType {
    AttributeType::Struct {
        name: "LifecycleRuleAndOperator".to_string(),
        fields: vec![
            StructField::new("prefix", AttributeType::String).with_provider_name("Prefix"),
            StructField::new("tags", AttributeType::list(s3_filter_tag()))
                .with_provider_name("Tags")
                .with_block_name("tag"),
            StructField::new("object_size_greater_than", AttributeType::Int)
                .with_provider_name("ObjectSizeGreaterThan"),
            StructField::new("object_size_less_than", AttributeType::Int)
                .with_provider_name("ObjectSizeLessThan"),
        ],
    }
}

/// Filter selecting which objects a lifecycle rule applies to. When a
/// rule needs no filter it still requires an empty `<Filter/>` element;
/// the provider emits one automatically if this attribute is omitted.
fn s3_lifecycle_rule_filter() -> AttributeType {
    AttributeType::Struct {
        name: "LifecycleRuleFilter".to_string(),
        fields: vec![
            StructField::new("prefix", AttributeType::String).with_provider_name("Prefix"),
            StructField::new("tag", s3_filter_tag())
                .with_provider_name("Tag")
                .with_block_name("tag"),
            StructField::new("object_size_greater_than", AttributeType::Int)
                .with_provider_name("ObjectSizeGreaterThan"),
            StructField::new("object_size_less_than", AttributeType::Int)
                .with_provider_name("ObjectSizeLessThan"),
            StructField::new("and", s3_lifecycle_filter_and())
                .with_provider_name("And")
                .with_block_name("and"),
        ],
    }
}

fn s3_lifecycle_rule() -> AttributeType {
    AttributeType::Struct {
        name: "LifecycleRule".to_string(),
        fields: vec![
            StructField::new("id", AttributeType::String).with_provider_name("ID"),
            StructField::new("status", s3_lifecycle_status())
                .with_provider_name("Status")
                .required(),
            StructField::new("filter", s3_lifecycle_rule_filter())
                .with_provider_name("Filter")
                .with_block_name("filter"),
            StructField::new("expiration", s3_lifecycle_expiration())
                .with_provider_name("Expiration")
                .with_block_name("expiration"),
            StructField::new(
                "transitions",
                AttributeType::list(s3_lifecycle_transition()),
            )
            .with_provider_name("Transitions")
            .with_block_name("transition"),
            StructField::new(
                "noncurrent_version_expiration",
                s3_noncurrent_version_expiration(),
            )
            .with_provider_name("NoncurrentVersionExpiration")
            .with_block_name("noncurrent_version_expiration"),
            StructField::new(
                "noncurrent_version_transitions",
                AttributeType::list(s3_noncurrent_version_transition()),
            )
            .with_provider_name("NoncurrentVersionTransitions")
            .with_block_name("noncurrent_version_transition"),
            StructField::new("abort_incomplete_multipart_upload", s3_abort_multipart())
                .with_provider_name("AbortIncompleteMultipartUpload")
                .with_block_name("abort_incomplete_multipart_upload"),
        ],
    }
}

pub fn bucket_lifecycle_rules() -> AttributeType {
    AttributeType::list(s3_lifecycle_rule())
}

fn s3_cors_rule() -> AttributeType {
    AttributeType::Struct {
        name: "CorsRule".to_string(),
        fields: vec![
            StructField::new("id", AttributeType::String).with_provider_name("ID"),
            StructField::new(
                "allowed_methods",
                AttributeType::list(AttributeType::String),
            )
            .with_provider_name("AllowedMethods")
            .required(),
            StructField::new(
                "allowed_origins",
                AttributeType::list(AttributeType::String),
            )
            .with_provider_name("AllowedOrigins")
            .required(),
            StructField::new(
                "allowed_headers",
                AttributeType::list(AttributeType::String),
            )
            .with_provider_name("AllowedHeaders"),
            StructField::new("expose_headers", AttributeType::list(AttributeType::String))
                .with_provider_name("ExposeHeaders"),
            StructField::new("max_age_seconds", AttributeType::Int)
                .with_provider_name("MaxAgeSeconds"),
        ],
    }
}

pub fn bucket_cors_rules() -> AttributeType {
    AttributeType::list(s3_cors_rule())
}

fn s3_notification_filter_rule() -> AttributeType {
    AttributeType::Struct {
        name: "FilterRule".to_string(),
        fields: vec![
            StructField::new("name", AttributeType::String)
                .with_provider_name("Name")
                .required(),
            StructField::new("value", AttributeType::String)
                .with_provider_name("Value")
                .required(),
        ],
    }
}

fn s3_notification_filter() -> AttributeType {
    AttributeType::Struct {
        name: "NotificationFilter".to_string(),
        fields: vec![
            StructField::new(
                "filter_rules",
                AttributeType::list(s3_notification_filter_rule()),
            )
            .with_provider_name("FilterRules"),
        ],
    }
}

fn s3_topic_configuration() -> AttributeType {
    AttributeType::Struct {
        name: "TopicConfiguration".to_string(),
        fields: vec![
            StructField::new("id", AttributeType::String).with_provider_name("Id"),
            StructField::new("topic_arn", AttributeType::String)
                .with_provider_name("TopicArn")
                .required(),
            StructField::new("events", AttributeType::list(AttributeType::String))
                .with_provider_name("Events")
                .required(),
            StructField::new("filter", s3_notification_filter()).with_provider_name("Filter"),
        ],
    }
}

fn s3_queue_configuration() -> AttributeType {
    AttributeType::Struct {
        name: "QueueConfiguration".to_string(),
        fields: vec![
            StructField::new("id", AttributeType::String).with_provider_name("Id"),
            StructField::new("queue_arn", AttributeType::String)
                .with_provider_name("QueueArn")
                .required(),
            StructField::new("events", AttributeType::list(AttributeType::String))
                .with_provider_name("Events")
                .required(),
            StructField::new("filter", s3_notification_filter()).with_provider_name("Filter"),
        ],
    }
}

fn s3_lambda_function_configuration() -> AttributeType {
    AttributeType::Struct {
        name: "LambdaFunctionConfiguration".to_string(),
        fields: vec![
            StructField::new("id", AttributeType::String).with_provider_name("Id"),
            StructField::new("lambda_function_arn", AttributeType::String)
                .with_provider_name("LambdaFunctionArn")
                .required(),
            StructField::new("events", AttributeType::list(AttributeType::String))
                .with_provider_name("Events")
                .required(),
            StructField::new("filter", s3_notification_filter()).with_provider_name("Filter"),
        ],
    }
}

pub fn bucket_topic_configurations() -> AttributeType {
    AttributeType::list(s3_topic_configuration())
}

pub fn bucket_queue_configurations() -> AttributeType {
    AttributeType::list(s3_queue_configuration())
}

pub fn bucket_lambda_function_configurations() -> AttributeType {
    AttributeType::list(s3_lambda_function_configuration())
}

pub fn bucket_event_bridge_configuration() -> AttributeType {
    AttributeType::Struct {
        name: "EventBridgeConfiguration".to_string(),
        fields: vec![],
    }
}

fn s3_partition_date_source() -> AttributeType {
    AttributeType::StringEnum {
        name: "PartitionDateSource".to_string(),
        values: vec!["EventTime".to_string(), "DeliveryTime".to_string()],
        namespace: Some("aws.s3.BucketLogging".to_string()),
        dsl_aliases: dsl_aliases_for(&["EventTime", "DeliveryTime"]),
    }
}

fn s3_partitioned_prefix() -> AttributeType {
    AttributeType::Struct {
        name: "PartitionedPrefix".to_string(),
        fields: vec![
            StructField::new("partition_date_source", s3_partition_date_source())
                .with_provider_name("PartitionDateSource"),
        ],
    }
}

fn s3_simple_prefix() -> AttributeType {
    AttributeType::Struct {
        name: "SimplePrefix".to_string(),
        fields: vec![],
    }
}

pub fn bucket_target_object_key_format() -> AttributeType {
    AttributeType::Struct {
        name: "TargetObjectKeyFormat".to_string(),
        fields: vec![
            StructField::new("simple_prefix", s3_simple_prefix())
                .with_provider_name("SimplePrefix"),
            StructField::new("partitioned_prefix", s3_partitioned_prefix())
                .with_provider_name("PartitionedPrefix"),
        ],
    }
}

pub fn s3_index_document() -> AttributeType {
    AttributeType::Struct {
        name: "IndexDocument".to_string(),
        fields: vec![
            StructField::new("suffix", AttributeType::String)
                .with_provider_name("Suffix")
                .required(),
        ],
    }
}

pub fn s3_error_document() -> AttributeType {
    AttributeType::Struct {
        name: "ErrorDocument".to_string(),
        fields: vec![
            StructField::new("key", AttributeType::String)
                .with_provider_name("Key")
                .required(),
        ],
    }
}

pub fn s3_redirect_all_requests_to() -> AttributeType {
    AttributeType::Struct {
        name: "RedirectAllRequestsTo".to_string(),
        fields: vec![
            StructField::new("host_name", AttributeType::String)
                .with_provider_name("HostName")
                .required(),
            StructField::new(
                "protocol",
                AttributeType::StringEnum {
                    name: "Protocol".to_string(),
                    values: vec!["http".to_string(), "https".to_string()],
                    namespace: Some("aws.s3.BucketWebsiteConfiguration".to_string()),
                    // Already lowercase — DSL spelling matches API; no aliases needed.
                    dsl_aliases: vec![],
                },
            )
            .with_provider_name("Protocol"),
        ],
    }
}

/// IAM condition operator — represented as a fully-decomposed sum
/// `(Option<qualifier>) × base × if_exists` so every parseable spelling
/// has exactly one constructible value, and unknown strings can be
/// rejected once at the [`ConditionOperator::from_snake`] /
/// [`ConditionOperator::from_aws`] boundary.
///
/// The cross-product is intentionally permissive: combinations like
/// `for_all_values_null` or `bool_if_exists` are constructible and
/// emitted by [`ConditionOperator::all`] because AWS, not carina, is the
/// authority on what IAM evaluates at apply time. See the doc-comment
/// on [`ConditionOperator::all`] for the rationale.
///
/// The fields are `pub` so callers can pattern-match exhaustively when
/// they need the structural decomposition (e.g. to render or to map
/// across spellings); the schema, validator, and string-wrapper
/// conversion functions all consume this type directly so they cannot
/// drift from each other.
///
/// See <https://docs.aws.amazon.com/IAM/latest/UserGuide/reference_policies_elements_condition_operators.html>
/// and issue #340.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct ConditionOperator {
    pub qualifier: Option<ConditionQualifier>,
    pub base: ConditionOperatorBase,
    pub if_exists: bool,
}

/// Set-aware qualifier prefix on a [`ConditionOperator`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ConditionQualifier {
    ForAllValues,
    ForAnyValue,
}

impl ConditionQualifier {
    /// Every defined qualifier, in the canonical AWS-doc order.
    pub const ALL: &'static [ConditionQualifier] = &[
        ConditionQualifier::ForAllValues,
        ConditionQualifier::ForAnyValue,
    ];

    /// Snake-case prefix written in the DSL (including the trailing `_`).
    pub const fn snake_prefix(self) -> &'static str {
        match self {
            ConditionQualifier::ForAllValues => "for_all_values_",
            ConditionQualifier::ForAnyValue => "for_any_value_",
        }
    }

    /// PascalCase prefix accepted by the AWS IAM API (including the trailing `:`).
    pub const fn aws_prefix(self) -> &'static str {
        match self {
            ConditionQualifier::ForAllValues => "ForAllValues:",
            ConditionQualifier::ForAnyValue => "ForAnyValue:",
        }
    }
}

/// Base IAM condition operator, without any qualifier or `_if_exists` suffix.
///
/// Marked `#[non_exhaustive]` so AWS adding a new operator does not
/// become a downstream SemVer break for code that pattern-matches on this
/// enum. The new variant goes into [`ConditionOperatorBase::ALL`] together
/// with the [`ConditionOperatorBase::snake`] / [`ConditionOperatorBase::aws`]
/// arms — the latter two are exhaustive matches so the compiler enforces
/// they stay complete, but `ALL` is hand-written and must be edited in
/// lockstep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ConditionOperatorBase {
    StringEquals,
    StringNotEquals,
    StringEqualsIgnoreCase,
    StringNotEqualsIgnoreCase,
    StringLike,
    StringNotLike,
    NumericEquals,
    NumericNotEquals,
    NumericLessThan,
    NumericLessThanEquals,
    NumericGreaterThan,
    NumericGreaterThanEquals,
    DateEquals,
    DateNotEquals,
    DateLessThan,
    DateLessThanEquals,
    DateGreaterThan,
    DateGreaterThanEquals,
    Bool,
    BinaryEquals,
    IpAddress,
    NotIpAddress,
    ArnEquals,
    ArnNotEquals,
    ArnLike,
    ArnNotLike,
    Null,
}

impl ConditionOperatorBase {
    /// Every defined base operator, in the canonical AWS-doc order
    /// (String / Numeric / Date / Boolean / Binary / IP / ARN / Null).
    pub const ALL: &'static [ConditionOperatorBase] = &[
        ConditionOperatorBase::StringEquals,
        ConditionOperatorBase::StringNotEquals,
        ConditionOperatorBase::StringEqualsIgnoreCase,
        ConditionOperatorBase::StringNotEqualsIgnoreCase,
        ConditionOperatorBase::StringLike,
        ConditionOperatorBase::StringNotLike,
        ConditionOperatorBase::NumericEquals,
        ConditionOperatorBase::NumericNotEquals,
        ConditionOperatorBase::NumericLessThan,
        ConditionOperatorBase::NumericLessThanEquals,
        ConditionOperatorBase::NumericGreaterThan,
        ConditionOperatorBase::NumericGreaterThanEquals,
        ConditionOperatorBase::DateEquals,
        ConditionOperatorBase::DateNotEquals,
        ConditionOperatorBase::DateLessThan,
        ConditionOperatorBase::DateLessThanEquals,
        ConditionOperatorBase::DateGreaterThan,
        ConditionOperatorBase::DateGreaterThanEquals,
        ConditionOperatorBase::Bool,
        ConditionOperatorBase::BinaryEquals,
        ConditionOperatorBase::IpAddress,
        ConditionOperatorBase::NotIpAddress,
        ConditionOperatorBase::ArnEquals,
        ConditionOperatorBase::ArnNotEquals,
        ConditionOperatorBase::ArnLike,
        ConditionOperatorBase::ArnNotLike,
        ConditionOperatorBase::Null,
    ];

    /// Snake-case DSL spelling of this base operator.
    pub const fn snake(self) -> &'static str {
        match self {
            ConditionOperatorBase::StringEquals => "string_equals",
            ConditionOperatorBase::StringNotEquals => "string_not_equals",
            ConditionOperatorBase::StringEqualsIgnoreCase => "string_equals_ignore_case",
            ConditionOperatorBase::StringNotEqualsIgnoreCase => "string_not_equals_ignore_case",
            ConditionOperatorBase::StringLike => "string_like",
            ConditionOperatorBase::StringNotLike => "string_not_like",
            ConditionOperatorBase::NumericEquals => "numeric_equals",
            ConditionOperatorBase::NumericNotEquals => "numeric_not_equals",
            ConditionOperatorBase::NumericLessThan => "numeric_less_than",
            ConditionOperatorBase::NumericLessThanEquals => "numeric_less_than_equals",
            ConditionOperatorBase::NumericGreaterThan => "numeric_greater_than",
            ConditionOperatorBase::NumericGreaterThanEquals => "numeric_greater_than_equals",
            ConditionOperatorBase::DateEquals => "date_equals",
            ConditionOperatorBase::DateNotEquals => "date_not_equals",
            ConditionOperatorBase::DateLessThan => "date_less_than",
            ConditionOperatorBase::DateLessThanEquals => "date_less_than_equals",
            ConditionOperatorBase::DateGreaterThan => "date_greater_than",
            ConditionOperatorBase::DateGreaterThanEquals => "date_greater_than_equals",
            ConditionOperatorBase::Bool => "bool",
            ConditionOperatorBase::BinaryEquals => "binary_equals",
            ConditionOperatorBase::IpAddress => "ip_address",
            ConditionOperatorBase::NotIpAddress => "not_ip_address",
            ConditionOperatorBase::ArnEquals => "arn_equals",
            ConditionOperatorBase::ArnNotEquals => "arn_not_equals",
            ConditionOperatorBase::ArnLike => "arn_like",
            ConditionOperatorBase::ArnNotLike => "arn_not_like",
            ConditionOperatorBase::Null => "null",
        }
    }

    /// PascalCase AWS-API spelling of this base operator.
    pub const fn aws(self) -> &'static str {
        match self {
            ConditionOperatorBase::StringEquals => "StringEquals",
            ConditionOperatorBase::StringNotEquals => "StringNotEquals",
            ConditionOperatorBase::StringEqualsIgnoreCase => "StringEqualsIgnoreCase",
            ConditionOperatorBase::StringNotEqualsIgnoreCase => "StringNotEqualsIgnoreCase",
            ConditionOperatorBase::StringLike => "StringLike",
            ConditionOperatorBase::StringNotLike => "StringNotLike",
            ConditionOperatorBase::NumericEquals => "NumericEquals",
            ConditionOperatorBase::NumericNotEquals => "NumericNotEquals",
            ConditionOperatorBase::NumericLessThan => "NumericLessThan",
            ConditionOperatorBase::NumericLessThanEquals => "NumericLessThanEquals",
            ConditionOperatorBase::NumericGreaterThan => "NumericGreaterThan",
            ConditionOperatorBase::NumericGreaterThanEquals => "NumericGreaterThanEquals",
            ConditionOperatorBase::DateEquals => "DateEquals",
            ConditionOperatorBase::DateNotEquals => "DateNotEquals",
            ConditionOperatorBase::DateLessThan => "DateLessThan",
            ConditionOperatorBase::DateLessThanEquals => "DateLessThanEquals",
            ConditionOperatorBase::DateGreaterThan => "DateGreaterThan",
            ConditionOperatorBase::DateGreaterThanEquals => "DateGreaterThanEquals",
            ConditionOperatorBase::Bool => "Bool",
            ConditionOperatorBase::BinaryEquals => "BinaryEquals",
            ConditionOperatorBase::IpAddress => "IpAddress",
            ConditionOperatorBase::NotIpAddress => "NotIpAddress",
            ConditionOperatorBase::ArnEquals => "ArnEquals",
            ConditionOperatorBase::ArnNotEquals => "ArnNotEquals",
            ConditionOperatorBase::ArnLike => "ArnLike",
            ConditionOperatorBase::ArnNotLike => "ArnNotLike",
            ConditionOperatorBase::Null => "Null",
        }
    }
}

impl ConditionOperator {
    /// Construct a [`ConditionOperator`] from its three structural pieces.
    /// Always succeeds — the type does not police IAM semantics; AWS does.
    pub const fn new(
        qualifier: Option<ConditionQualifier>,
        base: ConditionOperatorBase,
        if_exists: bool,
    ) -> Self {
        Self {
            qualifier,
            base,
            if_exists,
        }
    }

    /// Snake-case DSL spelling (e.g. `for_all_values_string_equals_if_exists`).
    pub fn to_snake(self) -> String {
        let prefix = self.qualifier.map_or("", ConditionQualifier::snake_prefix);
        let suffix = if self.if_exists { "_if_exists" } else { "" };
        format!("{prefix}{}{suffix}", self.base.snake())
    }

    /// PascalCase AWS-API spelling (e.g. `ForAllValues:StringEqualsIfExists`).
    pub fn to_aws(self) -> String {
        let prefix = self.qualifier.map_or("", ConditionQualifier::aws_prefix);
        let suffix = if self.if_exists { "IfExists" } else { "" };
        format!("{prefix}{}{suffix}", self.base.aws())
    }

    /// Parse a snake-case DSL spelling. Returns `None` if no variant matches.
    pub fn from_snake(snake: &str) -> Option<ConditionOperator> {
        let (rest, if_exists) = match snake.strip_suffix("_if_exists") {
            Some(base) => (base, true),
            None => (snake, false),
        };
        for &q in ConditionQualifier::ALL {
            if let Some(base) = rest.strip_prefix(q.snake_prefix()) {
                return ConditionOperatorBase::ALL
                    .iter()
                    .copied()
                    .find(|b| b.snake() == base)
                    .map(|base| ConditionOperator {
                        qualifier: Some(q),
                        base,
                        if_exists,
                    });
            }
        }
        ConditionOperatorBase::ALL
            .iter()
            .copied()
            .find(|b| b.snake() == rest)
            .map(|base| ConditionOperator {
                qualifier: None,
                base,
                if_exists,
            })
    }

    /// Parse a PascalCase AWS-API spelling. Returns `None` if no variant matches.
    pub fn from_aws(pascal: &str) -> Option<ConditionOperator> {
        let (rest, if_exists) = match pascal.strip_suffix("IfExists") {
            Some(base) => (base, true),
            None => (pascal, false),
        };
        for &q in ConditionQualifier::ALL {
            if let Some(base) = rest.strip_prefix(q.aws_prefix()) {
                return ConditionOperatorBase::ALL
                    .iter()
                    .copied()
                    .find(|b| b.aws() == base)
                    .map(|base| ConditionOperator {
                        qualifier: Some(q),
                        base,
                        if_exists,
                    });
            }
        }
        ConditionOperatorBase::ALL
            .iter()
            .copied()
            .find(|b| b.aws() == rest)
            .map(|base| ConditionOperator {
                qualifier: None,
                base,
                if_exists,
            })
    }

    /// Every `(qualifier, base, if_exists)` cross-product, in a stable order
    /// (outer = base in AWS-doc order, inner = base → `_if_exists` →
    /// qualified bases → qualified `_if_exists`). The schema's
    /// `ConditionOperator` `StringEnum` values and the validator's
    /// "valid operators" suggestion are both derived from this single list,
    /// so they cannot drift from each other or from `from_snake` / `from_aws`.
    ///
    /// The cross-product is unconditional, so semantically-nonsense
    /// combinations like `for_all_values_null` or `bool_if_exists` are also
    /// emitted. Intentional: AWS is the authority on what IAM evaluates at
    /// apply time, so carina does not pre-judge IAM semantics here.
    pub fn all() -> impl Iterator<Item = ConditionOperator> {
        ConditionOperatorBase::ALL.iter().copied().flat_map(|base| {
            let mut spellings: Vec<ConditionOperator> =
                Vec::with_capacity(2 + 2 * ConditionQualifier::ALL.len());
            spellings.push(ConditionOperator {
                qualifier: None,
                base,
                if_exists: false,
            });
            spellings.push(ConditionOperator {
                qualifier: None,
                base,
                if_exists: true,
            });
            for &q in ConditionQualifier::ALL {
                spellings.push(ConditionOperator {
                    qualifier: Some(q),
                    base,
                    if_exists: false,
                });
                spellings.push(ConditionOperator {
                    qualifier: Some(q),
                    base,
                    if_exists: true,
                });
            }
            spellings
        })
    }
}

impl std::fmt::Display for ConditionOperator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_aws())
    }
}

/// Every snake_case condition-operator spelling the type system admits.
///
/// Delegates to [`ConditionOperator::all`] so the schema's `StringEnum`
/// values and the validator's suggestion list both flow from the same
/// type-level source; the schema cannot drift from `ConditionOperator::from_snake`.
fn all_condition_operator_snake_forms() -> Vec<String> {
    ConditionOperator::all().map(|op| op.to_snake()).collect()
}

/// Convert a snake_case condition operator to its PascalCase AWS form.
/// Returns `None` if the operator is unknown.
///
/// Thin string-boundary wrapper over [`ConditionOperator::from_snake`] +
/// [`ConditionOperator::to_aws`] so callers that thread `&str` through
/// `unwrap_or_else(|| k.clone())` keep working. Prefer the typed API
/// inside this crate.
pub fn condition_operator_to_aws(snake: &str) -> Option<String> {
    ConditionOperator::from_snake(snake).map(|op| op.to_aws())
}

/// Convert a PascalCase AWS condition operator to snake_case DSL form.
/// Returns `None` if the operator is unknown.
///
/// Thin string-boundary wrapper over [`ConditionOperator::from_aws`] +
/// [`ConditionOperator::to_snake`].
pub fn condition_operator_to_snake(pascal: &str) -> Option<String> {
    ConditionOperator::from_aws(pascal).map(|op| op.to_snake())
}

/// Check if a string is a valid snake_case condition operator.
pub fn is_valid_condition_operator(key: &str) -> bool {
    ConditionOperator::from_snake(key).is_some()
}

/// Validate condition operators in a parsed IAM policy document.
///
/// Walks the document looking for `condition` maps and validates that
/// all operator keys are valid snake_case condition operators.
pub fn validate_condition_operators(value: &Value) -> Result<(), String> {
    let Value::Concrete(ConcreteValue::Map(doc)) = value else {
        return Ok(());
    };
    // Look for "statement" list
    let Some(Value::Concrete(ConcreteValue::List(statements))) = doc.get("statement") else {
        return Ok(());
    };
    for (i, stmt) in statements.iter().enumerate() {
        let Value::Concrete(ConcreteValue::Map(stmt_map)) = stmt else {
            continue;
        };
        let Some(Value::Concrete(ConcreteValue::Map(condition))) = stmt_map.get("condition") else {
            continue;
        };
        for key in condition.keys() {
            if !is_valid_condition_operator(key) {
                let valid_operators: Vec<&'static str> = ConditionOperatorBase::ALL
                    .iter()
                    .map(|b| b.snake())
                    .collect();
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
            t.validate(&Value::Concrete(ConcreteValue::String(
                "arn:aws:s3:::my-bucket".to_string()
            )))
            .is_ok()
        );
        assert!(
            t.validate(&Value::Concrete(ConcreteValue::String(
                "not-an-arn".to_string()
            )))
            .is_err()
        );
        assert!(
            t.validate(&Value::Concrete(ConcreteValue::Int(42)))
                .is_err()
        );
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
            t.validate(&Value::Concrete(ConcreteValue::String(
                "vpc-1a2b3c4d".to_string()
            )))
            .is_ok()
        );
        assert!(
            t.validate(&Value::Concrete(ConcreteValue::String("vpc".to_string())))
                .is_err()
        );
        assert!(
            t.validate(&Value::Concrete(ConcreteValue::Int(42)))
                .is_err()
        );
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
            t.validate(&Value::Concrete(ConcreteValue::String(
                "vpc-cidr-assoc-12345678".to_string()
            )))
            .is_ok()
        );
        assert!(
            t.validate(&Value::Concrete(ConcreteValue::String(
                "vpc-cidr-assoc-0123456789abcdef0".to_string()
            )))
            .is_ok()
        );
    }

    #[test]
    fn validate_vpc_cidr_block_association_id_invalid() {
        let t = vpc_cidr_block_association_id();
        assert!(
            t.validate(&Value::Concrete(ConcreteValue::String(
                "vpc-12345678".to_string()
            )))
            .is_err()
        );
        assert!(
            t.validate(&Value::Concrete(ConcreteValue::String(
                "invalid".to_string()
            )))
            .is_err()
        );
    }

    #[test]
    fn validate_tgw_route_table_id_valid() {
        let t = tgw_route_table_id();
        assert!(
            t.validate(&Value::Concrete(ConcreteValue::String(
                "tgw-rtb-12345678".to_string()
            )))
            .is_ok()
        );
        assert!(
            t.validate(&Value::Concrete(ConcreteValue::String(
                "tgw-rtb-0123456789abcdef0".to_string()
            )))
            .is_ok()
        );
    }

    #[test]
    fn validate_tgw_route_table_id_invalid() {
        let t = tgw_route_table_id();
        // Regular route table ID should fail
        assert!(
            t.validate(&Value::Concrete(ConcreteValue::String(
                "rtb-12345678".to_string()
            )))
            .is_err()
        );
        // Transit gateway ID should fail
        assert!(
            t.validate(&Value::Concrete(ConcreteValue::String(
                "tgw-12345678".to_string()
            )))
            .is_err()
        );
        assert!(
            t.validate(&Value::Concrete(ConcreteValue::String(
                "invalid".to_string()
            )))
            .is_err()
        );
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
        assert!(
            t.validate(&Value::Concrete(ConcreteValue::String(
                "use1-az1".to_string()
            )))
            .is_ok()
        );
        assert!(
            t.validate(&Value::Concrete(ConcreteValue::String(
                "us-east-1a".to_string()
            )))
            .is_err()
        );
        assert!(
            t.validate(&Value::Concrete(ConcreteValue::Int(42)))
                .is_err()
        );
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
        let doc = Value::Concrete(ConcreteValue::Map(
            vec![
                (
                    "version".to_string(),
                    Value::Concrete(ConcreteValue::EnumIdentifier("2012_10_17".to_string())),
                ),
                (
                    "statement".to_string(),
                    Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                        ConcreteValue::Map(
                            vec![
                                (
                                    "effect".to_string(),
                                    Value::Concrete(ConcreteValue::EnumIdentifier(
                                        "allow".to_string(),
                                    )),
                                ),
                                (
                                    "action".to_string(),
                                    Value::Concrete(ConcreteValue::String(
                                        "sts:AssumeRole".to_string(),
                                    )),
                                ),
                                (
                                    "resource".to_string(),
                                    Value::Concrete(ConcreteValue::String("*".to_string())),
                                ),
                            ]
                            .into_iter()
                            .collect(),
                        ),
                    )])),
                ),
            ]
            .into_iter()
            .collect(),
        ));
        assert!(validate_iam_policy_document(&doc).is_ok());
    }

    #[test]
    fn validate_iam_policy_document_invalid_version() {
        let doc = Value::Concrete(ConcreteValue::Map(
            vec![(
                "version".to_string(),
                Value::Concrete(ConcreteValue::EnumIdentifier("2020_01_01".to_string())),
            )]
            .into_iter()
            .collect(),
        ));
        assert!(validate_iam_policy_document(&doc).is_err());
    }

    #[test]
    fn validate_iam_policy_document_invalid_effect() {
        let doc = Value::Concrete(ConcreteValue::Map(
            vec![(
                "statement".to_string(),
                Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                    ConcreteValue::Map(
                        vec![(
                            "effect".to_string(),
                            Value::Concrete(ConcreteValue::EnumIdentifier("grant".to_string())),
                        )]
                        .into_iter()
                        .collect(),
                    ),
                )])),
            )]
            .into_iter()
            .collect(),
        ));
        assert!(validate_iam_policy_document(&doc).is_err());
    }

    #[test]
    fn iam_policy_document_type_validates() {
        let t = iam_policy_document();
        let valid_doc = Value::Concrete(ConcreteValue::Map(
            vec![
                (
                    "version".to_string(),
                    Value::Concrete(ConcreteValue::EnumIdentifier("2012_10_17".to_string())),
                ),
                (
                    "statement".to_string(),
                    Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                        ConcreteValue::Map(
                            vec![
                                (
                                    "effect".to_string(),
                                    Value::Concrete(ConcreteValue::EnumIdentifier(
                                        "deny".to_string(),
                                    )),
                                ),
                                (
                                    "action".to_string(),
                                    Value::Concrete(ConcreteValue::String("s3:*".to_string())),
                                ),
                                (
                                    "resource".to_string(),
                                    Value::Concrete(ConcreteValue::String("*".to_string())),
                                ),
                            ]
                            .into_iter()
                            .collect(),
                        ),
                    )])),
                ),
            ]
            .into_iter()
            .collect(),
        ));
        assert!(t.validate(&valid_doc).is_ok());
    }

    #[test]
    fn iam_policy_document_principal_map_validates() {
        let t = iam_policy_document();
        // principal as a map: { service = "ec2.amazonaws.com" }
        let doc_with_principal_map = Value::Concrete(ConcreteValue::Map(
            vec![
                (
                    "version".to_string(),
                    Value::Concrete(ConcreteValue::EnumIdentifier("2012_10_17".to_string())),
                ),
                (
                    "statement".to_string(),
                    Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                        ConcreteValue::Map(
                            vec![
                                (
                                    "effect".to_string(),
                                    Value::Concrete(ConcreteValue::EnumIdentifier(
                                        "allow".to_string(),
                                    )),
                                ),
                                (
                                    "principal".to_string(),
                                    Value::Concrete(ConcreteValue::Map(
                                        vec![(
                                            "service".to_string(),
                                            Value::Concrete(ConcreteValue::String(
                                                "ec2.amazonaws.com".to_string(),
                                            )),
                                        )]
                                        .into_iter()
                                        .collect(),
                                    )),
                                ),
                                (
                                    "action".to_string(),
                                    Value::Concrete(ConcreteValue::String(
                                        "sts:AssumeRole".to_string(),
                                    )),
                                ),
                            ]
                            .into_iter()
                            .collect(),
                        ),
                    )])),
                ),
            ]
            .into_iter()
            .collect(),
        ));
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
        let doc_with_principal_string = Value::Concrete(ConcreteValue::Map(
            vec![
                (
                    "version".to_string(),
                    Value::Concrete(ConcreteValue::EnumIdentifier("2012_10_17".to_string())),
                ),
                (
                    "statement".to_string(),
                    Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                        ConcreteValue::Map(
                            vec![
                                (
                                    "effect".to_string(),
                                    Value::Concrete(ConcreteValue::EnumIdentifier(
                                        "allow".to_string(),
                                    )),
                                ),
                                (
                                    "principal".to_string(),
                                    Value::Concrete(ConcreteValue::String("*".to_string())),
                                ),
                                (
                                    "action".to_string(),
                                    Value::Concrete(ConcreteValue::String(
                                        "sts:AssumeRole".to_string(),
                                    )),
                                ),
                            ]
                            .into_iter()
                            .collect(),
                        ),
                    )])),
                ),
            ]
            .into_iter()
            .collect(),
        ));
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
            Value::Concrete(ConcreteValue::Map(
                [
                    (
                        "key".to_string(),
                        Value::Concrete(ConcreteValue::String("Project".to_string())),
                    ),
                    (
                        "value".to_string(),
                        Value::Concrete(ConcreteValue::String("carina".to_string())),
                    ),
                ]
                .into_iter()
                .collect(),
            )),
        );
        assert!(validate_tags_map(&attrs).is_err());
    }

    #[test]
    fn validate_tags_map_case_insensitive() {
        let mut attrs = std::collections::HashMap::new();
        attrs.insert(
            "tags".to_string(),
            Value::Concrete(ConcreteValue::Map(
                [
                    (
                        "Key".to_string(),
                        Value::Concrete(ConcreteValue::String("Project".to_string())),
                    ),
                    (
                        "Value".to_string(),
                        Value::Concrete(ConcreteValue::String("carina".to_string())),
                    ),
                ]
                .into_iter()
                .collect(),
            )),
        );
        assert!(validate_tags_map(&attrs).is_err());
    }

    #[test]
    fn validate_tags_map_normal_tags_ok() {
        let mut attrs = std::collections::HashMap::new();
        attrs.insert(
            "tags".to_string(),
            Value::Concrete(ConcreteValue::Map(
                [
                    (
                        "Project".to_string(),
                        Value::Concrete(ConcreteValue::String("carina".to_string())),
                    ),
                    (
                        "ManagedBy".to_string(),
                        Value::Concrete(ConcreteValue::String("carina".to_string())),
                    ),
                ]
                .into_iter()
                .collect(),
            )),
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
    fn condition_operator_typed_roundtrip() {
        // Type-level guarantee: every constructible ConditionOperator round-trips
        // through both snake/AWS Display forms. The schema's StringEnum, the
        // validator's "valid operators" list, and the conversion wrappers all
        // flow from `ConditionOperator::all()`, so this assertion forecloses
        // drift between them.
        for op in ConditionOperator::all() {
            let snake = op.to_snake();
            let aws = op.to_aws();
            assert_eq!(
                ConditionOperator::from_snake(&snake),
                Some(op),
                "snake {snake:?} did not round-trip"
            );
            assert_eq!(
                ConditionOperator::from_aws(&aws),
                Some(op),
                "aws {aws:?} did not round-trip"
            );
            // `Display` is the canonical AWS-wire spelling.
            assert_eq!(op.to_string(), aws);
        }
        // Spot-check a representative spelling, including the qualifier+if_exists
        // combination that issue #340 unblocked.
        let op = ConditionOperator {
            qualifier: Some(ConditionQualifier::ForAllValues),
            base: ConditionOperatorBase::StringLike,
            if_exists: true,
        };
        assert_eq!(op.to_snake(), "for_all_values_string_like_if_exists");
        assert_eq!(op.to_aws(), "ForAllValues:StringLikeIfExists");
    }

    #[test]
    fn condition_operator_total_count_matches_cross_product() {
        // 27 base × 3 qualifier options (None, ForAllValues, ForAnyValue) × 2
        // if_exists options = 162. If a new base operator is added the count
        // moves in lockstep, which is the property we want.
        let n_base = ConditionOperatorBase::ALL.len();
        let n_qualifier_options = 1 + ConditionQualifier::ALL.len();
        let expected = n_base * n_qualifier_options * 2;
        assert_eq!(ConditionOperator::all().count(), expected);
    }

    #[test]
    fn condition_type_string_enum_includes_qualifier_and_if_exists_variants() {
        // The schema's StringEnum values must enumerate every snake_case spelling
        // that `condition_operator_to_aws` accepts — base, qualifier-prefixed,
        // `_if_exists` suffixed, and the combination — so that `validate` does
        // not reject inputs that the conversion layer already handles.
        let cond = condition_type();
        let AttributeType::Map { key, .. } = cond else {
            panic!("condition_type() should be a Map");
        };
        let AttributeType::StringEnum { values, .. } = *key else {
            panic!("condition_type() key should be a StringEnum");
        };
        for expected in [
            "string_equals",
            "for_all_values_string_equals",
            "for_any_value_string_like",
            "string_equals_if_exists",
            "for_all_values_string_like_if_exists",
            "for_any_value_arn_like_if_exists",
            "null",
        ] {
            assert!(
                values.iter().any(|v| v == expected),
                "ConditionOperator StringEnum should include {expected:?}; got {values:?}"
            );
        }
        // Sanity: every value in the schema must round-trip through the
        // conversion layer, otherwise the two stay in drift.
        for v in &values {
            assert!(
                condition_operator_to_aws(v).is_some(),
                "schema value {v:?} not accepted by condition_operator_to_aws"
            );
        }
    }

    #[test]
    fn validate_condition_operators_accepts_valid() {
        let doc = Value::Concrete(ConcreteValue::Map(
            vec![(
                "statement".to_string(),
                Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                    ConcreteValue::Map(
                        vec![(
                            "condition".to_string(),
                            Value::Concrete(ConcreteValue::Map(
                                vec![(
                                    "string_equals".to_string(),
                                    Value::Concrete(ConcreteValue::Map(
                                        vec![(
                                            "aws:RequestedRegion".to_string(),
                                            Value::Concrete(ConcreteValue::String(
                                                "us-east-1".to_string(),
                                            )),
                                        )]
                                        .into_iter()
                                        .collect(),
                                    )),
                                )]
                                .into_iter()
                                .collect(),
                            )),
                        )]
                        .into_iter()
                        .collect(),
                    ),
                )])),
            )]
            .into_iter()
            .collect(),
        ));
        assert!(validate_condition_operators(&doc).is_ok());
    }

    #[test]
    fn validate_condition_operators_rejects_pascal_case() {
        let doc = Value::Concrete(ConcreteValue::Map(
            vec![(
                "statement".to_string(),
                Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                    ConcreteValue::Map(
                        vec![(
                            "condition".to_string(),
                            Value::Concrete(ConcreteValue::Map(
                                vec![(
                                    "StringEquals".to_string(),
                                    Value::Concrete(ConcreteValue::Map(indexmap::IndexMap::new())),
                                )]
                                .into_iter()
                                .collect(),
                            )),
                        )]
                        .into_iter()
                        .collect(),
                    ),
                )])),
            )]
            .into_iter()
            .collect(),
        ));
        let err = validate_condition_operators(&doc).unwrap_err();
        assert!(
            err.contains("StringEquals"),
            "Error should mention the invalid key: {err}"
        );
    }

    #[test]
    fn validate_condition_operators_rejects_unknown() {
        let doc = Value::Concrete(ConcreteValue::Map(
            vec![(
                "statement".to_string(),
                Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                    ConcreteValue::Map(
                        vec![(
                            "condition".to_string(),
                            Value::Concrete(ConcreteValue::Map(
                                vec![(
                                    "foo_bar".to_string(),
                                    Value::Concrete(ConcreteValue::Map(indexmap::IndexMap::new())),
                                )]
                                .into_iter()
                                .collect(),
                            )),
                        )]
                        .into_iter()
                        .collect(),
                    ),
                )])),
            )]
            .into_iter()
            .collect(),
        ));
        assert!(validate_condition_operators(&doc).is_err());
    }

    #[test]
    fn validate_condition_operators_accepts_if_exists() {
        let doc = Value::Concrete(ConcreteValue::Map(
            vec![(
                "statement".to_string(),
                Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                    ConcreteValue::Map(
                        vec![(
                            "condition".to_string(),
                            Value::Concrete(ConcreteValue::Map(
                                vec![(
                                    "string_equals_if_exists".to_string(),
                                    Value::Concrete(ConcreteValue::Map(indexmap::IndexMap::new())),
                                )]
                                .into_iter()
                                .collect(),
                            )),
                        )]
                        .into_iter()
                        .collect(),
                    ),
                )])),
            )]
            .into_iter()
            .collect(),
        ));
        assert!(validate_condition_operators(&doc).is_ok());
    }

    #[test]
    fn validate_iam_policy_document_accepts_for_all_values_string_equals() {
        // Regression for the route53 cross-account delegation writer use case
        // (issue #340): `for_all_values_string_equals` is the only way to narrow
        // `route53:ChangeResourceRecordSets` to a specific record-name / type /
        // action set. The conversion layer accepts it; the schema must too.
        let condition_inner = Value::Concrete(ConcreteValue::Map(
            vec![(
                "route53:ChangeResourceRecordSetsRecordTypes".to_string(),
                Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                    ConcreteValue::String("NS".to_string()),
                )])),
            )]
            .into_iter()
            .collect(),
        ));
        let statement = Value::Concrete(ConcreteValue::Map(
            vec![
                (
                    "effect".to_string(),
                    Value::Concrete(ConcreteValue::EnumIdentifier("allow".to_string())),
                ),
                (
                    "action".to_string(),
                    Value::Concrete(ConcreteValue::String(
                        "route53:ChangeResourceRecordSets".to_string(),
                    )),
                ),
                (
                    "resource".to_string(),
                    Value::Concrete(ConcreteValue::String(
                        "arn:aws:route53:::hostedzone/ABC".to_string(),
                    )),
                ),
                (
                    "condition".to_string(),
                    Value::Concrete(ConcreteValue::Map(
                        vec![("for_all_values_string_equals".to_string(), condition_inner)]
                            .into_iter()
                            .collect(),
                    )),
                ),
            ]
            .into_iter()
            .collect(),
        ));
        let doc = Value::Concrete(ConcreteValue::Map(
            vec![
                (
                    "version".to_string(),
                    Value::Concrete(ConcreteValue::EnumIdentifier("2012_10_17".to_string())),
                ),
                (
                    "statement".to_string(),
                    Value::Concrete(ConcreteValue::List(vec![statement])),
                ),
            ]
            .into_iter()
            .collect(),
        ));
        validate_iam_policy_document(&doc).expect(
            "for_all_values_string_equals must validate (schema must match conversion layer)",
        );
    }

    #[test]
    fn iam_policy_effect_is_string_enum() {
        let effect = super::iam_policy_effect();
        match effect {
            AttributeType::StringEnum {
                name,
                values,
                namespace,
                dsl_aliases,
            } => {
                assert_eq!(name, "Effect");
                assert_eq!(values, vec!["Allow".to_string(), "Deny".to_string()]);
                assert_eq!(namespace, Some("aws.iam.PolicyDocument".to_string()));
                assert_eq!(
                    dsl_aliases,
                    vec![
                        ("Allow".to_string(), "allow".to_string()),
                        ("Deny".to_string(), "deny".to_string()),
                    ]
                );
            }
            other => panic!("expected StringEnum, got {:?}", other),
        }
    }

    #[test]
    fn iam_policy_version_is_string_enum() {
        let version = super::iam_policy_version();
        match version {
            AttributeType::StringEnum {
                name,
                values,
                namespace,
                dsl_aliases,
            } => {
                assert_eq!(name, "Version");
                assert_eq!(
                    values,
                    vec!["2012-10-17".to_string(), "2008-10-17".to_string()]
                );
                assert_eq!(namespace, Some("aws.iam.PolicyDocument".to_string()));
                assert_eq!(
                    dsl_aliases,
                    vec![
                        ("2012-10-17".to_string(), "2012_10_17".to_string()),
                        ("2008-10-17".to_string(), "2008_10_17".to_string()),
                    ]
                );
            }
            other => panic!("expected StringEnum, got {:?}", other),
        }
    }
}
