use carina_core::schema::AttributeType;

use crate::provider_type;

/// Validate an IdentityStore id (`d-<10-hex>` or a 36-char UUID).
pub fn validate_identity_store_id(id: &str) -> Result<(), String> {
    let looks_like_d = id
        .strip_prefix("d-")
        .is_some_and(|rest| rest.len() == 10 && rest.chars().all(|c| c.is_ascii_hexdigit()));
    let looks_like_uuid = id.len() == 36
        && id.chars().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        });
    if looks_like_d || looks_like_uuid {
        Ok(())
    } else {
        Err("must be d-<10 hex> or a 36-char UUID".to_string())
    }
}

/// IdentityStore identity store id (`d-...` or UUID).
pub fn identity_store_id() -> AttributeType {
    AttributeType::refined_string(
        Some(provider_type("identitystore", "Store", "Id")),
        Some("^(d-[0-9a-fA-F]{10}|[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})$".to_string()),
        None,
        None,
    )
}
