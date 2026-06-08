use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{AttributeType, legacy_validator};

use crate::{aws_resource_id, provider_type};

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
    AttributeType::custom(
        Some(provider_type("ec2", "IpamPool", "Id")),
        aws_resource_id(),
        Some("^ipam-pool-[0-9a-f]{8,}$".to_string()),
        None,
        legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                validate_ipam_pool_id(s)
                    .map_err(|reason| format!("Invalid IPAM Pool ID '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        }),
        None,
    )
}
