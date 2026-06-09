use carina_core::schema::AttributeType;

use super::provider_bare_type;

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
    AttributeType::refined_string(
        Some(provider_bare_type(&[], "ResourceId")),
        Some("^[a-z-]+-[0-9a-f]{8,}$".to_string()),
        None,
        None,
    )
}
