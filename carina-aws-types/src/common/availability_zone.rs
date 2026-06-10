use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{AttributeType, DslTransform, legacy_validator};
use carina_core::utils::validate_enum_namespace;

use super::provider_bare_type;

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

fn strip_availability_zone_prefix(value: &str) -> &str {
    value
        .strip_prefix("aws.AvailabilityZone.ZoneName.")
        .or_else(|| value.strip_prefix("ZoneName."))
        .unwrap_or(value)
}

/// Availability zone type with validation (e.g., "us-east-1a")
/// Accepts:
/// - DSL format: aws.AvailabilityZone.ZoneName.us_east_1a
/// - AWS string format: "us-east-1a"
/// - Shorthand: us_east_1a
pub fn availability_zone() -> AttributeType {
    AttributeType::enum_(
        provider_bare_type(&["AvailabilityZone"], "ZoneName"),
        None,
        vec![],
        Some(legacy_validator(|value| {
            if let Value::Concrete(ConcreteValue::String(s)) = value {
                let id = provider_bare_type(&["AvailabilityZone"], "ZoneName");
                validate_enum_namespace(s, &id)
                    .map_err(|reason| format!("Invalid availability zone '{}': {}", s, reason))?;
                let extracted = strip_availability_zone_prefix(s);
                let normalized = extracted.replace('_', "-");
                validate_availability_zone(&normalized)
                    .map_err(|reason| format!("Invalid availability zone '{}': {}", s, reason))
            } else {
                Err("Expected string".to_string())
            }
        })),
        Some(DslTransform::HyphenToUnderscore),
    )
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
    AttributeType::refined_string(
        Some(provider_bare_type(&["AvailabilityZone"], "ZoneId")),
        Some("^[a-z]+[0-9]+-az[0-9]+$".to_string()),
        None,
        None,
    )
}
