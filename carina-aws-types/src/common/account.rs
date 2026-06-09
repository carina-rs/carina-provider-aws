use carina_core::schema::AttributeType;

use super::provider_bare_type;

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
    AttributeType::refined_string(
        Some(provider_bare_type(&[], "AccountId")),
        Some("^\\d{12}$".to_string()),
        Some((Some(12), Some(12))),
        None,
    )
}
